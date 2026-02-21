pub mod endpoints;

use reqwest::Client;
use serde::de::DeserializeOwned;

use crate::error::{NordError, Result};

/// HTTP client wrapper for the Nord REST API.
#[derive(Debug, Clone)]
pub struct NordHttpClient {
    client: Client,
    base_url: String,
}

impl NordHttpClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            client: Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// GET a JSON resource.
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, &str)],
    ) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self
            .client
            .get(&url)
            .query(query)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NordError::Http {
                status,
                message: body,
            });
        }

        resp.json::<T>().await.map_err(NordError::Request)
    }

    /// POST a binary action payload to `/action`.
    pub async fn post_action(&self, payload: &[u8]) -> Result<Vec<u8>> {
        let url = format!("{}/action", self.base_url);
        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/octet-stream")
            .body(payload.to_vec())
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(NordError::Http {
                status,
                message: body,
            });
        }

        resp.bytes().await.map(|b| b.to_vec()).map_err(NordError::Request)
    }

    /// Get the base URL.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}
