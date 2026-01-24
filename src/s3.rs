use aws_config::meta::region::RegionProviderChain;
use aws_sdk_s3::{
    config::{Credentials, Region},
    primitives::ByteStream,
    Client,
};

pub async fn create_s3_client(s3_url: &str, s3_access_key: &str, s3_secret_key: &str) -> Client {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    let config = aws_config::from_env().region(region_provider).load().await;
    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .endpoint_url(s3_url)
        .credentials_provider(Credentials::new(
            s3_access_key,
            s3_secret_key,
            None,
            None,
            "seaweedfs",
        ))
        .region(Region::new("us-east-1"))
        .force_path_style(true)
        .build();

    Client::from_conf(s3_config)
}

pub async fn upload_stream(
    client: &Client,
    bucket: &str,
    key: &str,
    body: ByteStream,
) -> Result<(), Box<dyn std::error::Error>> {
    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await?;

    Ok(())
}
