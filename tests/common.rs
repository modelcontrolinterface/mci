use anyhow::Result;
use mci::s3::create_client;
use testcontainers_modules::{
    minio::MinIO,
    testcontainers::{runners::AsyncRunner, ContainerAsync},
};

pub async fn start_s3_server_and_client() -> Result<(ContainerAsync<MinIO>, aws_sdk_s3::Client)> {
    let container = MinIO::default().start().await?;
    let host = container.get_host().await?;
    let port = container.get_host_port_ipv4(9000).await?;
    let endpoint = format!("http://{host}:{port}");

    let client = create_client(&endpoint, "minioadmin", "minioadmin", "us-east-1").await;
    Ok((container, client))
}
