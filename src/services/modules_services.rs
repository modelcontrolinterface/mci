use crate::{
    db::DbConnection,
    models::{Module, ModuleType, NewModule, UpdateModule},
    s3,
    schema::modules,
    utils::{source_utils, stream_utils},
};
use anyhow::{Context, Result};
use aws_sdk_s3;
use aws_smithy_types::byte_stream::ByteStream;
use diesel::{associations::HasTable, prelude::*};
use futures::stream::TryStreamExt;
use http_body_util::StreamBody;
use reqwest;
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::fs;

fn ensure_wasm_file(file_url: &str) -> Result<()> {
    if !file_url.to_lowercase().ends_with(".wasm") {
        anyhow::bail!("Modules must reference a .wasm file");
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub enum SortBy {
    Id,
    Name,
    Type,
}

#[derive(Debug, Deserialize)]
pub enum SortOrder {
    Asc,
    Desc,
}

#[derive(Debug, Deserialize, Default)]
pub struct ModuleFilter {
    pub query: Option<String>,
    pub is_enabled: Option<bool>,
    pub r#type: Option<ModuleType>,
    pub limit: Option<i32>,
    pub offset: Option<i32>,
    pub sort_by: Option<SortBy>,
    pub sort_order: Option<SortOrder>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModulePayload {
    pub id: String,
    pub name: String,
    pub r#type: ModuleType,
    pub description: String,
    pub file_url: String,
    pub digest: String,
    pub source_url: Option<String>,
}

async fn fetch_module_from_path(path: &Path) -> Result<ModulePayload> {
    let content = fs::read_to_string(path)
        .await
        .context("Failed to read module file")?;
    let module_payload =
        serde_json::from_str::<ModulePayload>(&content).context("Failed to parse module JSON")?;

    Ok(module_payload)
}

async fn fetch_module_from_url(http_client: &reqwest::Client, url: &str) -> Result<ModulePayload> {
    let module_payload = http_client
        .get(url)
        .header("User-Agent", "MCI/1.0")
        .send()
        .await
        .context("Failed to send HTTP request")?
        .error_for_status()
        .context("HTTP request returned error status")?
        .json::<ModulePayload>()
        .await
        .context("Failed to parse module JSON from response")?;

    Ok(module_payload)
}

async fn fetch_module(
    http_client: &reqwest::Client,
    source: &source_utils::Source,
) -> Result<ModulePayload> {
    match source {
        source_utils::Source::Http(url) => fetch_module_from_url(http_client, url).await,
        source_utils::Source::File(path) => fetch_module_from_path(path).await,
    }
}

fn db_create_module(conn: &mut DbConnection, new_module: &NewModule) -> QueryResult<Module> {
    diesel::insert_into(modules::table)
        .values(new_module)
        .returning(Module::as_returning())
        .get_result(conn)
}

fn db_update_module(
    conn: &mut DbConnection,
    module_id: &str,
    update_module: &UpdateModule,
) -> QueryResult<Module> {
    diesel::update(modules::table.find(module_id))
        .set(update_module)
        .returning(Module::as_returning())
        .get_result(conn)
}

pub fn get_module(conn: &mut DbConnection, module_id: &str) -> QueryResult<Module> {
    modules::table
        .find(module_id)
        .select(Module::as_select())
        .first(conn)
}

pub fn delete_module(conn: &mut DbConnection, module_id: &str) -> QueryResult<usize> {
    diesel::delete(modules::table.find(module_id)).execute(conn)
}

pub fn update_module(
    conn: &mut DbConnection,
    module_id: &str,
    update_module: &UpdateModule,
) -> QueryResult<Module> {
    db_update_module(conn, module_id, update_module)
}

pub fn list_modules(conn: &mut DbConnection, filter: &ModuleFilter) -> QueryResult<Vec<Module>> {
    use crate::schema::modules::dsl::*;

    let mut query = modules::table().into_boxed();

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
    if let Some(ref module_type_filter) = filter.r#type {
        query = query.filter(type_.eq(module_type_filter));
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

    query.select(Module::as_select()).load(conn)
}

pub async fn create_module(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    payload: &ModulePayload,
) -> Result<Module> {
    if get_module(conn, &payload.id).is_ok() {
        anyhow::bail!("Conflict: Module with ID '{}' already exists", payload.id);
    }

    ensure_wasm_file(&payload.file_url)?;
    let module_source = source_utils::Source::parse(&payload.file_url)?;
    let obj_key = format!("{}.wasm", payload.id);

    let body = match &module_source {
        source_utils::Source::Http(url) => {
            let response = stream_utils::stream_content_from_url(http_client, url)
                .await
                .context("Failed to fetch module file from URL")?;

            let stream = response.bytes_stream();
            let frames = stream.map_ok(hyper::body::Frame::data);
            let body = StreamBody::new(frames);
            ByteStream::from_body_1_x(body)
        }
        source_utils::Source::File(path) => stream_utils::stream_content_from_path(path)
            .await
            .context("Failed to read module file from path")?,
    };

    s3::put_stream(s3_client, "modules", &obj_key, body, Some(&payload.digest))
        .await
        .context("Failed to upload module to S3")?;

    let new_module = NewModule {
        id: payload.id.clone(),
        type_: payload.r#type,
        name: payload.name.clone(),
        description: payload.description.clone(),
        module_object_key: obj_key.clone(),
        configuration_object_key: obj_key.clone(),
        secrets_object_key: obj_key.clone(),
        digest: payload.digest.clone(),
        source_url: payload.source_url.clone(),
    };

    db_create_module(conn, &new_module).context("Failed to save module to database")
}

pub async fn create_module_from_registry(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    source_input: &str,
) -> Result<Module> {
    let source = source_utils::Source::parse(source_input)?;
    let mut payload = fetch_module(http_client, &source)
        .await
        .context("Failed to load module metadata")?;

    if payload.source_url.is_none() {
        payload.source_url = Some(source_input.to_string());
    }

    create_module(conn, http_client, s3_client, &payload).await
}

pub async fn update_module_from_source(
    conn: &mut DbConnection,
    http_client: &reqwest::Client,
    s3_client: &aws_sdk_s3::Client,
    module_id: &str,
) -> Result<Module> {
    let module =
        get_module(conn, module_id).context("Failed to fetch current module from database")?;
    let source_url_str = module
        .source_url
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Module does not have a source_url to update from"))?;
    let source = source_utils::Source::parse(source_url_str)?;
    let remote_payload = fetch_module(http_client, &source)
        .await
        .context("Failed to fetch updated module metadata from source")?;

    if module.digest == remote_payload.digest {
        return Ok(module);
    }

    ensure_wasm_file(&remote_payload.file_url)?;
    let module_file_source = source_utils::Source::parse(&remote_payload.file_url)?;
    let obj_key = format!("{}.wasm", module.id);
    let body = match &module_file_source {
        source_utils::Source::Http(url) => {
            let response = stream_utils::stream_content_from_url(http_client, url)
                .await
                .context("Failed to fetch updated module file from URL")?;
            let stream = response.bytes_stream();
            let frames = stream.map_ok(hyper::body::Frame::data);
            let body = StreamBody::new(frames);

            ByteStream::from_body_1_x(body)
        }
        source_utils::Source::File(path) => stream_utils::stream_content_from_path(path)
            .await
            .context("Failed to read updated module file from path")?,
    };

    s3::put_stream(
        s3_client,
        "modules",
        &obj_key,
        body,
        Some(&remote_payload.digest),
    )
    .await
    .context("Failed to upload updated module to S3")?;

    let update_data = UpdateModule {
        type_: Some(remote_payload.r#type),
        digest: Some(remote_payload.digest),
        name: Some(remote_payload.name),
        description: Some(remote_payload.description),
        ..Default::default()
    };

    db_update_module(conn, module_id, &update_data).context("Failed to update module in database")
}
