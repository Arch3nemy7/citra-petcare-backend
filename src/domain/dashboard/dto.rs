use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::domain::appointments::dto::AppointmentResponse;
use crate::domain::inventory::models::InventoryCategory;

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummaryResponse {
    /// Clinic-local date (Asia/Jakarta) the summary was computed for.
    pub date: NaiveDate,
    pub generated_at: DateTime<Utc>,
    /// All of today's appointments (any status), soonest first.
    pub today_appointments: Vec<AppointmentResponse>,
    /// Vaccinations due within 14 days, or overdue.
    pub vaccinations_due: Vec<VaccinationDueResponse>,
    /// Items at/below their minimum stock level.
    pub low_stock: Vec<LowStockResponse>,
    /// Items expiring within 60 days (any category).
    pub expiring_soon: Vec<ExpiringItemResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct VaccinationDueResponse {
    pub vaccination_id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    /// None for detached pets ("Tanpa pemilik").
    pub owner_name: Option<String>,
    pub owner_phone: Option<String>,
    pub vaccine_name: String,
    /// None for due-only records (dose known to be due, never given here).
    pub date_given: Option<NaiveDate>,
    pub next_due_date: NaiveDate,
    /// Negative when overdue.
    pub days_until_due: i64,
    pub overdue: bool,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LowStockResponse {
    pub item_id: Uuid,
    pub name: String,
    pub category: InventoryCategory,
    pub unit: String,
    pub min_stock: f64,
    pub current_stock: f64,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ExpiringItemResponse {
    pub item_id: Uuid,
    pub name: String,
    pub category: InventoryCategory,
    pub unit: String,
    pub expiry_date: NaiveDate,
    /// Negative when already expired.
    pub days_left: i64,
}
