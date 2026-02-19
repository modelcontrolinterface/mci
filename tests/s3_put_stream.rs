use anyhow::{Context, Result};
use aws_smithy_types::byte_stream::ByteStream;
use mci::s3;
use sha2::Digest;
use uuid::Uuid;

mod common;

#[tokio::test]
async fn put_stream_uploads_object() -> Result<()> {
    let (container, client) = common::initialize_s3().await?;
    let bucket = format!("test-bucket-{}", Uuid::new_v4());
    client.create_bucket().bucket(&bucket).send().await?;

    let key = "hello.txt";
    let body = ByteStream::from_static(b"hello from put_stream");

    s3::put_stream(&client, &bucket, key, body, None).await?;

    let got = client
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .context("get_object failed")?;
    let bytes = got.body.collect().await?.into_bytes();
    assert_eq!(bytes.as_ref(), b"hello from put_stream");

    container.stop().await.ok();
    Ok(())
}

#[tokio::test]
async fn put_stream_validates_digest() -> Result<()> {
    let (container, client) = common::initialize_s3().await?;
    let bucket = format!("test-bucket-{}", Uuid::new_v4());
    client.create_bucket().bucket(&bucket).send().await?;

    let key = "digest.txt";
    let content = b"with digest check";
    let expected = format!("sha256:{:x}", sha2::Sha256::digest(content));

    s3::put_stream(
        &client,
        &bucket,
        key,
        ByteStream::from_static(&content[..]),
        Some(&expected),
    )
    .await?;

    let got = client.get_object().bucket(&bucket).key(key).send().await?;
    let bytes = got.body.collect().await?.into_bytes();
    assert_eq!(bytes.as_ref(), content);

    container.stop().await.ok();
    Ok(())
}

#[tokio::test]
async fn put_stream_rejects_bad_digest() -> Result<()> {
    let (container, client) = common::initialize_s3().await?;
    let bucket = format!("test-bucket-{}", Uuid::new_v4());
    client.create_bucket().bucket(&bucket).send().await?;

    let key = "bad-digest.txt";
    let body = ByteStream::from_static(b"oops");

    let result = s3::put_stream(
        &client,
        &bucket,
        key,
        body,
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000000"),
    )
    .await;

    assert!(result.is_err(), "expected digest mismatch error");

    container.stop().await.ok();
    Ok(())
}

#[tokio::test]
async fn put_stream_errors_on_invalid_digest_format() -> Result<()> {
    let (container, client) = common::initialize_s3().await?;
    let bucket = format!("test-bucket-{}", Uuid::new_v4());
    client.create_bucket().bucket(&bucket).send().await?;

    let key = "bad-format.txt";
    let body = ByteStream::from_static(b"content");

    let result = s3::put_stream(&client, &bucket, key, body, Some("badformat")).await;

    assert!(result.is_err(), "expected invalid digest format to error");

    container.stop().await.ok();
    Ok(())
}

#[tokio::test]
async fn put_stream_errors_on_unsupported_algorithm() -> Result<()> {
    let (container, client) = common::initialize_s3().await?;
    let bucket = format!("test-bucket-{}", Uuid::new_v4());
    client.create_bucket().bucket(&bucket).send().await?;

    let key = "unsupported-algo.txt";
    let body = ByteStream::from_static(b"content");

    let result = s3::put_stream(&client, &bucket, key, body, Some("md5:abcd")).await;

    assert!(result.is_err(), "expected unsupported algorithm to error");

    container.stop().await.ok();
    Ok(())
}

#[tokio::test]
async fn put_stream_handles_empty_body() -> Result<()> {
    let (container, client) = common::initialize_s3().await?;
    let bucket = format!("test-bucket-{}", Uuid::new_v4());
    client.create_bucket().bucket(&bucket).send().await?;

    let key = "empty.txt";
    let body = ByteStream::from_static(b"");

    s3::put_stream(&client, &bucket, key, body, None).await?;

    let got = client
        .get_object()
        .bucket(&bucket)
        .key(key)
        .send()
        .await
        .context("get_object failed")?;
    let bytes = got.body.collect().await?.into_bytes();
    assert_eq!(bytes.as_ref(), b"", "expected empty body round-trip");

    container.stop().await.ok();
    Ok(())
}
