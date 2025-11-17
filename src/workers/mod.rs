use std::sync::Arc;
use tokio::time::{interval, Duration};

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

            if let Err(e) = self.process_pending_analyses().await {
                log::error!("Error processing analyses: {}", e);
            }
        }
    }

    async fn process_pending_analyses(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let pending_captures = self.db_service.get_pending_analysis(10).await?;

        if pending_captures.is_empty() {
            return Ok(());
        }

        log::info!("Processing {} pending analyses", pending_captures.len());

        for capture_id in pending_captures {
            if let Err(e) = self.analyze_capture(&capture_id).await {
                log::error!("Failed to analyze capture {}: {}", capture_id, e);
                continue;
            }
        }

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

        // Skip if already analyzed
        if capture.vision_result.is_some() {
            self.db_service.mark_analysis_completed(capture_id).await?;
            return Ok(());
        }

        log::info!("Analyzing capture {}", capture_id);

        // Extract object key from image_url
        let object_key = self.extract_object_key(&capture.image_url)?;

        // Download image from S3
        let image_bytes = self.s3_service.download_object(&object_key).await?;

        // Analyze with AI
        let vision_result = self.ai_service.analyze_image(&image_bytes).await?;

        // Extract metadata
        let (category, confidence) = AIService::extract_metadata(&vision_result);

        // Update capture with analysis result
        self.db_service
            .update_capture_analysis(capture_id, &vision_result, &category, confidence)
            .await?;

        // Mark as completed
        self.db_service.mark_analysis_completed(capture_id).await?;

        log::info!("Capture {} analyzed successfully: category={}, confidence={}", capture_id, category, confidence);

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
