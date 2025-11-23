use actix_web::{web, HttpResponse, Result};
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

use crate::database::DatabaseService;
use crate::models::*;
use crate::storage::S3Service;
use crate::webhooks::{self, CapturePublishedEvent, WebhookClient};
use serde_json::Value as JsonValue;

/// Health check endpoint
pub async fn health_check() -> Result<HttpResponse> {
    Ok(
        HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "status": "healthy",
            "service": "crazytrip-crazydex-capture",
            "version": env!("CARGO_PKG_VERSION"),
            "timestamp": chrono::Utc::now().to_rfc3339()
        }))),
    )
}

/// Generate presigned upload URL
pub async fn generate_presigned_url(
    req: web::Json<PresignedUrlRequest>,
    s3_service: web::Data<Arc<S3Service>>,
) -> Result<HttpResponse> {
    log::info!(
        "📥 Received presigned URL request: filename={}, content_type={}",
        req.filename,
        req.content_type
    );
    log::info!("inicio ******** 1 - presign request start");
    if let Err(e) = req.validate() {
        log::warn!("❌ Validation error: {:?}", e);
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    let object_key = S3Service::generate_object_key(&req.filename);
    log::info!("🔑 Generated object key: {}", object_key);

    match s3_service
        .generate_presigned_put_url(&object_key, &req.content_type, 3600)
        .await
    {
        Ok(upload_url) => {
            let public_url = s3_service.get_public_url(&object_key);

            log::info!("✅ Presigned URL generated successfully");
            log::info!("fin ********1 - presign request end");
            log::debug!(
                "   Upload URL: {}...",
                &upload_url[..50.min(upload_url.len())]
            );
            log::debug!("   Public URL: {}", public_url);

            let response = PresignedUrlResponse {
                upload_url,
                object_key,
                public_url,
                expires_in_seconds: 3600,
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(response)))
        }
        Err(e) => {
            log::error!("❌ Failed to generate presigned URL: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to generate upload URL".to_string(),
                )),
            )
        }
    }
}

/// Create a new capture
pub async fn create_capture(
    req: web::Json<CreateCaptureRequest>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    let mut payload = req.into_inner();
    log::info!("📥 Received create capture request");
    log::debug!("   Device local ID: {:?}", payload.device_local_id);
    log::debug!("   Image URL: {}", payload.image_url);
    log::debug!("   Has vision_result: {}", payload.vision_result.is_some());

    if let Err(e) = payload.validate() {
        log::warn!("❌ Validation error: {:?}", e);
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    payload.author_name = payload
        .author_name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    if payload.user_id.is_none() {
        log::warn!(
            "⚠️ Capture created without user_id; stories will fall back to anonymous author"
        );
    }

    if payload.author_name.is_none() {
        if let Some(user_id) = payload.user_id {
            log::warn!(
                "⚠️ Capture for user {} missing author_name; default label will be used",
                user_id
            );
        } else {
            log::warn!(
                "⚠️ Capture missing author_name and user context; default label will be used"
            );
        }
    }

    match db_service.create_capture(&payload).await {
        Ok(capture) => {
            log::info!("✅ Capture created successfully: ID={}", capture.id);

            // Enqueue for analysis if no vision_result provided or if vision_result is an empty JSON object
            let should_enqueue = match &capture.vision_result {
                None => true,
                Some(JsonValue::Object(map)) if map.is_empty() => true,
                _ => false,
            };

            if should_enqueue {
                log::info!("inicio ******** 3 - enqueue capture start: {}", capture.id);
                log::info!("📋 Enqueueing capture for AI analysis...");
                if let Err(e) = db_service.enqueue_analysis(&capture.id).await {
                    log::error!("❌ Failed to enqueue analysis: {}", e);
                } else {
                    log::info!("✅ Capture enqueued for analysis");
                }
                log::info!("fin ********3 - enqueue capture end: {}", capture.id);
            } else {
                log::info!("ℹ️  Capture already has vision_result, skipping analysis queue");
            }

            Ok(HttpResponse::Created().json(ApiResponse::success(capture)))
        }
        Err(e) => {
            log::error!("❌ Failed to create capture: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to create capture".to_string(),
                )),
            )
        }
    }
}

/// Get capture by ID
pub async fn get_capture(
    path: web::Path<Uuid>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    let capture_id = path.into_inner();

    match db_service.get_capture_by_id(&capture_id).await {
        Ok(Some(capture)) => Ok(HttpResponse::Ok().json(ApiResponse::success(capture))),
        Ok(None) => Ok(HttpResponse::NotFound()
            .json(ApiResponse::<()>::error("Capture not found".to_string()))),
        Err(e) => {
            log::error!("Failed to get capture: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to retrieve capture".to_string(),
                )),
            )
        }
    }
}

/// Get captures list with pagination
pub async fn list_captures(
    query: web::Query<PaginationParams>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    let page = query.page.unwrap_or(1).max(1);
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    match db_service.get_captures(None, page, limit).await {
        Ok((captures, total)) => {
            let response = CaptureListResponse {
                captures,
                total,
                page,
                limit,
                has_more: (page * limit) < total as i32,
            };

            Ok(HttpResponse::Ok().json(ApiResponse::success(response)))
        }
        Err(e) => {
            log::error!("Failed to list captures: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to retrieve captures".to_string(),
                )),
            )
        }
    }
}

/// Update capture
pub async fn update_capture(
    path: web::Path<Uuid>,
    req: web::Json<UpdateCaptureRequest>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    let capture_id = path.into_inner();

    match db_service.update_capture(&capture_id, &req).await {
        Ok(Some(capture)) => Ok(HttpResponse::Ok().json(ApiResponse::success(capture))),
        Ok(None) => Ok(HttpResponse::NotFound()
            .json(ApiResponse::<()>::error("Capture not found".to_string()))),
        Err(e) => {
            log::error!("Failed to update capture: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to update capture".to_string(),
                )),
            )
        }
    }
}

/// Delete capture (hard delete + S3 cleanup)
pub async fn delete_capture(
    path: web::Path<Uuid>,
    db_service: web::Data<Arc<DatabaseService>>,
    s3_service: web::Data<Arc<S3Service>>,
) -> Result<HttpResponse> {
    let capture_id = path.into_inner();
    log::info!("🗑️ Deleting capture: {}", capture_id);

    // 1. Get capture to find image URL and thumbnail URL
    let capture = match db_service.get_capture_by_id(&capture_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Ok(HttpResponse::NotFound()
                .json(ApiResponse::<()>::error("Capture not found".to_string())))
        }
        Err(e) => {
            log::error!("Failed to retrieve capture for deletion: {}", e);
            return Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to retrieve capture".to_string(),
                )),
            );
        }
    };

    log::info!("📸 Capture image_url: {}", capture.image_url);
    log::info!("🖼️ Capture thumbnail_url: {:?}", capture.thumbnail_url);

    // 2. Delete main image from S3
    if let Some(object_key) = extract_key_from_url(&capture.image_url) {
        log::info!("🗑️ Deleting S3 main image: {}", object_key);
        if let Err(e) = s3_service.delete_object(&object_key).await {
            log::error!("❌ Failed to delete S3 object {}: {}", object_key, e);
            // Continue to delete from DB even if S3 fails
        } else {
            log::info!("✅ S3 main image deleted: {}", object_key);
        }
    } else {
        log::warn!(
            "⚠️ Could not extract S3 key from URL: {}",
            capture.image_url
        );
    }

    // 3. Delete thumbnail from S3 (if exists)
    if let Some(thumbnail_url) = &capture.thumbnail_url {
        if let Some(thumbnail_key) = extract_key_from_url(thumbnail_url) {
            log::info!("🗑️ Deleting S3 thumbnail: {}", thumbnail_key);
            if let Err(e) = s3_service.delete_object(&thumbnail_key).await {
                log::error!("❌ Failed to delete S3 thumbnail {}: {}", thumbnail_key, e);
                // Continue even if thumbnail deletion fails
            } else {
                log::info!("✅ S3 thumbnail deleted: {}", thumbnail_key);
            }
        }
    }

    // 4. Hard delete from DB
    match db_service.hard_delete_capture(&capture_id).await {
        Ok(true) => {
            log::info!("✅ Capture deleted from DB: {}", capture_id);
            Ok(
                HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
                    "message": "Capture deleted permanently"
                }))),
            )
        }
        Ok(false) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error(
            "Capture not found in DB".to_string(),
        ))),
        Err(e) => {
            log::error!("Failed to delete capture from DB: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to delete capture".to_string(),
                )),
            )
        }
    }
}

fn extract_key_from_url(url: &str) -> Option<String> {
    // Simple extraction: assume key starts after the domain
    // Example: https://crazytrip-captures.s3.amazonaws.com/captures/123/abc.jpg
    // Key: captures/123/abc.jpg

    if let Some(start_idx) = url.find(".com/") {
        Some(url[start_idx + 5..].to_string())
    } else if let Some(start_idx) = url.find("/captures/") {
        // Fallback if domain format is different but path is standard
        Some(url[start_idx + 1..].to_string())
    } else {
        // Maybe it IS the key?
        if !url.starts_with("http") {
            Some(url.to_string())
        } else {
            None
        }
    }
}

/// Sync upload from device
pub async fn sync_upload(
    req: web::Json<SyncUploadRequest>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    if let Err(e) = req.validate() {
        return Ok(
            HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!(
                "Validation error: {:?}",
                e
            ))),
        );
    }

    let mut synced = Vec::new();
    let mut failed = Vec::new();

    for capture_data in &req.captures {
        let create_req = CreateCaptureRequest {
            user_id: None,
            author_name: None,
            device_local_id: Some(capture_data.device_local_id.clone()),
            image_url: capture_data.image_url.clone(),
            thumbnail_url: None,
            image_size: None,
            vision_result: capture_data.vision_result.clone(),
            category: capture_data.category.clone(),
            confidence: capture_data.confidence,
            tags: None,
            location: capture_data.location.clone(),
            location_info: capture_data.location_info.clone(),
            orientation: capture_data.orientation.clone(),
        };

        match db_service.create_capture(&create_req).await {
            Ok(capture) => {
                synced.push(SyncedCapture {
                    device_local_id: capture_data.device_local_id.clone(),
                    server_id: capture.id,
                    image_url: capture.image_url,
                });

                // Enqueue for analysis if needed (treat empty JSON object as none)
                let should_enqueue = match &capture.vision_result {
                    None => true,
                    Some(JsonValue::Object(map)) if map.is_empty() => true,
                    _ => false,
                };

                if should_enqueue {
                    let _ = db_service.enqueue_analysis(&capture.id).await;
                }
            }
            Err(e) => {
                failed.push(SyncFailure {
                    device_local_id: capture_data.device_local_id.clone(),
                    error: e.to_string(),
                });
            }
        }
    }

    let response = SyncUploadResponse { synced, failed };
    Ok(HttpResponse::Ok().json(ApiResponse::success(response)))
}

/// Publish a capture (make it visible in public feed)
pub async fn publish_capture(
    path: web::Path<Uuid>,
    db_service: web::Data<Arc<DatabaseService>>,
    s3_service: web::Data<Arc<S3Service>>,
    webhook_client: web::Data<Arc<WebhookClient>>,
    webhooks_enabled: web::Data<bool>,
) -> Result<HttpResponse> {
    let capture_id = path.into_inner();
    log::info!("📢 Publishing capture: {}", capture_id);

    match db_service.publish_capture(&capture_id).await {
        Ok(Some(capture)) => {
            log::info!("✅ Capture published successfully: {}", capture_id);

            // Send webhook if enabled
            if *webhooks_enabled.as_ref() {
                let mut public_image_url = capture.image_url.clone();
                if let Some(object_key) = extract_key_from_url(&capture.image_url) {
                    match s3_service
                        .generate_presigned_get_url(&object_key, 86_400)
                        .await
                    {
                        Ok(url) => public_image_url = url,
                        Err(e) => log::warn!(
                            "Failed to generate presigned image URL for {}: {}",
                            object_key,
                            e
                        ),
                    }
                }

                let mut public_thumbnail_url = capture.thumbnail_url.clone();
                if let Some(thumbnail) = &capture.thumbnail_url {
                    if let Some(thumbnail_key) = extract_key_from_url(thumbnail) {
                        match s3_service
                            .generate_presigned_get_url(&thumbnail_key, 86_400)
                            .await
                        {
                            Ok(url) => public_thumbnail_url = Some(url),
                            Err(e) => log::warn!(
                                "Failed to generate presigned thumbnail URL for {}: {}",
                                thumbnail_key,
                                e
                            ),
                        }
                    }
                }

                let author_name = capture
                    .author_name
                    .as_ref()
                    .map(|name| name.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Explorador".to_string());

                let event = CapturePublishedEvent {
                    capture_id: capture.id,
                    author_id: capture.user_id.unwrap_or_else(|| Uuid::nil()),
                    author_name: Some(author_name.clone()),
                    image_url: public_image_url,
                    thumbnail_url: public_thumbnail_url,
                    category: capture.category.clone(),
                    tags: capture.tags.clone(),
                    location: capture
                        .location
                        .as_ref()
                        .and_then(|loc| webhooks::extract_location_from_json(loc)),
                    location_info: capture
                        .location_info
                        .as_ref()
                        .and_then(|info| webhooks::extract_location_info_from_json(info)),
                };

                tokio::spawn(async move {
                    if let Err(e) = webhook_client.send_capture_published(event).await {
                        log::error!("❌ Failed to send webhook: {}", e);
                    }
                });
            }

            Ok(HttpResponse::Ok().json(ApiResponse::success(capture)))
        }
        Ok(None) => {
            log::warn!("❌ Capture not found: {}", capture_id);
            Ok(HttpResponse::NotFound()
                .json(ApiResponse::<()>::error("Capture not found".to_string())))
        }
        Err(e) => {
            log::error!("❌ Failed to publish capture: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to publish capture".to_string(),
                )),
            )
        }
    }
}

/// Unpublish a capture (remove from public feed)
pub async fn unpublish_capture(
    path: web::Path<Uuid>,
    db_service: web::Data<Arc<DatabaseService>>,
    webhook_client: web::Data<Arc<WebhookClient>>,
    webhooks_enabled: web::Data<bool>,
) -> Result<HttpResponse> {
    let capture_id = path.into_inner();
    log::info!("🔇 Unpublishing capture: {}", capture_id);

    match db_service.unpublish_capture(&capture_id).await {
        Ok(Some(capture)) => {
            log::info!("✅ Capture unpublished successfully: {}", capture_id);

            // Send webhook if enabled
            if *webhooks_enabled.as_ref() {
                let author_name = capture
                    .author_name
                    .as_ref()
                    .map(|name| name.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| "Explorador".to_string());

                let event = CapturePublishedEvent {
                    capture_id: capture.id,
                    author_id: capture.user_id.unwrap_or_else(|| Uuid::nil()),
                    author_name: Some(author_name.clone()),
                    image_url: capture.image_url.clone(),
                    thumbnail_url: capture.thumbnail_url.clone(),
                    category: capture.category.clone(),
                    tags: capture.tags.clone(),
                    location: capture
                        .location
                        .as_ref()
                        .and_then(|loc| webhooks::extract_location_from_json(loc)),
                    location_info: capture
                        .location_info
                        .as_ref()
                        .and_then(|info| webhooks::extract_location_info_from_json(info)),
                };

                tokio::spawn(async move {
                    if let Err(e) = webhook_client.send_capture_unpublished(event).await {
                        log::error!("❌ Failed to send webhook: {}", e);
                    }
                });
            }

            Ok(HttpResponse::Ok().json(ApiResponse::success(capture)))
        }
        Ok(None) => {
            log::warn!("❌ Capture not found: {}", capture_id);
            Ok(HttpResponse::NotFound()
                .json(ApiResponse::<()>::error("Capture not found".to_string())))
        }
        Err(e) => {
            log::error!("❌ Failed to unpublish capture: {}", e);
            Ok(
                HttpResponse::InternalServerError().json(ApiResponse::<()>::error(
                    "Failed to unpublish capture".to_string(),
                )),
            )
        }
    }
}
