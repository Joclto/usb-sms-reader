use reqwest::Client;
use serde::{Deserialize, Serialize};
use crate::config::InfoPushConfig;
use crate::error::{AppError, Result};

#[derive(Debug, Serialize)]
pub struct PushMessage {
    pub title: String,
    pub content: String,
    #[serde(rename = "content_type")]
    pub content_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PushResponse {
    pub message_id: Option<String>,
    pub online_devices: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct InfoPushClient {
    config: InfoPushConfig,
    client: Client,
}

impl InfoPushClient {
    pub fn new(config: InfoPushConfig) -> Self {
        InfoPushClient {
            config,
            client: Client::new(),
        }
    }

    pub async fn push(&self, message: PushMessage) -> Result<PushResponse> {
        if self.config.push_token.is_empty() {
            return Err(AppError::TokenNotConfigured);
        }

        let url = format!(
            "{}/push/{}",
            self.config.server_url,
            self.config.push_token
        );

        for attempt in 1..=self.config.retry_count {
            let result = self.send_request(&url, &message).await;
            match result {
                Ok(response) => return Ok(response),
                Err(e) => {
                    if attempt == self.config.retry_count {
                        return Err(e);
                    }
                    tracing::warn!("Push attempt {} failed, retrying...", attempt);
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        }

        Err(AppError::StorageError("All retry attempts failed".to_string()))
    }

    async fn send_request(&self, url: &str, message: &PushMessage) -> Result<PushResponse> {
        let response = self.client
            .post(url)
            .json(message)
            .timeout(std::time::Duration::from_secs(self.config.timeout))
            .send()
            .await?;

        let result: PushResponse = response.json().await?;

        Ok(result)
    }
}