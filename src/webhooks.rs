use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct CapturePublishedEvent {
    pub capture_id: Uuid,
    pub author_id: Uuid,
    pub author_name: Option<String>,
    pub image_url: String,
    pub thumbnail_url: Option<String>,
    pub category: Option<String>,
    pub tags: Option<Vec<String>>,
    pub location: Option<Location>,
    pub location_info: Option<LocationInfo>,
}

#[derive(Debug, Serialize)]
pub struct Location {
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Serialize)]
pub struct LocationInfo {
    pub name: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct WebhookResponse {
    pub success: bool,
    pub story_id: Option<Uuid>,
    pub message: String,
}

pub struct WebhookClient {
    client: reqwest::Client,
    stories_service_url: String,
}

impl WebhookClient {
    pub fn new(stories_service_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            stories_service_url,
        }
    }

    pub async fn send_capture_published(&self, event: CapturePublishedEvent) -> Result<WebhookResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/v1/webhooks/capture-published", self.stories_service_url);
        
        log::info!("📤 Sending capture published webhook to: {}", url);
        
        let response = self.client
            .post(&url)
            .json(&event)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        let status = response.status();
        
        if status.is_success() {
            let webhook_response: WebhookResponse = response.json().await?;
            log::info!("✅ Webhook sent successfully: {:?}", webhook_response);
            Ok(webhook_response)
        } else {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("❌ Webhook failed with status {}: {}", status, error_text);
            Err(format!("Webhook failed: {} - {}", status, error_text).into())
        }
    }

    pub async fn send_capture_unpublished(&self, event: CapturePublishedEvent) -> Result<WebhookResponse, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/v1/webhooks/capture-unpublished", self.stories_service_url);
        
        log::info!("📤 Sending capture unpublished webhook to: {}", url);
        
        let response = self.client
            .post(&url)
            .json(&event)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await?;

        let status = response.status();
        
        if status.is_success() {
            let webhook_response: WebhookResponse = response.json().await?;
            log::info!("✅ Webhook sent successfully: {:?}", webhook_response);
            Ok(webhook_response)
        } else {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            log::error!("❌ Webhook failed with status {}: {}", status, error_text);
            Err(format!("Webhook failed: {} - {}", status, error_text).into())
        }
    }
}

/// Extract location from capture JSON
pub fn extract_location_from_json(location_json: &serde_json::Value) -> Option<Location> {
    if let Some(obj) = location_json.as_object() {
        let lat = obj.get("latitude")?.as_f64()?;
        let lng = obj.get("longitude")?.as_f64()?;
        Some(Location {
            latitude: lat,
            longitude: lng,
        })
    } else {
        None
    }
}

/// Extract location info from capture JSON
pub fn extract_location_info_from_json(location_info_json: &serde_json::Value) -> Option<LocationInfo> {
    if let Some(obj) = location_info_json.as_object() {
        Some(LocationInfo {
            name: obj.get("name").and_then(|v| v.as_str()).map(String::from),
            city: obj.get("city").and_then(|v| v.as_str()).map(String::from),
            country: obj.get("country").and_then(|v| v.as_str()).map(String::from),
        })
    } else {
        None
    }
}
