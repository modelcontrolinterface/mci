use anyhow::Result;
use aws_smithy_types::byte_stream::ByteStream;
use reqwest;
use std::path::Path;

pub async fn stream_content_from_url(
    http_client: &reqwest::Client,
    url: &str,
) -> Result<reqwest::Response> {
    let response = http_client.get(url).send().await?.error_for_status()?;

    Ok(response)
}

pub async fn stream_content_from_path(path: &str) -> Result<ByteStream> {
    Ok(ByteStream::from_path(Path::new(path)).await?)
}
