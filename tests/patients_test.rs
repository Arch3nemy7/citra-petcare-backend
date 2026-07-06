//! Patients CRUD: create (incl. client-generated ids), read, upsert, search,
//! soft delete, owner-delete guard, weight history.

mod common;

use axum::http::StatusCode;
use common::{create_owner, create_patient, request, spawn_logged_in};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn full_crud_with_client_generated_id_and_soft_delete() {
    let (app, token) = spawn_logged_in().await;
    let owner_id = create_owner(&app.router, &token, "Ratna Sari").await;

    // client-generated UUIDv7 (the offline-sync write pattern)
    let patient_id = Uuid::now_v7();
    let (status, body) = request(
        &app.router,
        "POST",
        "/api/v1/patients",
        Some(&token),
        Some(json!({
            "id": patient_id,
            "ownerId": owner_id,
            "name": "Mochi",
            "species": "CAT",
            "breed": "Domestic Short Hair",
            "sex": "FEMALE",
            "sterilized": true,
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["id"], patient_id.to_string());
    assert_eq!(body["ownerName"], "Ratna Sari");

    // duplicate client id → 409
    let (status, body) = request(
        &app.router,
        "POST",
        "/api/v1/patients",
        Some(&token),
        Some(json!({ "id": patient_id, "ownerId": owner_id, "name": "Clone", "species": "CAT" })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // read
    let (status, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/patients/{patient_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Mochi");

    // PUT is a full-representation idempotent upsert
    let update = json!({
        "ownerId": owner_id,
        "name": "Mochi Belang",
        "species": "CAT",
        "sex": "FEMALE",
        "sterilized": true,
        "alertNotes": "Agresif saat dipegang",
        "status": "ACTIVE",
    });
    let (status, body) = request(
        &app.router,
        "PUT",
        &format!("/api/v1/patients/{patient_id}"),
        Some(&token),
        Some(update.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], "Mochi Belang");
    assert_eq!(body["alertNotes"], "Agresif saat dipegang");

    // idempotent: same PUT again → same outcome
    let (status, body) = request(
        &app.router,
        "PUT",
        &format!("/api/v1/patients/{patient_id}"),
        Some(&token),
        Some(update),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["name"], "Mochi Belang");

    // search by name and filter by owner
    let (status, body) = request(
        &app.router,
        "GET",
        "/api/v1/patients?search=belang",
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["meta"]["hasMore"], false);

    let (_, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/patients?ownerId={owner_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(body["data"].as_array().unwrap().len(), 1);

    // owner cannot be deleted while a live patient references it
    let (status, body) = request(
        &app.router,
        "DELETE",
        &format!("/api/v1/owners/{owner_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");

    // soft-delete the patient
    let (status, _) = request(
        &app.router,
        "DELETE",
        &format!("/api/v1/patients/{patient_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // now 404s, disappears from lists, and the owner can be deleted
    let (status, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/patients/{patient_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["type"], "urn:citra-petcare:problem:not-found");

    let (_, body) = request(&app.router, "GET", "/api/v1/patients", Some(&token), None).await;
    assert_eq!(body["data"].as_array().unwrap().len(), 0);

    let (status, _) = request(
        &app.router,
        "DELETE",
        &format!("/api/v1/owners/{owner_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    // deleting again → 404 (soft delete is not repeatable)
    let (status, _) = request(
        &app.router,
        "DELETE",
        &format!("/api/v1/patients/{patient_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_invalid_payloads_with_field_errors() {
    let (app, token) = spawn_logged_in().await;
    let owner_id = create_owner(&app.router, &token, "Agus").await;

    // empty name
    let (status, body) = request(
        &app.router,
        "POST",
        "/api/v1/patients",
        Some(&token),
        Some(json!({ "ownerId": owner_id, "name": "", "species": "CAT" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body["errors"]["name"].is_array(), "{body}");

    // unknown species enum value → 400 malformed body
    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/patients",
        Some(&token),
        Some(json!({ "ownerId": owner_id, "name": "X", "species": "DRAGON" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // unknown owner → 422 via FK mapping
    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/patients",
        Some(&token),
        Some(json!({ "ownerId": Uuid::now_v7(), "name": "X", "species": "CAT" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn weight_history_is_derived_from_visits() {
    let (app, token) = spawn_logged_in().await;
    let owner_id = create_owner(&app.router, &token, "Dewi").await;
    let patient_id = create_patient(&app.router, &token, owner_id, "Bruno").await;

    let (_, me) = request(&app.router, "GET", "/api/v1/users/me", Some(&token), None).await;
    let vet_id = me["id"].as_str().unwrap();

    for (days_ago, weight) in [(30i64, 28.4f64), (7, 29.1)] {
        let visit_date = chrono::Utc::now() - chrono::Duration::days(days_ago);
        let (status, body) = request(
            &app.router,
            "POST",
            "/api/v1/visits",
            Some(&token),
            Some(json!({
                "patientId": patient_id,
                "vetId": vet_id,
                "visitDate": visit_date,
                "complaint": "Pemeriksaan rutin",
                "weightKg": weight,
            })),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    // a visit without weight must not appear in the series
    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/visits",
        Some(&token),
        Some(json!({
            "patientId": patient_id,
            "vetId": vet_id,
            "visitDate": chrono::Utc::now(),
            "complaint": "Konsultasi tanpa timbang",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/patients/{patient_id}/weight-history"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let points = body.as_array().unwrap();
    assert_eq!(points.len(), 2);
    // chronological: oldest first
    assert_eq!(points[0]["weightKg"], 28.4);
    assert_eq!(points[1]["weightKg"], 29.1);
}
