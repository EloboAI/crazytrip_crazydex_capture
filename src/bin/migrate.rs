use dotenvy::dotenv;
use glob::glob;
use std::fs;
use tokio_postgres::{NoTls};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = dotenv();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in environment");

    let (mut client, connection) = tokio_postgres::connect(&database_url, NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    client.execute(
        "\
        CREATE TABLE IF NOT EXISTS schema_migrations (\
            version VARCHAR(50) PRIMARY KEY,\
            description TEXT,\
            installed_on TIMESTAMPTZ NOT NULL DEFAULT NOW()\
        )",
        &[],
    ).await?;

    let mut migrations: Vec<String> = Vec::new();
    for entry in glob("migrations/V*.sql")? {
        if let Ok(path) = entry {
            migrations.push(path.to_string_lossy().to_string());
        }
    }

    migrations.sort();

    if migrations.is_empty() {
        println!("No migration files found in migrations/");
        return Ok(());
    }

    for file in migrations {
        let name = std::path::Path::new(&file)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&file)
            .to_string();

        let row = client
            .query_opt("SELECT version FROM schema_migrations WHERE version = $1", &[&name])
            .await?;

        if row.is_some() {
            println!("Skipping already-applied migration: {}", name);
            continue;
        }

        println!("Applying migration: {}", name);
        let sql = fs::read_to_string(&file)?;

        let txn = client.transaction().await?;
        txn.batch_execute(&sql).await?;
        txn.execute(
            "INSERT INTO schema_migrations (version, description) VALUES ($1, $2)",
            &[&name, &name],
        ).await?;
        txn.commit().await?;

        println!("Applied: {}", name);
    }

    println!("Migrations complete");
    Ok(())
}
