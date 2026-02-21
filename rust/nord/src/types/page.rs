use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub next_start_inclusive: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagedQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    #[serde(rename = "startInclusive")]
    pub start_inclusive: Option<u64>,
    #[serde(rename = "pageSize")]
    pub page_size: Option<u8>,
}
