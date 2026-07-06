use chrono::{DateTime, NaiveDate, Utc};
use uuid::Uuid;

/// A vaccination event. `next_due_date` drives the reminder scheduler and the
/// dashboard's "due soon" list.
#[derive(Debug, Clone)]
pub struct Vaccination {
    pub id: Uuid,
    pub patient_id: Uuid,
    pub patient_name: String,
    /// Visit during which the vaccine was administered, when recorded.
    pub visit_id: Option<Uuid>,
    pub vaccine_name: String,
    pub date_given: NaiveDate,
    pub batch_no: Option<String>,
    pub next_due_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
