use aws_config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;
use aws_sdk_s3::config::SharedCredentialsProvider;
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::{Client, Config};
use std::time::Duration;

use crate::config::StorageConfig;

pub struct S3Service {
    client: Client,
    bucket: String,
}

impl S3Service {
    pub async fn new(
        config: &StorageConfig,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let credentials = Credentials::new(
            &config.aws_access_key_id,
            &config.aws_secret_access_key,
            None,
            None,
            "static",
        );

        let credentials_provider = SharedCredentialsProvider::new(credentials);
        let region = Region::new(config.aws_region.clone());

        let mut s3_config_builder = Config::builder()
            .behavior_version(BehaviorVersion::latest())
            .credentials_provider(credentials_provider)
            .region(region);

        // Support for custom S3 endpoints (MinIO, LocalStack)
        if let Some(endpoint) = &config.s3_endpoint {
            s3_config_builder = s3_config_builder.endpoint_url(endpoint);
            s3_config_builder = s3_config_builder.force_path_style(true);
        }

        let s3_config = s3_config_builder.build();
        let client = Client::from_conf(s3_config);

        log::info!("S3 service initialized with bucket: {}", config.s3_bucket);

        Ok(Self {
            client,
            bucket: config.s3_bucket.clone(),
        })
    }

    /// Generate presigned PUT URL for upload
    pub async fn generate_presigned_put_url(
        &self,
        object_key: &str,
        content_type: &str,
        expires_in_seconds: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        log::info!(
            "inicio ******** 2 - generate_presigned_put_url start: {}",
            object_key
        );
        let presigning_config =
            PresigningConfig::expires_in(Duration::from_secs(expires_in_seconds))?;

        let presigned_request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(object_key)
            .content_type(content_type)
            .presigned(presigning_config)
            .await?;

        Ok(presigned_request.uri().to_string())
    }

    /// Generate presigned GET URL for download
    pub async fn generate_presigned_get_url(
        &self,
        object_key: &str,
        expires_in_seconds: u64,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        log::info!(
            "inicio ******** 2b - generate_presigned_get_url start: {}",
            object_key
        );
        let presigning_config =
            PresigningConfig::expires_in(Duration::from_secs(expires_in_seconds))?;

        let presigned_request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(object_key)
            .presigned(presigning_config)
            .await?;

        log::info!(
            "fin ********2b - generate_presigned_get_url end: {}",
            object_key
        );
        Ok(presigned_request.uri().to_string())
    }

    /// Generate presigned GET URL for download
    // (duplicate removed) the presigned GET url method is defined above with logging

    /// Upload bytes directly to S3
    pub async fn upload_bytes(
        &self,
        object_key: &str,
        data: Vec<u8>,
        content_type: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        log::info!(
            "inicio ******** 2 - upload_bytes start: {} ({} bytes)",
            object_key,
            data.len()
        );
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(object_key)
            .body(data.into())
            .content_type(content_type)
            .send()
            .await?;

        let public = format!("https://{}.s3.amazonaws.com/{}", self.bucket, object_key);
        log::info!("fin ********2 - upload_bytes end: {}", object_key);
        Ok(public)
    }

    /// Download object from S3
    pub async fn download_object(
        &self,
        object_key: &str,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
        log::info!("inicio ******** 2 - download_object start: {}", object_key);
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(object_key)
            .send()
            .await?;

        let body = response.body.collect().await?;
        let bytes = body.into_bytes();
        log::info!(
            "fin ********2 - download_object end: {} ({} bytes)",
            object_key,
            bytes.len()
        );
        Ok(bytes.to_vec())
    }

    /// Delete object from S3
    pub async fn delete_object(
        &self,
        object_key: &str,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(object_key)
            .send()
            .await?;

        Ok(())
    }

    /// Get public URL for object
    pub fn get_public_url(&self, object_key: &str) -> String {
        let url = format!("https://{}.s3.amazonaws.com/{}", self.bucket, object_key);
        log::info!(
            "inicio ******** 2c - get_public_url: {} -> {}",
            object_key,
            url
        );
        log::info!("fin ********2c - get_public_url end: {}", object_key);
        url
    }

    /// Generate unique object key
    pub fn generate_object_key(filename: &str) -> String {
        let uuid = uuid::Uuid::new_v4();
        let timestamp = chrono::Utc::now().timestamp();
        let extension = std::path::Path::new(filename)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("jpg");

        format!("captures/{}/{}.{}", timestamp, uuid, extension)
    }
}
