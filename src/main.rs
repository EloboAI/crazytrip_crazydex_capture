mod ai;
mod config;
mod database;
mod handlers;
mod models;
mod storage;
mod webhooks;
mod workers;

use actix_web::{middleware as actix_middleware, web, App, HttpServer};
use dotenvy::dotenv;
use std::sync::Arc;

use ai::AIService;
use config::AppConfig;
use database::DatabaseService;
use handlers::*;
use storage::S3Service;
use webhooks::WebhookClient;
use workers::AnalysisWorker;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Load .env file
    let _ = dotenv();

    // Load configuration
    let config = match AppConfig::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize logging
    if let Ok(logger) = flexi_logger::Logger::try_with_str(config.logging.level.clone()) {
        let file_spec = flexi_logger::FileSpec::default()
            .directory("logs")
            .suppress_timestamp();
        let _ = logger
            .log_to_file(file_spec)
            .duplicate_to_stdout(flexi_logger::Duplicate::Info)
            .start();
    } else {
        env_logger::builder()
            .filter_level(log::LevelFilter::Info)
            .format_timestamp_secs()
            .init();
    }

    log::info!("Starting CrazyTrip Crazydex Capture Service v{}", env!("CARGO_PKG_VERSION"));
    log::info!("Server: {}:{}", config.server.host, config.server.port);

    // Initialize database
    let db_service = match DatabaseService::new(&config.database).await {
        Ok(db) => Arc::new(db),
        Err(e) => {
            log::error!("Failed to initialize database: {}", e);
            eprintln!("Failed to initialize database: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize schema
    if let Err(e) = db_service.init_schema().await {
        log::error!("Failed to initialize DB schema: {}", e);
    } else {
        log::info!("DB schema ensured");
    }

    // Initialize S3 service
    let s3_service = match S3Service::new(&config.storage).await {
        Ok(s3) => Arc::new(s3),
        Err(e) => {
            log::error!("Failed to initialize S3 service: {}", e);
            eprintln!("Failed to initialize S3 service: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize AI service
    let ai_service = Arc::new(AIService::new(&config.ai));

    // Initialize webhook client
    let webhook_client = Arc::new(WebhookClient::new(config.webhooks.stories_service_url.clone()));

    // Spawn analysis worker if enabled
    if config.worker.analysis_enabled {
        let worker = AnalysisWorker::new(
            Arc::clone(&db_service),
            Arc::clone(&s3_service),
            Arc::clone(&ai_service),
            config.worker.analysis_interval_seconds,
        );

        tokio::spawn(async move {
            worker.start().await;
        });
    }

    // Print access information
    println!("🚀 CrazyTrip Crazydex Capture Service started!");
    println!("📍 Local access: http://{}:{}", config.server.host, config.server.port);
    println!("📍 Health check: http://{}:{}/api/v1/health", config.server.host, config.server.port);
    println!("🌍 Environment: {}", config.logging.level);
    println!("📝 Press Ctrl+C to stop the server");
    println!();

    // Create and run HTTP server
    HttpServer::new(move || {
        App::new()
            // Shared data
            .app_data(web::Data::new(Arc::clone(&db_service)))
            .app_data(web::Data::new(Arc::clone(&s3_service)))
            .app_data(web::Data::new(Arc::clone(&ai_service)))
            .app_data(web::Data::new(Arc::clone(&webhook_client)))
            .app_data(web::Data::new(config.webhooks.enabled))
            // Middleware
            .wrap(actix_middleware::Logger::default())
            .wrap(actix_middleware::Compress::default())
            .wrap(
                actix_cors::Cors::default()
                    .allow_any_origin()
                    .allow_any_method()
                    .allow_any_header()
                    .max_age(3600),
            )
            // Routes
            .service(
                web::scope("/api/v1")
                    .route("/health", web::get().to(health_check))
                    .route("/uploads/presign", web::post().to(generate_presigned_url))
                    .route("/captures", web::post().to(create_capture))
                    .route("/captures", web::get().to(list_captures))
                    .route("/captures/{id}", web::get().to(get_capture))
                    .route("/captures/{id}", web::patch().to(update_capture))
                    .route("/captures/{id}", web::delete().to(delete_capture))
                    .route("/captures/{id}/publish", web::patch().to(publish_capture))
                    .route("/captures/{id}/unpublish", web::patch().to(unpublish_capture))
                    .route("/sync/upload", web::post().to(sync_upload))
            )
    })
    .bind((config.server.host.clone(), config.server.port))?
    .workers(config.server.workers)
    .keep_alive(std::time::Duration::from_secs(config.server.keep_alive_seconds))
    .client_request_timeout(std::time::Duration::from_secs(config.server.client_timeout_seconds))
    .client_disconnect_timeout(std::time::Duration::from_secs(config.server.client_shutdown_seconds))
    .max_connections(config.server.max_connections)
    .run()
    .await
}
