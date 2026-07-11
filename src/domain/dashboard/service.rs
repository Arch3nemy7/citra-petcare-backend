use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Asia::Jakarta;
use sqlx::PgPool;

use super::dto::{
    DashboardSummaryResponse, ExpiringItemResponse, LowStockResponse, VaccinationDueResponse,
};
use super::repo;
use crate::domain::appointments;
use crate::error::AppError;

/// Dashboard windows (the daily reminder job uses tighter ones).
const VACCINATION_WINDOW_DAYS: i64 = 14;
const EXPIRY_WINDOW_DAYS: i64 = 60;
/// Sanity cap for "today's appointments" — a 2-vet clinic never books more.
const MAX_TODAY_APPOINTMENTS: i64 = 200;

/// Today's date on the clinic's wall clock (WIB), not UTC — around midnight
/// they differ by 7 hours.
pub fn jakarta_today() -> NaiveDate {
    Utc::now().with_timezone(&Jakarta).date_naive()
}

/// UTC instants of a Jakarta calendar day's start and end (exclusive).
pub fn jakarta_day_bounds(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let midnight = date.and_hms_opt(0, 0, 0).expect("00:00:00 is always valid");
    let start = Jakarta
        .from_local_datetime(&midnight)
        .single()
        .expect("Asia/Jakarta has no DST, every local midnight exists exactly once")
        .with_timezone(&Utc);
    (start, start + Duration::days(1))
}

pub async fn summary(db: &PgPool) -> Result<DashboardSummaryResponse, AppError> {
    let today = jakarta_today();
    let (day_start, day_end) = jakarta_day_bounds(today);

    let today_appointments = appointments::repo::list(
        db,
        Some(day_start),
        Some(day_end),
        None,
        None,
        None,
        MAX_TODAY_APPOINTMENTS,
    )
    .await?;
    let vaccinations_due =
        repo::vaccinations_due(db, today + Duration::days(VACCINATION_WINDOW_DAYS)).await?;
    let low_stock = repo::low_stock(db).await?;
    let expiring = repo::expiring(db, today + Duration::days(EXPIRY_WINDOW_DAYS), None).await?;

    Ok(DashboardSummaryResponse {
        date: today,
        generated_at: Utc::now(),
        today_appointments: today_appointments.into_iter().map(Into::into).collect(),
        vaccinations_due: vaccinations_due
            .into_iter()
            .map(|v| {
                let days_until_due = (v.next_due_date - today).num_days();
                VaccinationDueResponse {
                    vaccination_id: v.vaccination_id,
                    patient_id: v.patient_id,
                    patient_name: v.patient_name,
                    patient_species: v.patient_species,
                    patient_photo_key: v.patient_photo_key,
                    owner_name: v.owner_name,
                    owner_phone: v.owner_phone,
                    vaccine_name: v.vaccine_name,
                    date_given: v.date_given,
                    next_due_date: v.next_due_date,
                    days_until_due,
                    overdue: days_until_due < 0,
                }
            })
            .collect(),
        low_stock: low_stock
            .into_iter()
            .map(|i| LowStockResponse {
                item_id: i.item_id,
                name: i.name,
                category: i.category,
                unit: i.unit,
                min_stock: i.min_stock,
                current_stock: i.current_stock,
            })
            .collect(),
        expiring_soon: expiring
            .into_iter()
            .map(|i| ExpiringItemResponse {
                item_id: i.item_id,
                name: i.name,
                category: i.category,
                unit: i.unit,
                days_left: (i.expiry_date - today).num_days(),
                expiry_date: i.expiry_date,
            })
            .collect(),
    })
}
