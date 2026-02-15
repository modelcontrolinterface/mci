use mci::s3::create_client;

#[cfg(test)]
mod tests {
    use super::*;
    use testcontainers_modules::minio::MinIO;
    use testcontainers_modules::testcontainers::runners::AsyncRunner;

    #[tokio::test]
    async fn test_client_connects_to_minio() {
        let minio = MinIO::default().start().await.unwrap();

        let host = minio.get_host().await.unwrap();
        let port = minio.get_host_port_ipv4(9000).await.unwrap();

        let client = create_client(
            &format!("http://{}:{}", host, port),
            "minioadmin",
            "minioadmin",
            "us-east-1",
        )
        .await;

        let result = client.list_buckets().send().await;
        assert!(
            result.is_ok(),
            "Failed to connect to MinIO: {:?}",
            result.err()
        );
    }
}
