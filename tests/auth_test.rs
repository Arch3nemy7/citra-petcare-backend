//! Auth flow: login, guarded routes, refresh rotation with reuse detection,
//! logout revocation, and RFC 7807 error bodies.

mod common;

use axum::http::StatusCode;
use common::{TEST_PASSWORD, create_user, login, request, spawn_app};
use serde_json::json;

#[tokio::test]
async fn login_returns_tokens_and_guards_work() {
    let app = spawn_app().await;
    create_user(&app.db, "vet@test.id").await;

    // wrong password → opaque 401 problem
    let (status, body) = request(
        &app.router,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": "vet@test.id", "password": "wrong-password" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["type"], "urn:citra-petcare:problem:unauthorized");

    // unknown email → identical 401 (no user enumeration)
    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": "ghost@test.id", "password": TEST_PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // correct credentials
    let (access, _refresh) = login(&app.router, "vet@test.id").await;

    // guarded route without token → 401
    let (status, _) = request(&app.router, "GET", "/api/v1/users/me", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // with token → the profile
    let (status, body) = request(&app.router, "GET", "/api/v1/users/me", Some(&access), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["email"], "vet@test.id");
    assert_eq!(body["role"], "VET");

    // garbage token → 401
    let (status, _) = request(
        &app.router,
        "GET",
        "/api/v1/users/me",
        Some("not-a-jwt"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refresh_rotates_and_detects_reuse() {
    let app = spawn_app().await;
    create_user(&app.db, "vet@test.id").await;
    let (_access, refresh1) = login(&app.router, "vet@test.id").await;

    // rotate: refresh1 → refresh2
    let (status, body) = request(
        &app.router,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(json!({ "refreshToken": refresh1 })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let refresh2 = body["refreshToken"].as_str().unwrap().to_string();
    assert_ne!(refresh1, refresh2, "refresh token must rotate");

    // replaying the rotated (revoked) token → 401 and kills the family
    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(json!({ "refreshToken": refresh1 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // the *newer* token is now dead too (reuse ⇒ assume compromise)
    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(json!({ "refreshToken": refresh2 })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // a fresh login still works
    let (_, _) = login(&app.router, "vet@test.id").await;
}

#[tokio::test]
async fn logout_revokes_refresh_token() {
    let app = spawn_app().await;
    create_user(&app.db, "vet@test.id").await;
    let (access, refresh) = login(&app.router, "vet@test.id").await;

    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/auth/logout",
        Some(&access),
        Some(json!({ "refreshToken": refresh })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = request(
        &app.router,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(json!({ "refreshToken": refresh })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_validation_produces_field_errors() {
    let app = spawn_app().await;
    let (status, body) = request(
        &app.router,
        "POST",
        "/api/v1/auth/login",
        None,
        Some(json!({ "email": "not-an-email", "password": "x" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(body["type"], "urn:citra-petcare:problem:validation");
    assert!(
        body["errors"]["email"].is_array(),
        "email error missing: {body}"
    );
    assert!(
        body["errors"]["password"].is_array(),
        "password error missing: {body}"
    );
}
