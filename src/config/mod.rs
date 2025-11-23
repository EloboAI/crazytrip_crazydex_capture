use serde::{Deserialize, Serialize};
use std::env;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub storage: StorageConfig,
    pub ai: AIConfig,
    pub security: SecurityConfig,
    pub logging: LoggingConfig,
    pub worker: WorkerConfig,
    pub webhooks: WebhookConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub max_connections: usize,
    pub timeout_seconds: u64,
    pub keep_alive_seconds: u64,
    pub client_timeout_seconds: u64,
    pub client_shutdown_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_seconds: u64,
    pub idle_timeout_seconds: u64,
    pub max_lifetime_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub aws_region: String,
    pub aws_access_key_id: String,
    pub aws_secret_access_key: String,
    pub s3_bucket: String,
    pub s3_endpoint: Option<String>,
    pub max_image_size_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIConfig {
    pub gemini_api_key: String,
    pub gemini_endpoint: String,
    pub gemini_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub cors_allowed_origins: Vec<String>,
    pub rate_limit_requests: u32,
    pub rate_limit_window_seconds: u64,
    pub max_request_size_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub analysis_enabled: bool,
    pub analysis_interval_seconds: u64,
    pub thumbnail_enabled: bool,
    pub max_thumbnail_width: u32,
    pub max_thumbnail_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub stories_service_url: String,
    pub enabled: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error>> {
        let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "8081".to_string())
            .parse::<u16>()?;
        let workers = env::var("WORKERS")
            .unwrap_or_else(|_| "4".to_string())
            .parse::<usize>()?;
        let max_connections = env::var("MAX_CONNECTIONS")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<usize>()?;
        let timeout_seconds = env::var("TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<u64>()?;
        let keep_alive_seconds = env::var("KEEP_ALIVE_SECONDS")
            .unwrap_or_else(|_| "75".to_string())
            .parse::<u64>()?;
        let client_timeout_seconds = env::var("CLIENT_TIMEOUT_SECONDS")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<u64>()?;
        let client_shutdown_seconds = env::var("CLIENT_SHUTDOWN_SECONDS")
            .unwrap_or_else(|_| "5".to_string())
            .parse::<u64>()?;

        let database_url = env::var("DATABASE_URL")?;
        let db_max_connections = env::var("DB_MAX_CONNECTIONS")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u32>()?;
        let db_min_connections = env::var("DB_MIN_CONNECTIONS")
            .unwrap_or_else(|_| "1".to_string())
            .parse::<u32>()?;
        let db_connect_timeout = env::var("DB_CONNECT_TIMEOUT")
            .unwrap_or_else(|_| "10".to_string())
            .parse::<u64>()?;
        let db_idle_timeout = env::var("DB_IDLE_TIMEOUT")
            .unwrap_or_else(|_| "300".to_string())
            .parse::<u64>()?;
        let db_max_lifetime = env::var("DB_MAX_LIFETIME")
            .unwrap_or_else(|_| "3600".to_string())
            .parse::<u64>()?;

        let aws_region = env::var("AWS_REGION").unwrap_or_else(|_| "us-east-1".to_string());
        let aws_access_key_id = env::var("AWS_ACCESS_KEY_ID")?;
        let aws_secret_access_key = env::var("AWS_SECRET_ACCESS_KEY")?;
        let s3_bucket = env::var("S3_BUCKET")?;
        let s3_endpoint = env::var("S3_ENDPOINT").ok();
        let max_image_size_bytes = env::var("MAX_IMAGE_SIZE_BYTES")
            .unwrap_or_else(|_| "10485760".to_string())
            .parse::<usize>()?;

        let gemini_api_key = env::var("GEMINI_API_KEY")?;
        let gemini_endpoint = env::var("GEMINI_ENDPOINT")
            .unwrap_or_else(|_| "https://generativelanguage.googleapis.com/v1".to_string());
        let gemini_model =
            env::var("GEMINI_MODEL").unwrap_or_else(|_| "models/gemini-2.5-flash".to_string());

        let cors_allowed_origins = env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_else(|_| "http://localhost:3000".to_string())
            .split(',')
            .map(|s| s.trim().to_string())
            .collect();
        let rate_limit_requests = env::var("RATE_LIMIT_REQUESTS")
            .unwrap_or_else(|_| "100".to_string())
            .parse::<u32>()?;
        let rate_limit_window_seconds = env::var("RATE_LIMIT_WINDOW_SECONDS")
            .unwrap_or_else(|_| "60".to_string())
            .parse::<u64>()?;
        let max_request_size_bytes = env::var("MAX_REQUEST_SIZE_BYTES")
            .unwrap_or_else(|_| "52428800".to_string())
            .parse::<usize>()?;

        let logging_level = env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
        let logging_format = env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string());

        let analysis_enabled = env::var("ANALYSIS_WORKER_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            == "true";
        let analysis_interval_seconds = env::var("ANALYSIS_WORKER_INTERVAL_SECONDS")
            .unwrap_or_else(|_| "30".to_string())
            .parse::<u64>()?;
        let thumbnail_enabled = env::var("THUMBNAIL_GENERATION_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            == "true";
        let max_thumbnail_width = env::var("MAX_THUMBNAIL_WIDTH")
            .unwrap_or_else(|_| "400".to_string())
            .parse::<u32>()?;
        let max_thumbnail_height = env::var("MAX_THUMBNAIL_HEIGHT")
            .unwrap_or_else(|_| "400".to_string())
            .parse::<u32>()?;

        let stories_service_url =
            env::var("STORIES_SERVICE_URL").unwrap_or_else(|_| "http://localhost:8083".to_string());
        let webhooks_enabled = env::var("WEBHOOKS_ENABLED")
            .unwrap_or_else(|_| "true".to_string())
            .to_lowercase()
            == "true";

        Ok(Self {
            server: ServerConfig {
                host,
                port,
                workers,
                max_connections,
                timeout_seconds,
                keep_alive_seconds,
                client_timeout_seconds,
                client_shutdown_seconds,
            },
            database: DatabaseConfig {
                url: database_url,
                max_connections: db_max_connections,
                min_connections: db_min_connections,
                connect_timeout_seconds: db_connect_timeout,
                idle_timeout_seconds: db_idle_timeout,
                max_lifetime_seconds: db_max_lifetime,
            },
            storage: StorageConfig {
                aws_region,
                aws_access_key_id,
                aws_secret_access_key,
                s3_bucket,
                s3_endpoint,
                max_image_size_bytes,
            },
            ai: AIConfig {
                gemini_api_key,
                gemini_endpoint,
                gemini_model,
            },
            security: SecurityConfig {
                cors_allowed_origins,
                rate_limit_requests,
                rate_limit_window_seconds,
                max_request_size_bytes,
            },
            logging: LoggingConfig {
                level: logging_level,
                format: logging_format,
            },
            worker: WorkerConfig {
                analysis_enabled,
                analysis_interval_seconds,
                thumbnail_enabled,
                max_thumbnail_width,
                max_thumbnail_height,
            },
            webhooks: WebhookConfig {
                stories_service_url,
                enabled: webhooks_enabled,
            },
        })
    }
}
