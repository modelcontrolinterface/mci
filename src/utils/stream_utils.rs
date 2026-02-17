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

pub async fn stream_content_from_path(path: impl AsRef<Path>) -> Result<ByteStream> {
    Ok(ByteStream::from_path(path).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    mod http_tests {
        use super::*;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        #[tokio::test]
        async fn test_stream_url_success() {
            let client = reqwest::Client::new();
            let server = MockServer::start().await;

            Mock::given(method("GET"))
                .and(path("/hello"))
                .respond_with(ResponseTemplate::new(200).set_body_string("content"))
                .mount(&server)
                .await;

            let res = stream_content_from_url(&client, &format!("{}/hello", &server.uri())).await;

            assert!(res.is_ok());
            assert_eq!(res.unwrap().text().await.unwrap(), "content");
        }

        #[tokio::test]
        async fn test_stream_url_404_error() {
            let client = reqwest::Client::new();
            let server = MockServer::start().await;

            Mock::given(method("GET"))
                .respond_with(ResponseTemplate::new(404))
                .mount(&server)
                .await;

            let res = stream_content_from_url(&client, &server.uri()).await;
            assert!(res.is_err());
        }
    }

    mod file_tests {
        use super::*;
        use std::io::Write;
        use tempfile::NamedTempFile;

        #[tokio::test]
        async fn test_stream_path_success() {
            let mut file = NamedTempFile::new().unwrap();
            writeln!(file, "file content").unwrap();

            let path = file.path().to_path_buf();
            let stream = stream_content_from_path(&path).await;
            assert!(stream.is_ok());

            let data = stream.unwrap().collect().await.unwrap().to_vec();
            assert_eq!(data, b"file content\n");
        }

        #[tokio::test]
        async fn test_stream_path_missing_file() {
            let res = stream_content_from_path("/NA.txt").await;
            assert!(res.is_err());
        }
    }
}
