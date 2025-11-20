use std::sync::Arc;
use tokio::time::{interval, Duration};
use image::imageops::FilterType;
use image::ImageFormat;
use std::io::Cursor;

use crate::ai::AIService;
use crate::database::DatabaseService;
use crate::storage::S3Service;

pub struct AnalysisWorker {
    db_service: Arc<DatabaseService>,
    s3_service: Arc<S3Service>,
    ai_service: Arc<AIService>,
    interval_seconds: u64,
}

impl AnalysisWorker {
    pub fn new(
        db_service: Arc<DatabaseService>,
        s3_service: Arc<S3Service>,
        ai_service: Arc<AIService>,
        interval_seconds: u64,
    ) -> Self {
        Self {
            db_service,
            s3_service,
            ai_service,
            interval_seconds,
        }
    }

    pub async fn start(self) {
        log::info!("Starting analysis worker with interval: {}s", self.interval_seconds);

        let mut interval = interval(Duration::from_secs(self.interval_seconds));

        loop {
            interval.tick().await;

            log::info!("inicio ******** 4 - worker tick");
            if let Err(e) = self.process_pending_analyses().await {
                log::error!("Error processing analyses: {}", e);
            }
            log::info!("fin ********4 - worker tick end");
        }
    }

    async fn process_pending_analyses(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pending_captures = self.db_service.get_pending_analysis(10).await?;

        if pending_captures.is_empty() {
            return Ok(());
        }

        log::info!("inicio ******** 5 - Processing {} pending analyses", pending_captures.len());

        for capture_id in pending_captures {
            if let Err(e) = self.analyze_capture(&capture_id).await {
                let error_msg = e.to_string();
                let is_transient = error_msg.contains("503") 
                    || error_msg.contains("overloaded")
                    || error_msg.contains("UNAVAILABLE");
                
                if is_transient {
                    log::warn!("Transient error for capture {}, will retry later: {}", capture_id, error_msg);
                } else {
                    log::error!("Permanent failure for capture {}: {}", capture_id, error_msg);
                    // Increment attempts for permanent failures
                    if let Err(db_err) = self.db_service.increment_analysis_attempts(&capture_id).await {
                        log::error!("Failed to increment attempts for {}: {}", capture_id, db_err);
                    }
                }
                continue;
            }
        }

        log::info!("fin ********5 - Processing pending analyses end");
        Ok(())
    }

    async fn analyze_capture(&self, capture_id: &uuid::Uuid) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Get capture
        let capture = match self.db_service.get_capture_by_id(capture_id).await? {
            Some(c) => c,
            None => {
                log::warn!("Capture {} not found", capture_id);
                return Ok(());
            }
        };

        // Skip if already analyzed (treat empty JSON object as not analyzed)
        match &capture.vision_result {
            Some(v) => {
                if v.is_object() {
                    if let serde_json::Value::Object(map) = v {
                        if map.is_empty() {
                            log::info!("Capture {} has empty vision_result; will analyze", capture_id);
                        } else {
                            log::info!("Capture {} already has vision_result, marking completed", capture_id);
                            self.db_service.mark_analysis_completed(capture_id).await?;
                            return Ok(());
                        }
                    }
                } else {
                    // vision_result exists and is not an object (rare), consider it analyzed
                    log::info!("Capture {} has non-object vision_result, marking completed", capture_id);
                    self.db_service.mark_analysis_completed(capture_id).await?;
                    return Ok(());
                }
            }
            None => {
                // proceed to analyze
            }
        }

        log::info!("inicio ******** 6 - analyze_capture start: {}", capture_id);
        log::info!("Analyzing capture {}", capture_id);

        // Extract object key from image_url
        let object_key = match self.extract_object_key(&capture.image_url) {
            Ok(k) => k,
            Err(e) => {
                log::error!("Failed to extract object key for capture {}: {}", capture_id, e);
                return Ok(());
            }
        };

        // Download image from S3
        log::info!("inicio ******** 7 - download image start: {}", object_key);
        let image_bytes = match self.s3_service.download_object(&object_key).await {
            Ok(b) => {
                log::info!("fin ********7 - download image end: {} ({} bytes)", object_key, b.len());
                b
            }
            Err(e) => {
                log::error!("Failed to download image for capture {}: {}", capture_id, e);
                return Ok(());
            }
        };

        // Analyze with AI (with retry logic for transient errors)
        log::info!("inicio ******** 8 - ai analyze start: {}", capture_id);
        
        let mut attempts = 0;
        let max_attempts = 3;
        let mut last_error = None;
        
        let vision_result = loop {
            attempts += 1;
            
            match self.ai_service.analyze_image(
                &image_bytes,
                capture.location.as_ref(),
                capture.location_info.as_ref(),
                capture.orientation.as_ref(),
                Some(&capture.created_at),
            ).await {
                Ok(v) => {
                    log::info!("fin ********8 - ai analyze end: {} (attempt {})", capture_id, attempts);
                    break v;
                }
                Err(e) => {
                    let error_msg = e.to_string();
                    let is_retryable = error_msg.contains("503") 
                        || error_msg.contains("overloaded")
                        || error_msg.contains("UNAVAILABLE");
                    
                    if is_retryable && attempts < max_attempts {
                        let wait_seconds = 2u64.pow(attempts - 1) * 5; // 5s, 10s, 20s
                        log::warn!("AI analysis failed (attempt {}/{}), retrying in {}s: {}", 
                                  attempts, max_attempts, wait_seconds, error_msg);
                        tokio::time::sleep(tokio::time::Duration::from_secs(wait_seconds)).await;
                        continue;
                    }
                    
                    log::error!("AI analysis failed for capture {} after {} attempts: {}", 
                               capture_id, attempts, error_msg);
                    last_error = Some(e);
                    break serde_json::Value::Null;
                }
            }
        };
        
        // If analysis failed after all retries, return early
        if vision_result.is_null() {
            if let Some(e) = last_error {
                return Err(e);
            }
            return Err("AI analysis returned null".into());
        }

        // Extract metadata
        let (category, confidence) = AIService::extract_metadata(&vision_result);
        
        // Extract difficulty (default to EASY if not present)
        let difficulty = vision_result
            .get("difficulty")
            .and_then(|v| v.as_str())
            .unwrap_or("EASY")
            .to_string();
        
        // Extract verified field (default to false if not present)
        let verified = vision_result
            .get("verified")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        // Extract tags (default to empty array if not present)
        let tags: Vec<String> = vision_result
            .get("tags")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase().trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        log::info!("Extracted metadata: category={}, confidence={}, difficulty={}, verified={}, tags={:?}", 
                   category, confidence, difficulty, verified, tags);

        // Update capture with analysis result
        log::info!("inicio ******** 9 - update capture analysis start: {}", capture_id);
        if let Err(e) = self.db_service
            .update_capture_analysis(capture_id, &vision_result, &category, confidence, &difficulty, verified)
            .await {
            log::error!("Failed to update capture analysis {}: {}", capture_id, e);
            return Ok(());
        }
        
        // Save tags to normalized tables
        if !tags.is_empty() {
            log::info!("Saving {} tags for capture {}", tags.len(), capture_id);
            if let Err(e) = self.db_service.save_capture_tags(capture_id, &tags).await {
                log::error!("Failed to save tags for capture {}: {}", capture_id, e);
                // Continue even if tags fail - don't block the analysis
            } else {
                log::info!("Tags saved successfully for capture {}", capture_id);
            }
        }

        // Mark as completed
        if let Err(e) = self.db_service.mark_analysis_completed(capture_id).await {
            log::error!("Failed to mark analysis completed {}: {}", capture_id, e);
            return Ok(());
        }

        log::info!("fin ********9 - update capture analysis end: {}", capture_id);
        log::info!("Capture {} analyzed successfully: category={}, confidence={}, difficulty={}, tags_count={}", 
                   capture_id, category, confidence, difficulty, tags.len());
        
        // Generate and upload thumbnail (non-blocking - errors won't fail the analysis)
        log::info!("inicio ******** 10 - generate thumbnail start: {}", capture_id);
        if let Err(e) = self.generate_and_upload_thumbnail(capture_id, &image_bytes).await {
            log::error!("Failed to generate thumbnail for capture {}: {}", capture_id, e);
            // Continue - thumbnail is optional
        } else {
            log::info!("fin ******** 10 - generate thumbnail end: {}", capture_id);
        }

        Ok(())
    }
    
    /// Generate a 200x200 thumbnail and upload to S3
    async fn generate_and_upload_thumbnail(
        &self,
        capture_id: &uuid::Uuid,
        image_bytes: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Load image
        let img = image::load_from_memory(image_bytes)?;
        
        // Resize to 200x200 maintaining aspect ratio (cover mode)
        let thumbnail = img.resize_to_fill(200, 200, FilterType::Lanczos3);
        
        // Encode to JPEG
        let mut buffer = Cursor::new(Vec::new());
        thumbnail.write_to(&mut buffer, ImageFormat::Jpeg)?;
        let thumbnail_bytes = buffer.into_inner();
        
        // Upload to S3 with thumbnails/ prefix
        let thumbnail_key = format!("thumbnails/{}.jpg", capture_id);
        let thumbnail_url = self.s3_service.upload_bytes(
            &thumbnail_key,
            thumbnail_bytes,
            "image/jpeg",
        ).await?;
        
        log::info!("Thumbnail uploaded successfully: {}", thumbnail_url);
        
        // Update capture with thumbnail URL
        let client = self.db_service.get_client().await?;
        client.execute("
            UPDATE captures SET thumbnail_url = $2, updated_at = NOW()
            WHERE id = $1
        ", &[capture_id, &thumbnail_url]).await?;
        
        log::info!("Thumbnail URL saved to database for capture {}", capture_id);
        
        Ok(())
    }

    fn extract_object_key(&self, url: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        // Extract object key from S3 URL
        // Example: https://bucket.s3.amazonaws.com/captures/123/uuid.jpg -> captures/123/uuid.jpg
        let parts: Vec<&str> = url.split('/').collect();
        if parts.len() >= 4 {
            let key = parts[3..].join("/");
            Ok(key)
        } else {
            Err("Invalid S3 URL format".into())
        }
    }
}
