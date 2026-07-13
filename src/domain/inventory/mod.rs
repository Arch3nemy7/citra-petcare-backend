pub mod dto;
pub mod handlers;
pub mod models;
pub mod repo;
pub mod service;

/// Inventory domain errors (mapped to 422 responses).
#[derive(Debug, thiserror::Error)]
pub enum InventoryError {
    #[error("insufficient stock: requested {requested}, available {available}")]
    InsufficientStock { requested: f64, available: f64 },
    #[error("{0}")]
    InvalidQuantity(String),
    #[error("this movement was recorded by a visit; correct it from the visit instead")]
    VisitLinkedMovement,
}
