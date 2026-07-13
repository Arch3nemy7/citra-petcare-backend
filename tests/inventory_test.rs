//! Inventory: stock is derived from the movement ledger, OUT movements can
//! never drive it negative, sign rules are enforced per movement type, and
//! expiry-dated stock-ins open batches consumed earliest-expiry-first (FEFO).

mod common;

use axum::http::StatusCode;
use common::{request, spawn_logged_in};
use serde_json::json;

async fn create_item(router: &axum::Router, token: &str) -> String {
    let (status, body) = request(
        router,
        "POST",
        "/api/v1/inventory/items",
        Some(token),
        Some(json!({
            "name": "Amoxicillin Sirup 60 ml",
            "category": "DRUG",
            "unit": "botol",
            "minStock": 5,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["currentStock"], 0.0, "new item starts at zero stock");
    body["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn stock_is_derived_from_movements() {
    let (app, token) = spawn_logged_in().await;
    let item_id = create_item(&app.router, &token).await;
    let movements_uri = format!("/api/v1/inventory/items/{item_id}/movements");

    // IN 10 → 10
    let (status, body) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "IN", "qty": 10, "reason": "Pembelian" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["currentStock"], 10.0);

    // OUT 3 → 7
    let (_, body) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "OUT", "qty": 3 })),
    )
    .await;
    assert_eq!(body["currentStock"], 7.0);

    // ADJUSTMENT -2 → 5 (signed delta, e.g. breakage at stock opname)
    let (_, body) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "ADJUSTMENT", "qty": -2, "reason": "Stok opname" })),
    )
    .await;
    assert_eq!(body["currentStock"], 5.0);

    // the item read re-derives the same number from the ledger
    let (_, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/inventory/items/{item_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(body["currentStock"], 5.0);

    // ledger holds all three entries
    let (_, body) = request(&app.router, "GET", &movements_uri, Some(&token), None).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 3);
}

#[tokio::test]
async fn batches_follow_fefo_and_refresh_the_expiry_badge() {
    let (app, token) = spawn_logged_in().await;
    let item_id = create_item(&app.router, &token).await;
    let movements_uri = format!("/api/v1/inventory/items/{item_id}/movements");
    let item_uri = format!("/api/v1/inventory/items/{item_id}");

    // two lots: the later-received one expires sooner
    let (status, body) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({
            "type": "IN", "qty": 8,
            "expiryDate": "2027-01-14", "lotNo": "AMX-B",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["lotNo"], "AMX-B");
    let (_, _) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({
            "type": "IN", "qty": 6,
            "expiryDate": "2026-08-30", "lotNo": "AMX-A",
        })),
    )
    .await;

    // consumption drains the earliest-expiring lot first
    let (_, _) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "OUT", "qty": 4 })),
    )
    .await;

    let (status, body) = request(&app.router, "GET", &item_uri, Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["currentStock"], 10.0);
    // item expiry badge = earliest remaining batch
    assert_eq!(body["expiryDate"], "2026-08-30");
    let batches = body["batches"].as_array().unwrap();
    assert_eq!(batches.len(), 2, "{body}");
    assert_eq!(batches[0]["lotNo"], "AMX-A");
    assert_eq!(batches[0]["qtyRemaining"], 2.0);
    assert_eq!(batches[1]["lotNo"], "AMX-B");
    assert_eq!(batches[1]["qtyRemaining"], 8.0);

    // draining lot A entirely advances the badge to lot B's expiry
    let (_, _) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "OUT", "qty": 2 })),
    )
    .await;
    let (_, body) = request(&app.router, "GET", &item_uri, Some(&token), None).await;
    assert_eq!(body["expiryDate"], "2027-01-14");
    assert_eq!(body["batches"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn batch_expiry_can_be_corrected() {
    let (app, token) = spawn_logged_in().await;
    let item_id = create_item(&app.router, &token).await;
    let movements_uri = format!("/api/v1/inventory/items/{item_id}/movements");
    let batches_uri = format!("/api/v1/inventory/items/{item_id}/batches");

    // one lot-numbered batch, one without a lot number
    for payload in [
        json!({ "type": "IN", "qty": 6, "expiryDate": "2026-08-30", "lotNo": "AMX-A" }),
        json!({ "type": "IN", "qty": 8, "expiryDate": "2027-01-14" }),
    ] {
        let (status, body) = request(
            &app.router,
            "POST",
            &movements_uri,
            Some(&token),
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    // wrong key → 404 without touching anything: unknown date, mismatched
    // lot, missing lot on a lot-numbered batch
    for payload in [
        json!({ "expiryDate": "2026-01-01", "lotNo": "AMX-A", "newExpiryDate": "2026-09-15" }),
        json!({ "expiryDate": "2026-08-30", "lotNo": "AMX-X", "newExpiryDate": "2026-09-15" }),
        json!({ "expiryDate": "2026-08-30", "newExpiryDate": "2026-09-15" }),
    ] {
        let (status, body) = request(
            &app.router,
            "PATCH",
            &batches_uri,
            Some(&token),
            Some(payload),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");
    }

    // re-date the lot-numbered batch; the badge follows the corrected date
    let (status, body) = request(
        &app.router,
        "PATCH",
        &batches_uri,
        Some(&token),
        Some(json!({
            "expiryDate": "2026-08-30", "lotNo": "AMX-A",
            "newExpiryDate": "2026-09-15",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expiryDate"], "2026-09-15");
    let batches = body["batches"].as_array().unwrap();
    assert_eq!(batches.len(), 2, "{body}");
    assert_eq!(batches[0]["lotNo"], "AMX-A");
    assert_eq!(batches[0]["expiryDate"], "2026-09-15");
    assert_eq!(batches[0]["qtyRemaining"], 6.0);

    // re-dating past the other lot re-orders FEFO and moves the badge
    let (status, body) = request(
        &app.router,
        "PATCH",
        &batches_uri,
        Some(&token),
        Some(json!({
            "expiryDate": "2026-09-15", "lotNo": "AMX-A",
            "newExpiryDate": "2027-06-30",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expiryDate"], "2027-01-14");
    let batches = body["batches"].as_array().unwrap();
    assert_eq!(batches[0]["lotNo"], serde_json::Value::Null);
    assert_eq!(batches[1]["lotNo"], "AMX-A");

    // the lot-less batch is addressable by expiry date alone
    let (status, body) = request(
        &app.router,
        "PATCH",
        &batches_uri,
        Some(&token),
        Some(json!({ "expiryDate": "2027-01-14", "newExpiryDate": "2027-02-01" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["expiryDate"], "2027-02-01");
}

#[tokio::test]
async fn out_beyond_stock_is_rejected() {
    let (app, token) = spawn_logged_in().await;
    let item_id = create_item(&app.router, &token).await;
    let movements_uri = format!("/api/v1/inventory/items/{item_id}/movements");

    let (_, _) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "IN", "qty": 2 })),
    )
    .await;

    // OUT 5 with only 2 available → 422 insufficient-stock, nothing recorded
    let (status, body) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "OUT", "qty": 5 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["type"], "urn:citra-petcare:problem:insufficient-stock");

    let (_, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/inventory/items/{item_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(
        body["currentStock"], 2.0,
        "failed OUT must not change stock"
    );

    // an ADJUSTMENT below zero is rejected the same way
    let (status, _) = request(
        &app.router,
        "POST",
        &movements_uri,
        Some(&token),
        Some(json!({ "type": "ADJUSTMENT", "qty": -3 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn sign_rules_per_movement_type() {
    let (app, token) = spawn_logged_in().await;
    let item_id = create_item(&app.router, &token).await;
    let movements_uri = format!("/api/v1/inventory/items/{item_id}/movements");

    for payload in [
        json!({ "type": "IN", "qty": -5 }),
        json!({ "type": "OUT", "qty": 0 }),
        json!({ "type": "ADJUSTMENT", "qty": 0 }),
    ] {
        let (status, body) = request(
            &app.router,
            "POST",
            &movements_uri,
            Some(&token),
            Some(payload.clone()),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "payload {payload} → {body}"
        );
    }

    // unknown item → 404
    let (status, _) = request(
        &app.router,
        "POST",
        &format!("/api/v1/inventory/items/{}/movements", uuid::Uuid::now_v7()),
        Some(&token),
        Some(json!({ "type": "IN", "qty": 1 })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
