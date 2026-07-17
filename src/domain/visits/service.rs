use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::dto::{AttachmentRequest, VisitRequest};
use super::models::{AttachmentKind, Visit, VisitAttachment, VisitStockUsage, VisitType};
use super::repo;
use crate::error::AppError;
use crate::http::pagination::Paginated;

pub async fn list(
    db: &PgPool,
    patient_id: Option<Uuid>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    cursor: Option<Uuid>,
    limit: i64,
) -> Result<Paginated<Visit>, AppError> {
    let rows = repo::list(db, patient_id, from, to, cursor, limit).await?;
    Ok(Paginated::from_rows(rows, limit, |visit| visit.id))
}

pub async fn get(db: &PgPool, id: Uuid) -> Result<Visit, AppError> {
    repo::find(db, id).await?.ok_or(AppError::NotFound("visit"))
}

pub async fn get_detail(
    db: &PgPool,
    id: Uuid,
) -> Result<(Visit, Vec<VisitAttachment>, Vec<VisitStockUsage>), AppError> {
    let visit = get(db, id).await?;
    let attachments = repo::attachments_for(db, id).await?;
    let stock_usage = repo::stock_usage_for(db, id).await?;
    Ok((visit, attachments, stock_usage))
}

pub async fn create(db: &PgPool, input: &VisitRequest) -> Result<Visit, AppError> {
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::insert(db, id, input).await?;
    get(db, id).await
}

pub async fn upsert(db: &PgPool, id: Uuid, input: &VisitRequest) -> Result<Visit, AppError> {
    repo::upsert(db, id, input).await?;
    get(db, id).await
}

pub async fn delete(db: &PgPool, id: Uuid) -> Result<(), AppError> {
    if !repo::soft_delete_with_attachments(db, id).await? {
        return Err(AppError::NotFound("visit"));
    }
    Ok(())
}

pub async fn add_attachment(
    db: &PgPool,
    visit_id: Uuid,
    input: &AttachmentRequest,
) -> Result<VisitAttachment, AppError> {
    let visit = get(db, visit_id).await?; // 404 before a confusing FK error
    // A CONSENT attachment is the owner's signed approval for a procedure, so
    // it only belongs on the visit types that involve one.
    if input.kind == AttachmentKind::Consent
        && !matches!(visit.visit_type, VisitType::Sterilisasi | VisitType::Opname)
    {
        return Err(AppError::Unprocessable(
            "CONSENT attachments are only allowed on STERILISASI or OPNAME visits".into(),
        ));
    }
    let id = input.id.unwrap_or_else(Uuid::now_v7);
    repo::insert_attachment(db, id, visit_id, &input.file_key, input.kind).await
}

pub async fn remove_attachment(
    db: &PgPool,
    visit_id: Uuid,
    attachment_id: Uuid,
) -> Result<(), AppError> {
    if !repo::soft_delete_attachment(db, visit_id, attachment_id).await? {
        return Err(AppError::NotFound("attachment"));
    }
    Ok(())
}
