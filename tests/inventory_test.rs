//! Inventory: stock is derived from the movement ledger, OUT movements can
//! never drive it negative, and sign rules are enforced per movement type.

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
