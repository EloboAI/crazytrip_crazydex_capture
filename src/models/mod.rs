use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use validator::Validate;

/// Capture model - representa una captura de imagen con análisis AI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capture {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub author_name: Option<String>,
    pub device_local_id: Option<String>,
    pub image_url: String,
    pub thumbnail_url: Option<String>,
    pub image_size: Option<i64>,
    pub storage_type: String,
    pub vision_result: Option<serde_json::Value>,
    pub category: Option<String>,
    pub confidence: Option<f64>,
    pub tags: Option<Vec<String>>,
    pub location: Option<serde_json::Value>,
    pub location_info: Option<serde_json::Value>,
    pub orientation: Option<serde_json::Value>,
    pub is_deleted: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub difficulty: Option<String>,
    pub verified: Option<bool>,
    pub is_public: bool,
}

/// Request para crear una captura
#[derive(Debug, Deserialize, Validate)]
pub struct CreateCaptureRequest {
    pub user_id: Option<Uuid>,
    pub author_name: Option<String>,
    pub device_local_id: Option<String>,
    #[validate(url)]
    pub image_url: String,
    pub thumbnail_url: Option<String>,
    pub image_size: Option<i64>,
    pub vision_result: Option<serde_json::Value>,
    pub category: Option<String>,
    pub confidence: Option<f64>,
    pub tags: Option<Vec<String>>,
    pub location: Option<serde_json::Value>,
    pub location_info: Option<serde_json::Value>,
    pub orientation: Option<serde_json::Value>,
}

/// Request para actualizar una captura
#[derive(Debug, Deserialize)]
pub struct UpdateCaptureRequest {
    pub tags: Option<Vec<String>>,
    pub category: Option<String>,
    pub notes: Option<String>,
}

/// Respuesta paginada de capturas
#[derive(Debug, Serialize)]
pub struct CaptureListResponse {
    pub captures: Vec<Capture>,
    pub total: i64,
    pub page: i32,
    pub limit: i32,
    pub has_more: bool,
}

/// Request para presigned URL
#[derive(Debug, Deserialize, Validate)]
pub struct PresignedUrlRequest {
    #[validate(length(min = 1, max = 255))]
    pub filename: String,
    #[validate(length(min = 1, max = 100))]
    pub content_type: String,
}

/// Response de presigned URL
#[derive(Debug, Serialize)]
pub struct PresignedUrlResponse {
    pub upload_url: String,
    pub object_key: String,
    pub public_url: String,
    pub expires_in_seconds: i64,
}

/// Sync request desde dispositivo
#[derive(Debug, Deserialize, Validate)]
pub struct SyncUploadRequest {
    pub captures: Vec<SyncCaptureData>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SyncCaptureData {
    pub device_local_id: String,
    #[validate(url)]
    pub image_url: String,
    pub vision_result: Option<serde_json::Value>,
    pub category: Option<String>,
    pub confidence: Option<f64>,
    pub location: Option<serde_json::Value>,
    pub location_info: Option<serde_json::Value>,
    pub orientation: Option<serde_json::Value>,
    pub timestamp: DateTime<Utc>,
}

/// Response de sync
#[derive(Debug, Serialize)]
pub struct SyncUploadResponse {
    pub synced: Vec<SyncedCapture>,
    pub failed: Vec<SyncFailure>,
}

#[derive(Debug, Serialize)]
pub struct SyncedCapture {
    pub device_local_id: String,
    pub server_id: Uuid,
    pub image_url: String,
}

#[derive(Debug, Serialize)]
pub struct SyncFailure {
    pub device_local_id: String,
    pub error: String,
}

/// Device upload tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceUpload {
    pub id: Uuid,
    pub device_id: String,
    pub device_local_id: String,
    pub server_capture_id: Option<Uuid>,
    pub status: String,
    pub error_message: Option<String>,
    pub last_attempt: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Analysis result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisResult {
    pub id: Uuid,
    pub capture_id: Uuid,
    pub model_name: String,
    pub model_version: String,
    pub result: serde_json::Value,
    pub confidence: Option<f64>,
    pub created_at: DateTime<Utc>,
}

/// API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            timestamp: Utc::now(),
        }
    }

    pub fn error(message: String) -> ApiResponse<()> {
        ApiResponse {
            success: false,
            data: None,
            error: Some(message),
            timestamp: Utc::now(),
        }
    }
}

/// Pagination parameters
#[derive(Debug, Deserialize)]
pub struct PaginationParams {
    pub page: Option<i32>,
    pub limit: Option<i32>,
    pub cursor: Option<Uuid>,
}

impl Default for PaginationParams {
    fn default() -> Self {
        Self {
            page: Some(1),
            limit: Some(20),
            cursor: None,
        }
    }
}
