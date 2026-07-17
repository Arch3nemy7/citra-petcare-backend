use chrono::{DateTime, Utc};
use sqlx::PgPool;

use super::dto::{ChangeSet, ChangesResponse};
use super::repo;
use crate::error::AppError;

/// Assemble the change feed for every synced entity.
///
/// The queries run sequentially: at clinic scale each is a fast indexed range
/// scan, and sequential code stays easy to follow. `server_time` is captured
/// first — and from the database clock, matching the clock that stamps
/// `updated_at` — so a row updated mid-pull is re-sent next pull instead of lost.
pub async fn changes(db: &PgPool, since: DateTime<Utc>) -> Result<ChangesResponse, AppError> {
    let server_time = repo::server_time(db).await?;

    Ok(ChangesResponse {
        since,
        server_time,
        owners: ChangeSet {
            upserts: repo::owners_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::owners_tombstones(db, since).await?,
        },
        patients: ChangeSet {
            upserts: repo::patients_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::patients_tombstones(db, since).await?,
        },
        visits: ChangeSet {
            upserts: repo::visits_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::visits_tombstones(db, since).await?,
        },
        visit_attachments: ChangeSet {
            upserts: repo::attachments_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::attachments_tombstones(db, since).await?,
        },
        vaccinations: ChangeSet {
            upserts: repo::vaccinations_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::vaccinations_tombstones(db, since).await?,
        },
        appointments: ChangeSet {
            upserts: repo::appointments_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::appointments_tombstones(db, since).await?,
        },
        inventory_items: ChangeSet {
            upserts: repo::items_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::items_tombstones(db, since).await?,
        },
        stock_movements: ChangeSet {
            upserts: repo::movements_changed(db, since)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
            tombstones: repo::movements_tombstones(db, since).await?,
        },
    })
}
