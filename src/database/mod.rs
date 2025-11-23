use chrono::{DateTime, Utc};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio_postgres::NoTls;
use uuid::Uuid;

use crate::config::DatabaseConfig;
use crate::models::{AnalysisResult, Capture, DeviceUpload};

pub type DbPool = Pool;

pub struct DatabaseService {
    pool: DbPool,
}

impl DatabaseService {
    pub async fn new(
        config: &DatabaseConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let mut cfg = Config::new();
        cfg.url = Some(config.url.clone());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)?;
        let client = pool.get().await?;
        client.execute("SELECT 1", &[]).await?;

        log::info!("Database connection established");
        Ok(Self { pool })
    }

    pub async fn get_client(
        &self,
    ) -> Result<deadpool_postgres::Client, Box<dyn std::error::Error + Send + Sync>> {
        Ok(self.pool.get().await?)
    }

    /// Initialize database schema
    pub async fn init_schema(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        client
            .execute("CREATE EXTENSION IF NOT EXISTS pgcrypto", &[])
            .await
            .ok();

        // Captures table
        client
            .execute(
                "
            CREATE TABLE IF NOT EXISTS captures (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                user_id UUID,
                author_name TEXT,
                device_local_id VARCHAR(255),
                image_url TEXT NOT NULL,
                thumbnail_url TEXT,
                image_size BIGINT,
                storage_type VARCHAR(50) NOT NULL DEFAULT 's3',
                vision_result JSONB,
                category VARCHAR(100),
                confidence DOUBLE PRECISION,
                tags TEXT[],
                location JSONB,
                location_info JSONB,
                orientation JSONB,
                is_deleted BOOLEAN NOT NULL DEFAULT false,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        ",
                &[],
            )
            .await?;

        // Ensure backward compatibility for newly added columns
        client
            .execute(
                "ALTER TABLE captures ADD COLUMN IF NOT EXISTS author_name TEXT",
                &[],
            )
            .await?;

        // Indexes
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_captures_user_id ON captures(user_id)",
                &[],
            )
            .await?;
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_captures_created_at ON captures(created_at DESC)",
                &[],
            )
            .await?;
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_captures_category ON captures(category)",
                &[],
            )
            .await?;
        client.execute("CREATE INDEX IF NOT EXISTS idx_captures_device_local_id ON captures(device_local_id)", &[]).await?;
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_captures_is_deleted ON captures(is_deleted)",
                &[],
            )
            .await?;

        // Analysis results table
        client
            .execute(
                "
            CREATE TABLE IF NOT EXISTS analysis_results (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                capture_id UUID NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
                model_name VARCHAR(100) NOT NULL,
                model_version VARCHAR(50) NOT NULL,
                result JSONB NOT NULL,
                confidence DOUBLE PRECISION,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        ",
                &[],
            )
            .await?;

        client.execute("CREATE INDEX IF NOT EXISTS idx_analysis_results_capture_id ON analysis_results(capture_id)", &[]).await?;

        // Device uploads table
        client
            .execute(
                "
            CREATE TABLE IF NOT EXISTS device_uploads (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                device_id VARCHAR(255) NOT NULL,
                device_local_id VARCHAR(255) NOT NULL,
                server_capture_id UUID REFERENCES captures(id) ON DELETE SET NULL,
                status VARCHAR(50) NOT NULL DEFAULT 'pending',
                error_message TEXT,
                last_attempt TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(device_id, device_local_id)
            )
        ",
                &[],
            )
            .await?;

        client.execute("CREATE INDEX IF NOT EXISTS idx_device_uploads_device_id ON device_uploads(device_id)", &[]).await?;
        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_device_uploads_status ON device_uploads(status)",
                &[],
            )
            .await?;

        // Analysis queue table (for pending analysis tasks)
        client
            .execute(
                "
            CREATE TABLE IF NOT EXISTS analysis_queue (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                capture_id UUID NOT NULL REFERENCES captures(id) ON DELETE CASCADE,
                status VARCHAR(50) NOT NULL DEFAULT 'pending',
                attempts INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                last_attempt TIMESTAMPTZ
            )
        ",
                &[],
            )
            .await?;

        client
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_analysis_queue_status ON analysis_queue(status)",
                &[],
            )
            .await?;
        client.execute("CREATE INDEX IF NOT EXISTS idx_analysis_queue_created_at ON analysis_queue(created_at)", &[]).await?;

        log::info!("Database schema initialized");
        Ok(())
    }

    /// Create a capture
    pub async fn create_capture(
        &self,
        req: &crate::models::CreateCaptureRequest,
    ) -> Result<Capture, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let id = Uuid::new_v4();
        let now = Utc::now();

        let row = client.query_one("
            INSERT INTO captures (id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, 
                                vision_result, category, confidence, tags, location, location_info, 
                                orientation, created_at, updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            RETURNING id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, storage_type,
                      vision_result, category, confidence, tags, location, location_info, orientation,
                      is_deleted, created_at, updated_at, difficulty, verified, is_public
        ", &[
            &id,
            &req.user_id,
            &req.author_name,
            &req.device_local_id,
            &req.image_url,
            &req.thumbnail_url,
            &req.image_size,
            &req.vision_result,
            &req.category,
            &req.confidence,
            &req.tags,
            &req.location,
            &req.location_info,
            &req.orientation,
            &now,
            &now
        ]).await?;

        Ok(Self::row_to_capture(&row))
    }

    /// Get capture by ID
    pub async fn get_capture_by_id(
        &self,
        id: &Uuid,
    ) -> Result<Option<Capture>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let rows = client
            .query(
                "
                 SELECT id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, storage_type,
                     vision_result, category, confidence, tags, location, location_info, orientation,
                     is_deleted, created_at, updated_at, difficulty, verified, is_public
            FROM captures WHERE id = $1 AND is_deleted = false
        ",
                &[id],
            )
            .await?;

        if rows.is_empty() {
            return Ok(None);
        }

        Ok(Some(Self::row_to_capture(&rows[0])))
    }

    /// Get captures with pagination
    pub async fn get_captures(
        &self,
        user_id: Option<Uuid>,
        page: i32,
        limit: i32,
    ) -> Result<(Vec<Capture>, i64), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;
        let offset = (page - 1) * limit;

        let count_query = if user_id.is_some() {
            "SELECT COUNT(*) FROM captures WHERE user_id = $1 AND is_deleted = false"
        } else {
            "SELECT COUNT(*) FROM captures WHERE is_deleted = false"
        };

        let total: i64 = if let Some(uid) = user_id {
            let row = client.query_one(count_query, &[&uid]).await?;
            row.get(0)
        } else {
            let row = client.query_one(count_query, &[]).await?;
            row.get(0)
        };

        let query = if user_id.is_some() {
            "SELECT id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, storage_type,
                    vision_result, category, confidence, tags, location, location_info, orientation,
                    is_deleted, created_at, updated_at, difficulty, verified, is_public
                 FROM captures WHERE user_id = $1 AND is_deleted = false
             ORDER BY created_at DESC LIMIT $2 OFFSET $3"
        } else {
            "SELECT id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, storage_type,
                    vision_result, category, confidence, tags, location, location_info, orientation,
                    is_deleted, created_at, updated_at, difficulty, verified, is_public
             FROM captures WHERE is_deleted = false
             ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        };

        let rows = if let Some(uid) = user_id {
            client.query(query, &[&uid, &limit, &offset]).await?
        } else {
            client.query(query, &[&limit, &offset]).await?
        };

        let captures = rows.iter().map(|row| Self::row_to_capture(row)).collect();

        Ok((captures, total))
    }

    /// Update capture
    pub async fn update_capture(
        &self,
        id: &Uuid,
        req: &crate::models::UpdateCaptureRequest,
    ) -> Result<Option<Capture>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;
        let now = Utc::now();

        let row = client.query_opt("
            UPDATE captures SET
                tags = COALESCE($2, tags),
                category = COALESCE($3, category),
                updated_at = $4
            WHERE id = $1 AND is_deleted = false
            RETURNING id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, storage_type,
                      vision_result, category, confidence, tags, location, location_info, orientation,
                      is_deleted, created_at, updated_at, difficulty, verified, is_public
        ", &[id, &req.tags, &req.category, &now]).await?;

        Ok(row.map(|r| Self::row_to_capture(&r)))
    }

    /// Soft delete capture
    pub async fn delete_capture(
        &self,
        id: &Uuid,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let result = client
            .execute(
                "
            UPDATE captures SET is_deleted = true, updated_at = NOW()
            WHERE id = $1 AND is_deleted = false
        ",
                &[id],
            )
            .await?;

        Ok(result > 0)
    }

    /// Publish a capture (set is_public = true)
    pub async fn publish_capture(
        &self,
        id: &Uuid,
    ) -> Result<Option<Capture>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let row = client.query_opt("
            UPDATE captures SET is_public = true, updated_at = NOW()
            WHERE id = $1 AND is_deleted = false
            RETURNING id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, storage_type,
                      vision_result, category, confidence, tags, location, location_info, orientation,
                      is_deleted, created_at, updated_at, difficulty, verified, is_public
        ", &[id]).await?;

        Ok(row.map(|r| Self::row_to_capture(&r)))
    }

    /// Unpublish a capture (set is_public = false)
    pub async fn unpublish_capture(
        &self,
        id: &Uuid,
    ) -> Result<Option<Capture>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let row = client.query_opt("
            UPDATE captures SET is_public = false, updated_at = NOW()
            WHERE id = $1 AND is_deleted = false
            RETURNING id, user_id, author_name, device_local_id, image_url, thumbnail_url, image_size, storage_type,
                      vision_result, category, confidence, tags, location, location_info, orientation,
                      is_deleted, created_at, updated_at, difficulty, verified, is_public
        ", &[id]).await?;

        Ok(row.map(|r| Self::row_to_capture(&r)))
    }

    /// Hard delete capture (permanently remove from DB)
    pub async fn hard_delete_capture(
        &self,
        id: &Uuid,
    ) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let result = client
            .execute(
                "
            DELETE FROM captures WHERE id = $1
        ",
                &[id],
            )
            .await?;

        Ok(result > 0)
    }

    /// Enqueue capture for analysis
    pub async fn enqueue_analysis(
        &self,
        capture_id: &Uuid,
    ) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let id = Uuid::new_v4();
        client
            .execute(
                "
            INSERT INTO analysis_queue (id, capture_id, status, created_at)
            VALUES ($1, $2, 'pending', NOW())
            ON CONFLICT DO NOTHING
        ",
                &[&id, capture_id],
            )
            .await?;

        Ok(id)
    }

    /// Get pending analysis tasks
    pub async fn get_pending_analysis(
        &self,
        limit: i32,
    ) -> Result<Vec<Uuid>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;
        let limit_i64 = limit as i64;

        let rows = client
            .query(
                "
            SELECT capture_id FROM analysis_queue
            WHERE status = 'pending' AND (attempts < 3 OR attempts IS NULL)
            ORDER BY created_at ASC LIMIT $1
        ",
                &[&limit_i64],
            )
            .await?;

        Ok(rows.iter().map(|row| row.get(0)).collect())
    }

    /// Mark analysis as completed
    pub async fn mark_analysis_completed(
        &self,
        capture_id: &Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        client
            .execute(
                "
            UPDATE analysis_queue SET status = 'completed', last_attempt = NOW()
            WHERE capture_id = $1
        ",
                &[capture_id],
            )
            .await?;

        Ok(())
    }

    /// Increment analysis attempts counter
    pub async fn increment_analysis_attempts(
        &self,
        capture_id: &Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        client
            .execute(
                "
            UPDATE analysis_queue SET attempts = attempts + 1, last_attempt = NOW()
            WHERE capture_id = $1
        ",
                &[capture_id],
            )
            .await?;

        Ok(())
    }

    /// Update capture with analysis result
    pub async fn update_capture_analysis(
        &self,
        capture_id: &Uuid,
        vision_result: &serde_json::Value,
        category: &str,
        confidence: f64,
        difficulty: &str,
        verified: bool,
        tags: Option<&Vec<String>>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let tags_param = tags.cloned();

        let result = client
            .execute(
                "
            UPDATE captures 
            SET vision_result = $2, 
                category = $3, 
                confidence = $4, 
                difficulty = $5, 
                verified = $6,
                tags = $7,
                updated_at = NOW()
            WHERE id = $1
        ",
                &[
                    capture_id,
                    vision_result,
                    &category,
                    &confidence,
                    &difficulty,
                    &verified,
                    &tags_param,
                ],
            )
            .await;

        match result {
            Ok(rows) => {
                if rows == 0 {
                    log::warn!("No rows updated for capture {}", capture_id);
                }
                Ok(())
            }
            Err(e) => {
                log::error!("Database error updating capture {}: {:?}", capture_id, e);
                Err(Box::new(e))
            }
        }
    }

    /// Upsert a tag (insert if not exists, return existing if it does)
    pub async fn upsert_tag(
        &self,
        name: &str,
    ) -> Result<Uuid, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let row = client
            .query_one(
                "
            INSERT INTO tags (name) VALUES ($1)
            ON CONFLICT (name) DO UPDATE SET name = EXCLUDED.name
            RETURNING id
        ",
                &[&name],
            )
            .await?;

        Ok(row.get(0))
    }

    /// Insert a capture-tag relationship
    pub async fn insert_capture_tag(
        &self,
        capture_id: &Uuid,
        tag_id: &Uuid,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        client
            .execute(
                "
            INSERT INTO capture_tags (capture_id, tag_id) 
            VALUES ($1, $2) 
            ON CONFLICT DO NOTHING
        ",
                &[capture_id, tag_id],
            )
            .await?;

        Ok(())
    }

    /// Get all tags for a capture
    pub async fn get_tags_for_capture(
        &self,
        capture_id: &Uuid,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let rows = client
            .query(
                "
            SELECT t.name FROM tags t
            JOIN capture_tags ct ON t.id = ct.tag_id
            WHERE ct.capture_id = $1
            ORDER BY t.name
        ",
                &[capture_id],
            )
            .await?;

        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    /// Save tags for a capture (replaces existing tags)
    pub async fn save_capture_tags(
        &self,
        capture_id: &Uuid,
        tag_names: &[String],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Delete existing tags for this capture
        let client = self.get_client().await?;
        client
            .execute(
                "DELETE FROM capture_tags WHERE capture_id = $1",
                &[capture_id],
            )
            .await?;

        // Insert new tags
        for tag_name in tag_names {
            let tag_id = self.upsert_tag(tag_name).await?;
            self.insert_capture_tag(capture_id, &tag_id).await?;
        }

        Ok(())
    }

    /// Get all unique tags in the system
    pub async fn get_all_tags(
        &self,
    ) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
        let client = self.get_client().await?;

        let rows = client
            .query(
                "
            SELECT name FROM tags ORDER BY name
        ",
                &[],
            )
            .await?;

        Ok(rows.iter().map(|r| r.get(0)).collect())
    }

    fn row_to_capture(row: &tokio_postgres::Row) -> Capture {
        Capture {
            id: row.get(0),
            user_id: row.get(1),
            author_name: row.get(2),
            device_local_id: row.get(3),
            image_url: row.get(4),
            thumbnail_url: row.get(5),
            image_size: row.get(6),
            storage_type: row.get(7),
            vision_result: row.get(8),
            category: row.get(9),
            confidence: row.get(10),
            tags: row.get(11),
            location: row.get(12),
            location_info: row.get(13),
            orientation: row.get(14),
            is_deleted: row.get(15),
            created_at: row.get(16),
            updated_at: row.get(17),
            difficulty: row.get(18),
            verified: row.get(19),
            is_public: row.get(20),
        }
    }
}
