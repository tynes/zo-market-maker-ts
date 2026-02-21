use serde::{Deserialize, Serialize};

/// Paginated result containing items and a cursor for the next page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub next_start_inclusive: Option<serde_json::Value>,
}

/// Query parameters for paginated API requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    #[serde(rename = "startInclusive")]
    pub start_inclusive: Option<u64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<u8>,
}
