//! Background job scheduling. One daily job at 07:00 Asia/Jakarta collects
//! everything the clinic should act on today — vaccinations due within 3
//! days (or overdue), low-stock items, drugs expiring within 30 days —
//! persists a notification row per category and pushes it via the Notifier.

use chrono::Duration;
use tokio_cron_scheduler::{Job, JobScheduler, JobSchedulerError};

use crate::domain::inventory::models::InventoryCategory;
use crate::domain::{auth, dashboard, notifications};
use crate::error::AppError;
use crate::state::AppState;

const VACCINATION_WINDOW_DAYS: i64 = 3;
const DRUG_EXPIRY_WINDOW_DAYS: i64 = 30;

/// Six-field cron (sec min hour dom mon dow): 07:00:00 every day, evaluated
/// on the Jakarta wall clock rather than UTC.
const DAILY_CRON: &str = "0 0 7 * * *";

pub async fn start(state: AppState) -> Result<JobScheduler, JobSchedulerError> {
    let scheduler = JobScheduler::new().await?;

    let job_state = state.clone();
    let job = Job::new_async_tz(DAILY_CRON, chrono_tz::Asia::Jakarta, move |_id, _lock| {
        // The closure fires once per day; each firing moves its own clone of
        // the (cheaply cloneable) AppState into the spawned async block.
        let state = job_state.clone();
        Box::pin(async move {
            tracing::info!("daily reminder job started");
            if let Err(error) = run_daily_reminders(&state).await {
                tracing::error!(%error, "daily reminder job failed");
            }
        })
    })?;

    scheduler.add(job).await?;
    scheduler.start().await?;
    tracing::info!(cron = DAILY_CRON, tz = "Asia/Jakarta", "scheduler started");
    Ok(scheduler)
}

/// The job body — public so tests (or an admin shell) can trigger it directly.
///
/// Each category dispatches at most once per Jakarta calendar day: if the
/// process restarts and the job re-fires, `exists_since` sees the earlier row
/// and skips the duplicate.
pub async fn run_daily_reminders(state: &AppState) -> Result<(), AppError> {
    let db = &state.db;
    let notifier = state.notifier.as_ref();
    let today = dashboard::service::jakarta_today();
    let (day_start, _) = dashboard::service::jakarta_day_bounds(today);

    // --- vaccinations due within 3 days or overdue ---
    let due =
        dashboard::repo::vaccinations_due(db, today + Duration::days(VACCINATION_WINDOW_DAYS))
            .await?;
    if !due.is_empty()
        && !notifications::repo::exists_since(db, "VACCINATION_DUE", day_start).await?
    {
        let payload = serde_json::json!({
            "items": due.iter().map(|v| serde_json::json!({
                "vaccinationId": v.vaccination_id,
                "patientId": v.patient_id,
                "patientName": v.patient_name,
                "ownerName": v.owner_name,
                "ownerPhone": v.owner_phone,
                "vaccineName": v.vaccine_name,
                "nextDueDate": v.next_due_date,
            })).collect::<Vec<_>>(),
        });
        notifications::service::create_and_dispatch(
            db,
            notifier,
            "VACCINATION_DUE",
            "Pengingat vaksinasi".to_string(),
            format!("{} pasien jatuh tempo vaksinasi dalam {VACCINATION_WINDOW_DAYS} hari ke depan atau sudah terlewat.", due.len()),
            payload,
        )
        .await?;
    }

    // --- items at/below minimum stock ---
    let low = dashboard::repo::low_stock(db).await?;
    if !low.is_empty() && !notifications::repo::exists_since(db, "LOW_STOCK", day_start).await? {
        let payload = serde_json::json!({
            "items": low.iter().map(|i| serde_json::json!({
                "itemId": i.item_id,
                "name": i.name,
                "currentStock": i.current_stock,
                "minStock": i.min_stock,
                "unit": i.unit,
            })).collect::<Vec<_>>(),
        });
        notifications::service::create_and_dispatch(
            db,
            notifier,
            "LOW_STOCK",
            "Stok menipis".to_string(),
            format!("{} item berada di bawah batas stok minimum.", low.len()),
            payload,
        )
        .await?;
    }

    // --- drugs expiring within 30 days ---
    let expiring = dashboard::repo::expiring(
        db,
        today + Duration::days(DRUG_EXPIRY_WINDOW_DAYS),
        Some(InventoryCategory::Drug),
    )
    .await?;
    if !expiring.is_empty()
        && !notifications::repo::exists_since(db, "EXPIRY_WARNING", day_start).await?
    {
        let payload = serde_json::json!({
            "items": expiring.iter().map(|i| serde_json::json!({
                "itemId": i.item_id,
                "name": i.name,
                "expiryDate": i.expiry_date,
            })).collect::<Vec<_>>(),
        });
        notifications::service::create_and_dispatch(
            db,
            notifier,
            "EXPIRY_WARNING",
            "Obat mendekati kedaluwarsa".to_string(),
            format!(
                "{} obat akan kedaluwarsa dalam {DRUG_EXPIRY_WINDOW_DAYS} hari.",
                expiring.len()
            ),
            payload,
        )
        .await?;
    }

    // --- housekeeping: drop long-expired refresh tokens ---
    let pruned = auth::repo::prune_expired(db).await?;
    if pruned > 0 {
        tracing::debug!(pruned, "pruned expired refresh tokens");
    }

    tracing::info!(
        vaccinations_due = due.len(),
        low_stock = low.len(),
        expiring_drugs = expiring.len(),
        "daily reminder job finished"
    );
    Ok(())
}
