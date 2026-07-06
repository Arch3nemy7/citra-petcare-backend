//! Domain modules. Each follows the same layering:
//! `handlers.rs` (thin HTTP layer) → `service.rs` (business rules) →
//! `repo.rs` (all SQL), with `models.rs` for domain types and `dto.rs` for
//! request/response shapes. sqlx rows never cross the repo boundary.

pub mod appointments;
pub mod auth;
pub mod dashboard;
pub mod inventory;
pub mod notifications;
pub mod owners;
pub mod patients;
pub mod storage;
pub mod sync;
pub mod users;
pub mod vaccinations;
pub mod visits;
