use anyhow::Result;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use mci::{db, s3};
use testcontainers_modules::{
    minio, postgres,
    testcontainers::{runners::AsyncRunner, ContainerAsync},
};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

pub async fn initialize_s3() -> Result<(ContainerAsync<minio::MinIO>, aws_sdk_s3::Client)> {
    let container = minio::MinIO::default().start().await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(9000).await?;

    let endpoint = format!("http://{host}:{port}");
    let client = s3::create_client(&endpoint, "minioadmin", "minioadmin", "us-east-1").await;

    Ok((container, client))
}

pub async fn initialize_pg() -> Result<(ContainerAsync<postgres::Postgres>, db::PgPool)> {
    let container = postgres::Postgres::default().start().await?;

    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(5432).await?;

    let conn_str = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = tokio::task::spawn_blocking(move || db::create_pool(&conn_str)).await?;
    let migration_pool = pool.clone();

    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut conn = migration_pool.get()?;
        conn.run_pending_migrations(MIGRATIONS)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    })
    .await??;

    Ok((container, pool))
}
