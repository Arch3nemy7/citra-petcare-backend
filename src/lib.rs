//! Citra PetCare — internal veterinary clinic record system API.
//!
//! `main.rs` is wiring only. Everything lives in this library crate so the
//! integration tests can build the exact same router against a throwaway
//! Postgres container.

pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod http;
pub mod scheduler;
pub mod seed;
pub mod state;
pub mod telemetry;
