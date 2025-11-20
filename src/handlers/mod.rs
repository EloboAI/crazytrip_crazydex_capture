use actix_web::{web, HttpResponse, Result};
use std::sync::Arc;
use uuid::Uuid;
use validator::Validate;

use crate::database::DatabaseService;
use crate::models::*;
use crate::storage::S3Service;
use serde_json::Value as JsonValue;

/// Health check endpoint
pub async fn health_check() -> Result<HttpResponse> {
    Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
        "status": "healthy",
        "service": "crazytrip-crazydex-capture",
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339()
    }))))
}

/// Generate presigned upload URL
pub async fn generate_presigned_url(
    req: web::Json<PresignedUrlRequest>,
    s3_service: web::Data<Arc<S3Service>>,
) -> Result<HttpResponse> {
    log::info!("📥 Received presigned URL request: filename={}, content_type={}", req.filename, req.content_type);
    log::info!("inicio ******** 1 - presign request start");
    if let Err(e) = req.validate() {
        log::warn!("❌ Validation error: {:?}", e);
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("Validation error: {:?}", e))));
    }

    let object_key = S3Service::generate_object_key(&req.filename);
    log::info!("🔑 Generated object key: {}", object_key);
    
    match s3_service.generate_presigned_put_url(&object_key, &req.content_type, 3600).await {
        Ok(upload_url) => {
            let public_url = s3_service.get_public_url(&object_key);
            
            log::info!("✅ Presigned URL generated successfully");
            log::info!("fin ********1 - presign request end");
            log::debug!("   Upload URL: {}...", &upload_url[..50.min(upload_url.len())]);
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
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to generate upload URL".to_string())))
        }
    }
}

/// Create a new capture
pub async fn create_capture(
    req: web::Json<CreateCaptureRequest>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    log::info!("📥 Received create capture request");
    log::debug!("   Device local ID: {:?}", req.device_local_id);
    log::debug!("   Image URL: {}", req.image_url);
    log::debug!("   Has vision_result: {}", req.vision_result.is_some());
    
    if let Err(e) = req.validate() {
        log::warn!("❌ Validation error: {:?}", e);
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("Validation error: {:?}", e))));
    }

    match db_service.create_capture(&req).await {
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
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to create capture".to_string())))
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
        Ok(None) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("Capture not found".to_string()))),
        Err(e) => {
            log::error!("Failed to get capture: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to retrieve capture".to_string())))
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
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to retrieve captures".to_string())))
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
        Ok(None) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("Capture not found".to_string()))),
        Err(e) => {
            log::error!("Failed to update capture: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to update capture".to_string())))
        }
    }
}

/// Delete capture (soft delete)
pub async fn delete_capture(
    path: web::Path<Uuid>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    let capture_id = path.into_inner();

    match db_service.delete_capture(&capture_id).await {
        Ok(true) => Ok(HttpResponse::Ok().json(ApiResponse::success(serde_json::json!({
            "message": "Capture deleted successfully"
        })))),
        Ok(false) => Ok(HttpResponse::NotFound().json(ApiResponse::<()>::error("Capture not found".to_string()))),
        Err(e) => {
            log::error!("Failed to delete capture: {}", e);
            Ok(HttpResponse::InternalServerError().json(ApiResponse::<()>::error("Failed to delete capture".to_string())))
        }
    }
}

/// Sync upload from device
pub async fn sync_upload(
    req: web::Json<SyncUploadRequest>,
    db_service: web::Data<Arc<DatabaseService>>,
) -> Result<HttpResponse> {
    if let Err(e) = req.validate() {
        return Ok(HttpResponse::BadRequest().json(ApiResponse::<()>::error(format!("Validation error: {:?}", e))));
    }

    let mut synced = Vec::new();
    let mut failed = Vec::new();

    for capture_data in &req.captures {
        let create_req = CreateCaptureRequest {
            user_id: None,
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
