//! Offline-sync change feed: upserts appear after their `updated_at`,
//! soft deletes surface as tombstones, and `serverTime` advances the cursor.

mod common;

use axum::http::StatusCode;
use common::{create_owner, create_patient, request, spawn_logged_in};

const EPOCH: &str = "1970-01-01T00:00:00Z";

#[tokio::test]
async fn changes_carry_upserts_then_tombstones() {
    let (app, token) = spawn_logged_in().await;
    let owner_id = create_owner(&app.router, &token, "Siti Rahayu").await;
    let patient_id = create_patient(&app.router, &token, owner_id, "Snowy").await;

    // full pull from the epoch
    let (status, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/sync/changes?since={EPOCH}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    let owner_upserts = body["owners"]["upserts"].as_array().unwrap();
    assert_eq!(owner_upserts.len(), 1);
    assert_eq!(owner_upserts[0]["id"], owner_id.to_string());
    let patient_upserts = body["patients"]["upserts"].as_array().unwrap();
    assert_eq!(patient_upserts.len(), 1);
    assert!(
        body["patients"]["tombstones"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    let checkpoint = body["serverTime"].as_str().unwrap().to_string();

    // no changes since the checkpoint → all sets empty
    let (_, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/sync/changes?since={checkpoint}"),
        Some(&token),
        None,
    )
    .await;
    for entity in [
        "owners",
        "patients",
        "visits",
        "appointments",
        "inventoryItems",
    ] {
        assert!(
            body[entity]["upserts"].as_array().unwrap().is_empty(),
            "{entity} should have no upserts: {body}"
        );
        assert!(body[entity]["tombstones"].as_array().unwrap().is_empty());
    }

    // soft-delete the patient → next incremental pull carries a tombstone only
    let (status, _) = request(
        &app.router,
        "DELETE",
        &format!("/api/v1/patients/{patient_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/sync/changes?since={checkpoint}"),
        Some(&token),
        None,
    )
    .await;
    let tombstones = body["patients"]["tombstones"].as_array().unwrap();
    assert_eq!(tombstones.len(), 1, "{body}");
    assert_eq!(tombstones[0]["id"], patient_id.to_string());
    assert!(tombstones[0]["deletedAt"].is_string());
    assert!(body["patients"]["upserts"].as_array().unwrap().is_empty());
    // the owner was untouched since the checkpoint
    assert!(body["owners"]["upserts"].as_array().unwrap().is_empty());

    // an update after the checkpoint reappears as an upsert (server-bumped updated_at)
    let (status, _) = request(
        &app.router,
        "PUT",
        &format!("/api/v1/owners/{owner_id}"),
        Some(&token),
        Some(serde_json::json!({ "name": "Siti Rahayu Baru", "phone": "+628123456789" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/sync/changes?since={checkpoint}"),
        Some(&token),
        None,
    )
    .await;
    let owner_upserts = body["owners"]["upserts"].as_array().unwrap();
    assert_eq!(owner_upserts.len(), 1);
    assert_eq!(owner_upserts[0]["name"], "Siti Rahayu Baru");
}

#[tokio::test]
async fn malformed_since_is_a_problem_response() {
    let (app, token) = spawn_logged_in().await;
    let (status, body) = request(
        &app.router,
        "GET",
        "/api/v1/sync/changes?since=yesterday",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["type"], "urn:citra-petcare:problem:bad-request");
}
