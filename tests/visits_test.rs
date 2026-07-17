//! Visits: the OPNAME visit type and CONSENT attachments (the owner's
//! signed letter of approval for anesthesia/surgery/inpatient care).

mod common;

use axum::http::StatusCode;
use common::{create_owner, create_patient, request, spawn_logged_in};
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn opname_visit_carries_a_consent_attachment() {
    let (app, token) = spawn_logged_in().await;
    let owner_id = create_owner(&app.router, &token, "Sari Wijaya").await;
    let patient_id = create_patient(&app.router, &token, owner_id, "Milo").await;

    let (_, me) = request(&app.router, "GET", "/api/v1/users/me", Some(&token), None).await;
    let vet_id = me["id"].as_str().unwrap();

    // an OPNAME visit round-trips through create and detail
    let (status, body) = request(
        &app.router,
        "POST",
        "/api/v1/visits",
        Some(&token),
        Some(json!({
            "patientId": patient_id,
            "vetId": vet_id,
            "visitType": "OPNAME",
            "visitDate": chrono::Utc::now(),
            "complaint": "Rawat inap pasca operasi",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["visitType"], "OPNAME");
    let visit_id = body["id"].as_str().unwrap().to_string();

    // the signed letter of approval is attached with kind CONSENT
    let (status, body) = request(
        &app.router,
        "POST",
        &format!("/api/v1/visits/{visit_id}/attachments"),
        Some(&token),
        Some(json!({ "fileKey": "visits/consent-1.jpg", "kind": "CONSENT" })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["kind"], "CONSENT");
    let attachment_id = body["id"].as_str().unwrap().to_string();

    // detail returns the visit type and the consent attachment
    let (status, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/visits/{visit_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["visitType"], "OPNAME");
    let attachments = body["attachments"].as_array().unwrap();
    assert_eq!(attachments.len(), 1);
    assert_eq!(attachments[0]["kind"], "CONSENT");
    assert_eq!(attachments[0]["fileKey"], "visits/consent-1.jpg");

    // a mistaken consent photo can be detached again
    let (status, _) = request(
        &app.router,
        "DELETE",
        &format!("/api/v1/visits/{visit_id}/attachments/{attachment_id}"),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (_, body) = request(
        &app.router,
        "GET",
        &format!("/api/v1/visits/{visit_id}"),
        Some(&token),
        None,
    )
    .await;
    assert!(body["attachments"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn unknown_visit_type_is_rejected() {
    let (app, token) = spawn_logged_in().await;

    // the unknown variant is rejected while deserializing the body, before
    // any patient/vet lookup — random ids suffice
    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/visits",
        Some(&token),
        Some(json!({
            "patientId": Uuid::now_v7(),
            "vetId": Uuid::now_v7(),
            "visitType": "RAWAT_JALAN",
            "visitDate": chrono::Utc::now(),
            "complaint": "X",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // attachments on a visit that does not exist → 404
    let (status, _) = request(
        &app.router,
        "POST",
        &format!("/api/v1/visits/{}/attachments", Uuid::now_v7()),
        Some(&token),
        Some(json!({ "fileKey": "visits/x.jpg", "kind": "CONSENT" })),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
