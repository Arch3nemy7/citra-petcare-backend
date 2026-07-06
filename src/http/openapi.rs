//! OpenAPI document assembly. Most operations register themselves through
//! `utoipa_axum::routes!` in each domain's `handlers::router()`; the few
//! that need manual registration (wildcard paths axum can route but OpenAPI
//! cannot express) are listed in `paths(...)` here.

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Citra PetCare API",
        description = "Internal REST API for the Citra PetCare veterinary clinic: \
                       patients & owners, visit records, vaccinations, appointments, \
                       inventory, dashboard, file storage and offline sync. \
                       All endpoints except /auth/login and /auth/refresh require a \
                       Bearer access token.",
    ),
    modifiers(&SecurityAddon),
    paths(crate::domain::storage::handlers::presign_download),
    tags(
        (name = "auth", description = "Login, refresh-token rotation, logout"),
        (name = "users", description = "Clinic staff accounts (seeded, no public registration)"),
        (name = "owners", description = "Pet owners"),
        (name = "patients", description = "Animal patients"),
        (name = "visits", description = "Consultation/exam records with file attachments"),
        (name = "vaccinations", description = "Vaccination history and due dates"),
        (name = "appointments", description = "Clinic agenda"),
        (name = "inventory", description = "Items and stock-movement ledger"),
        (name = "dashboard", description = "Home-screen summary"),
        (name = "notifications", description = "Reminder log produced by the daily scheduler"),
        (name = "storage", description = "Presigned upload/download URLs"),
        (name = "sync", description = "Offline-sync change feed"),
    )
)]
pub struct ApiDoc;

/// Registers the `bearerAuth` security scheme referenced by the
/// `security(("bearerAuth" = []))` annotations on protected operations.
pub struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearerAuth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some("Access token from POST /api/v1/auth/login"))
                    .build(),
            ),
        );
    }
}
