use anyhow::{Context, Result};
use aws_sdk_s3::{
    config::{Credentials, Region},
    primitives::ByteStream,
    Client,
};
use bytes::Bytes;
use sha2::{Digest, Sha256};

pub async fn create_client(
    endpoint_url: &str,
    access_key: &str,
    secret_key: &str,
    region: &str,
) -> Client {
    let s3_config = aws_sdk_s3::Config::builder()
        .endpoint_url(endpoint_url)
        .credentials_provider(Credentials::new(
            access_key.to_string(),
            secret_key.to_string(),
            None,
            None,
            "mci-storage",
        ))
        .region(Region::new(region.to_string()))
        .force_path_style(true)
        .build();

    Client::from_conf(s3_config)
}

pub async fn put_stream(
    client: &Client,
    bucket: &str,
    key: &str,
    body: ByteStream,
    expected_digest: Option<&str>,
) -> Result<()> {
    let body = if let Some(expected_digest) = expected_digest {
        let (algorithm, expected_hash) = expected_digest
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("Invalid digest format, expected 'algorithm:hash'"))?;

        let mut all_bytes = Vec::new();
        let computed_hash = match algorithm {
            "sha256" => {
                let bytes = body.collect().await?.into_bytes();
                let mut hasher = Sha256::new();
                hasher.update(&bytes);
                all_bytes.extend_from_slice(&bytes);
                format!("{:x}", hasher.finalize())
            }
            _ => anyhow::bail!("Unsupported hash algorithm: {}", algorithm),
        };

        if computed_hash != expected_hash {
            anyhow::bail!(
                "Digest mismatch: expected {}, got {}:{}",
                expected_digest,
                algorithm,
                computed_hash
            );
        }

        ByteStream::from(Bytes::from(all_bytes))
    } else {
        body
    };

    client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(body)
        .send()
        .await
        .context("Failed to upload object to S3")?;

    Ok(())
}
