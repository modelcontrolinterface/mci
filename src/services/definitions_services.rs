use crate::{
    db::DbConnection,
    models::{Definition, NewDefinition, UpdateDefinition},
    schema::definitions,
    utils::stream_utils,
};
use anyhow::{Context, Result};
use aws_sdk_s3;
use aws_smithy_types::byte_stream::ByteStream;
use bytes::Bytes;
use diesel::{associations::HasTable, prelude::*};
use futures::stream::TryStreamExt;
use http_body_util::StreamBody;
use reqwest;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use tokio::fs;
use url::Url;

#[derive(Debug, Deserialize)]
pub enum SortBy {
    Id,
    Type,
    Name,
}

#[derive(Debug, Deserialize)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Deserialize, Default)]
pub struct DefinitionFilter {
    pub query: Option<String>,
    pub is_enabled: Option<bool>,
    pub r#type: Option<String>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub sort_by: Option<SortBy>,
    pub sort_order: Option<SortOrder>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DefinitionPayload {
    pub id: String,
    pub name: String,
    pub r#type: String,
    pub description: String,
    pub file_url: String,
    pub digest: String,
    pub source_url: Option<String>,
}

#[derive(Debug, Clone)]
pub enum DefinitionSource {
    Http(String),
    File(String),
}

impl DefinitionSource {
    pub fn parse(input: &str) -> Self {
        match Url::parse(input) {
            Ok(url) if url.scheme() == "http" || url.scheme() == "https" => {
                Self::Http(input.to_string())
            }
            Ok(url) if url.scheme() == "file" => Self::File(input.to_string()),
            _ => Self::File(input.to_string()),
        }
    }
}

async fn fetch_definition_from_path(path: &str) -> Result<DefinitionPayload> {
    let resolved_path = if let Ok(url) = Url::parse(path) {
        url.to_file_path()
            .map_err(|()| anyhow::anyhow!("Invalid file URL: {}", path))?
    } else {
        Path::new(path).to_path_buf()
    };

    if !resolved_path.is_file() {
        anyhow::bail!("Path is not a file: {}", resolved_path.display());
    }

    let content = fs::read_to_string(&resolved_path)
        .await
        .context("Failed to read definition file")?;
    let definition_payload = serde_json::from_str::<DefinitionPayload>(&content)
        .context("Failed to parse definition JSON")?;

    Ok(definition_payload)
}

async fn fetch_definition_from_url(
    http_client: &reqwest::Client,
    url: &str,
) -> Result<DefinitionPayload> {
    let definition_payload = http_client
        .get(url)
        .header("User-Agent", "MCI/1.0")
        .send()
        .await
        .context("Failed to send HTTP request")?
        .error_for_status()
        .context("HTTP request returned error status")?
        .json::<DefinitionPayload>()
        .await
        .context("Failed to parse definition JSON from response")?;

    Ok(definition_payload)
}

async fn s3_put_stream(
    client: &aws_sdk_s3::Client,
    key: &str,
    body: ByteStream,
    expected_digest: &str,
) -> Result<()> {
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

    let verified_body = ByteStream::from(Bytes::from(all_bytes));

    client
        .put_object()
        .bucket("definitions")
        .key(key)
        .body(verified_body)
        .send()
        .await
        .context("Failed to upload object to S3")?;

    Ok(())
}

fn db_create_definition(
    conn: &mut DbConnection,
    new_definition: &NewDefinition,
) -> QueryResult<Definition> {
    diesel::insert_into(definitions::table)
        .values(new_definition)
        .returning(Definition::as_returning())
        .get_result(conn)
}

fn db_update_definition(
    conn: &mut DbConnection,
    definition_id: &str,
    update_definition: &UpdateDefinition,
) -> QueryResult<Definition> {
    diesel::update(definitions::table.find(definition_id))
        .set(update_definition)
        .returning(Definition::as_returning())
        .get_result(conn)
}

pub fn get_definition(conn: &mut DbConnection, definition_id: &str) -> QueryResult<Definition> {
    definitions::table
        .find(definition_id)
        .select(Definition::as_select())
        .first(conn)
}

pub fn list_definitions(
    conn: &mut DbConnection,
    filter: &DefinitionFilter,
) -> QueryResult<Vec<Definition>> {
    use crate::schema::definitions::dsl::*;

    let mut query = definitions::table().into_boxed();

    if let Some(ref search_query) = filter.query {
        query = query.filter(
            id.ilike(format!("%{}%", search_query))
                .or(name.ilike(format!("%{}%", search_query)))
                .or(description.ilike(format!("%{}%", search_query))),
        );
    }

    if let Some(enabled_filter) = filter.is_enabled {
        query = query.filter(is_enabled.eq(enabled_filter));
    }
    if let Some(ref definition_type_filter) = filter.r#type {
        query = query.filter(type_.eq(definition_type_filter));
    }

    match (&filter.sort_by, &filter.sort_order) {
        (Some(SortBy::Id), Some(SortOrder::Desc)) => query = query.order(id.desc()),
        (Some(SortBy::Id), _) => query = query.order(id.asc()),
        (Some(SortBy::Type), Some(SortOrder::Desc)) => query = query.order(type_.desc()),
        (Some(SortBy::Type), _) => query = query.order(type_.asc()),
        (Some(SortBy::Name), Some(SortOrder::Desc)) => query = query.order(name.desc()),
        (Some(SortBy::Name), _) => query = query.order(name.asc()),
        (None, _) => {}
    }

    if let Some(limit_val) = filter.limit {
        query = query.limit(limit_val as i64);
    }
    if let Some(offset_val) = filter.offset {
        query = query.offset(offset_val as i64);
    }

    query.select(Definition::as_select()).load(conn)
}

pub fn delete_definition(conn: &mut DbConnection, definition_id: &str) -> QueryResult<usize> {
    diesel::delete(definitions::table.find(definition_id)).execute(conn)
}

pub fn update_definition(
    conn: &mut DbConnection,
    definition_id: &str,
    update_definition: &UpdateDefinition,
) -> QueryResult<Definition> {
    db_update_definition(conn, definition_id, update_definition)
}

pub async fn create_definition(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    payload: &DefinitionPayload,
) -> Result<Definition> {
    if get_definition(conn, &payload.id).is_ok() {
        anyhow::bail!(
            "Conflict: Definition with ID '{}' already exists",
            payload.id
        );
    }

    let definition_url = DefinitionSource::parse(&payload.file_url);
    let obj_key = payload.id.clone();

    let body = match &definition_url {
        DefinitionSource::Http(url) => {
            let response = stream_utils::stream_content_from_url(http_client, url)
                .await
                .context("Failed to fetch definition file from URL")?;

            let stream = response.bytes_stream();
            let frames = stream.map_ok(|bytes| hyper::body::Frame::data(bytes));
            let body = StreamBody::new(frames);
            ByteStream::from_body_1_x(body)
        }
        DefinitionSource::File(path) => stream_utils::stream_content_from_path(path)
            .await
            .context("Failed to read definition file from path")?,
    };

    s3_put_stream(s3_client, &obj_key, body, &payload.digest)
        .await
        .context("Failed to upload definition to S3")?;

    let new_definition = NewDefinition {
        id: payload.id.clone(),
        type_: payload.r#type.clone(),
        name: payload.name.clone(),
        description: payload.description.clone(),
        definition_object_key: obj_key.clone(),
        configuration_object_key: obj_key.clone(),
        secrets_object_key: obj_key.clone(),
        digest: payload.digest.clone(),
        source_url: payload.source_url.clone(),
    };

    db_create_definition(conn, &new_definition)
        .context("Failed to save definition to database")
        .map_err(Into::into)
}

pub async fn create_definition_from_registry(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    source_input: &str,
) -> Result<Definition> {
    let source_url = DefinitionSource::parse(source_input);
    let mut payload = match &source_url {
        DefinitionSource::Http(url) => fetch_definition_from_url(http_client, url)
            .await
            .context("Failed to fetch definition metadata from registry")?,
        DefinitionSource::File(path) => fetch_definition_from_path(path)
            .await
            .context("Failed to read definition metadata from file")?,
    };

    if payload.source_url.is_none() {
        payload.source_url = Some(source_input.to_string());
    }

    create_definition(conn, http_client, s3_client, &payload).await
}

pub async fn update_definition_from_source(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    definition_id: &str,
) -> Result<Definition> {
    let definition = get_definition(conn, definition_id)
        .context("Failed to fetch current definition from database")?;
    let source_url_str = definition
        .source_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Definition does not have a source_url to update from"))?;
    let source = DefinitionSource::parse(source_url_str);
    let remote_payload = match &source {
        DefinitionSource::Http(url) => fetch_definition_from_url(http_client, url)
            .await
            .context("Failed to fetch updated definition metadata from registry")?,
        DefinitionSource::File(path) => fetch_definition_from_path(path)
            .await
            .context("Failed to read updated definition metadata from file")?,
    };

    if definition.digest == remote_payload.digest {
        return Ok(definition);
    }

    let definition_file_source = DefinitionSource::parse(&remote_payload.file_url);
    let obj_key = definition.id.clone();
    let body = match &definition_file_source {
        DefinitionSource::Http(url) => {
            let response = stream_utils::stream_content_from_url(http_client, url)
                .await
                .context("Failed to fetch updated definition file from URL")?;
            let stream = response.bytes_stream();
            let frames = stream.map_ok(|bytes| hyper::body::Frame::data(bytes));
            let body = StreamBody::new(frames);

            ByteStream::from_body_1_x(body)
        }
        DefinitionSource::File(path) => stream_utils::stream_content_from_path(path)
            .await
            .context("Failed to read updated definition file from path")?,
    };

    s3_put_stream(s3_client, &obj_key, body, &remote_payload.digest)
        .await
        .context("Failed to upload updated definition to S3")?;

    let update_data = UpdateDefinition {
        type_: Some(remote_payload.r#type),
        digest: Some(remote_payload.digest),
        name: Some(remote_payload.name),
        description: Some(remote_payload.description),
        ..Default::default()
    };

    db_update_definition(conn, definition_id, &update_data)
        .context("Failed to update definition in database")
        .map_err(Into::into)
}
