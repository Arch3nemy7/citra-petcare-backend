use chrono::NaiveDate;
use uuid::Uuid;

use crate::domain::inventory::models::InventoryCategory;

/// Latest vaccination per (patient, vaccine) whose next dose is due before a
/// cutoff. Shared by the dashboard (14-day window) and the daily reminder
/// job (3-day window).
#[derive(Debug, Clone)]
pub struct VaccinationDue {
    pub vaccination_id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    /// None for detached pets ("Tanpa pemilik").
    pub owner_name: Option<String>,
    pub owner_phone: Option<String>,
    pub vaccine_name: String,
    /// None for due-only records — a dose that is scheduled/known to be due
    /// but was never administered here (no date to show).
    pub date_given: Option<NaiveDate>,
    pub next_due_date: NaiveDate,
}

/// Item at or below its minimum stock level.
#[derive(Debug, Clone)]
pub struct LowStockItem {
    pub item_id: Uuid,
    pub name: String,
    pub category: InventoryCategory,
    pub unit: String,
    pub min_stock: f64,
    pub current_stock: f64,
}

/// Item whose expiry date falls before a cutoff.
#[derive(Debug, Clone)]
pub struct ExpiringItem {
    pub item_id: Uuid,
    pub name: String,
    pub category: InventoryCategory,
    pub unit: String,
    pub expiry_date: NaiveDate,
}
