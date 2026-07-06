//! Cursor pagination. List queries fetch `limit + 1` rows ordered by a keyset
//! (usually `id DESC`, where UUIDv7 ids are time-sortable); the extra row only
//! signals that another page exists. The cursor returned to clients is the
//! last row's id — opaque, stable, and immune to concurrent inserts shifting
//! offsets.

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

pub const DEFAULT_LIMIT: i64 = 20;
pub const MAX_LIMIT: i64 = 100;

pub fn clamp_limit(limit: Option<i64>) -> i64 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

/// Bare cursor+limit query params, for nested lists that need nothing else.
#[derive(Debug, Deserialize, IntoParams)]
#[serde(rename_all = "camelCase")]
#[into_params(parameter_in = Query)]
pub struct PageParams {
    /// Opaque cursor from `meta.nextCursor` of the previous page.
    pub cursor: Option<Uuid>,
    /// Page size 1–100 (default 20).
    pub limit: Option<i64>,
}

/// The `{ data, meta }` list envelope used by every collection endpoint.
#[derive(Debug, Serialize, ToSchema)]
pub struct Paginated<T> {
    pub data: Vec<T>,
    pub meta: PageMeta,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PageMeta {
    pub limit: i64,
    /// Pass as `cursor` to fetch the next page; absent on the last page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<Uuid>,
    pub has_more: bool,
}

impl<T> Paginated<T> {
    /// Build a page from `limit + 1` fetched rows. `id_of` extracts the
    /// cursor id from the last visible row.
    pub fn from_rows(mut rows: Vec<T>, limit: i64, id_of: impl Fn(&T) -> Uuid) -> Self {
        let has_more = rows.len() as i64 > limit;
        if has_more {
            rows.truncate(limit as usize);
        }
        let next_cursor = if has_more {
            rows.last().map(id_of)
        } else {
            None
        };
        Self {
            data: rows,
            meta: PageMeta {
                limit,
                next_cursor,
                has_more,
            },
        }
    }

    /// Map the page's rows (model → response DTO) keeping the meta intact.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Paginated<U> {
        Paginated {
            data: self.data.into_iter().map(f).collect(),
            meta: self.meta,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_limits() {
        assert_eq!(clamp_limit(None), DEFAULT_LIMIT);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(-5)), 1);
        assert_eq!(clamp_limit(Some(1000)), MAX_LIMIT);
        assert_eq!(clamp_limit(Some(50)), 50);
    }

    #[test]
    fn full_page_yields_cursor() {
        let ids: Vec<Uuid> = (0..4).map(|_| Uuid::now_v7()).collect();
        let page = Paginated::from_rows(ids.clone(), 3, |id| *id);
        assert_eq!(page.data.len(), 3);
        assert!(page.meta.has_more);
        assert_eq!(page.meta.next_cursor, Some(ids[2]));
    }

    #[test]
    fn short_page_has_no_cursor() {
        let ids: Vec<Uuid> = (0..2).map(|_| Uuid::now_v7()).collect();
        let page = Paginated::from_rows(ids, 3, |id| *id);
        assert_eq!(page.data.len(), 2);
        assert!(!page.meta.has_more);
        assert_eq!(page.meta.next_cursor, None);
    }
}
