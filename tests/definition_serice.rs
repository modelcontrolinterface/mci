use anyhow::Result;
use diesel::prelude::*;
use mci::{
    models::{Definition, NewDefinition},
    schema::definitions::dsl::*,
    services::definitions_services::{
        create_definition, create_definition_from_registry, list_definitions,
        update_definition_from_source, DefinitionFilter, DefinitionPayload, SortBy, SortOrder,
    },
};
use sha2::{Digest, Sha256};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

mod common;

#[tokio::test]
async fn create_definition_from_http_source() -> Result<()> {
    let (pg_container, pool) = common::initialize_pg().await?;
    let (s3_container, s3_client) = common::initialize_s3().await?;

    s3_client
        .create_bucket()
        .bucket("definitions")
        .send()
        .await?;

    let mock = MockServer::start().await;
    let file_body = b"hello-def";
    let digest_str = format!("sha256:{:x}", Sha256::digest(file_body));

    Mock::given(method("GET"))
        .and(path("/file.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(file_body, "application/json"))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/meta.json"))
        .and(header("User-Agent", "MCI/1.0"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(&DefinitionPayload {
                id: "def-1".into(),
                name: "Example Name".into(),
                r#type: "example-type".into(),
                description: "Example description".into(),
                file_url: format!("{}/file.json", mock.uri()),
                digest: digest_str.clone(),
                source_url: Some(format!("{}/meta.json", mock.uri())),
            }),
        )
        .mount(&mock)
        .await;

    let digest_for_task = digest_str.clone();

    tokio::task::spawn_blocking({
        let pool = pool.clone();
        let http_client = reqwest::Client::new();
        let s3_client = s3_client.clone();
        let meta_url = format!("{}/meta.json", mock.uri());

        move || -> Result<Definition> {
            let mut conn = pool.get()?;
            let payload = DefinitionPayload {
                id: "def-1".into(),
                name: "Example Name".into(),
                r#type: "example-type".into(),
                description: "Example description".into(),
                file_url: format!("{}/file.json", mock.uri()),
                digest: digest_for_task,
                source_url: Some(meta_url.clone()),
            };

            tokio::runtime::Handle::current().block_on(async {
                create_definition(&mut conn, &http_client, &s3_client, &payload).await
            })
        }
    })
    .await??;

    let inserted = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Definition> {
            let mut conn = pool.get()?;
            definitions
                .find("def-1")
                .first(&mut conn)
                .map_err(Into::into)
        }
    })
    .await??;

    assert_eq!(inserted.digest, digest_str);
    assert_eq!(inserted.name, "Example Name");

    pg_container.stop().await.ok();
    s3_container.stop().await.ok();

    Ok(())
}

#[tokio::test]
async fn create_definition_conflict_errors() -> Result<()> {
    let (pg_container, pool) = common::initialize_pg().await?;
    let (s3_container, s3_client) = common::initialize_s3().await?;

    s3_client.create_bucket().bucket("definitions").send().await?;

    let mock = MockServer::start().await;
    let mock_uri = mock.uri();

    let file_body = b"hello-conflict";
    let digest_str = format!("sha256:{:x}", Sha256::digest(file_body));

    Mock::given(method("GET"))
        .and(path("/file.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(file_body, "application/json"))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/meta.json"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&DefinitionPayload {
            id: "def-2".into(),
            name: "Name".into(),
            r#type: "t".into(),
            description: "d".into(),
            file_url: format!("{}/file.json", mock_uri.clone()),
            digest: digest_str.clone(),
            source_url: Some(format!("{}/meta.json", mock_uri.clone())),
        }))
        .mount(&mock)
        .await;

    let http_client = reqwest::Client::new();

    let file_url = format!("{}/file.json", mock_uri.clone());
    let meta_url = format!("{}/meta.json", mock_uri.clone());

    let digest_for_task = digest_str.clone();

    tokio::task::spawn_blocking({
        let pool = pool.clone();
        let s3_client = s3_client.clone();
        let http_client = http_client.clone();

        move || -> Result<()> {
            let mut conn = pool.get()?;
            let payload = DefinitionPayload {
                id: "def-2".into(),
                name: "Name".into(),
                r#type: "t".into(),
                description: "d".into(),
                file_url: file_url.clone(),
                digest: digest_for_task.clone(),
                source_url: None,
            };
            tokio::runtime::Handle::current()
                .block_on(async { create_definition(&mut conn, &http_client, &s3_client, &payload).await })
                .map(|_| ())
        }
    })
    .await??;

    let digest_for_task = digest_str.clone();

    let conflict_result = tokio::task::spawn_blocking({
        let pool = pool.clone();
        let s3_client = s3_client.clone();
        let http_client = http_client.clone();

        let file_url = format!("{}/file.json", mock_uri.clone());
        let meta_url = meta_url.clone();

        move || -> Result<()> {
            let mut conn = pool.get()?;
            let payload = DefinitionPayload {
                id: "def-2".into(),
                name: "Name".into(),
                r#type: "t".into(),
                description: "d".into(),
                file_url,
                digest: digest_for_task,
                source_url: Some(meta_url),
            };
            tokio::runtime::Handle::current()
                .block_on(async { create_definition(&mut conn, &http_client, &s3_client, &payload).await })?;
            Ok(())
        }
    })
    .await?;

    assert!(conflict_result.is_err());

    let err = conflict_result.unwrap_err();

    assert!(err.to_string().contains("Conflict: Definition with ID 'def-2'"));

    pg_container.stop().await.ok();
    s3_container.stop().await.ok();

    Ok(())
}

#[tokio::test]
async fn create_definition_from_registry_sets_source_url() -> Result<()> {
    let (pg_container, pool) = common::initialize_pg().await?;
    let (s3_container, s3_client) = common::initialize_s3().await?;

    s3_client
        .create_bucket()
        .bucket("definitions")
        .send()
        .await?;

    let mock = MockServer::start().await;
    let file_body = b"registry-body";
    let digest_str = format!("sha256:{:x}", Sha256::digest(file_body));
    let registry_url = format!("{}/registry.json", mock.uri());

    Mock::given(method("GET"))
        .and(path("/file.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(file_body, "application/json"))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/registry.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(&DefinitionPayload {
                id: "def-3".into(),
                name: "RegName".into(),
                r#type: "reg-type".into(),
                description: "reg-desc".into(),
                file_url: format!("{}/file.json", mock.uri()),
                digest: digest_str.clone(),
                source_url: None,
            }),
        )
        .mount(&mock)
        .await;

    let http_client = reqwest::Client::new();

    tokio::task::spawn_blocking({
        let pool = pool.clone();
        let http_client = http_client.clone();
        let s3_client = s3_client.clone();
        let registry_url = registry_url.clone();

        move || -> Result<Definition> {
            let mut conn = pool.get()?;
            tokio::runtime::Handle::current().block_on(async {
                create_definition_from_registry(&mut conn, &http_client, &s3_client, &registry_url)
                    .await
            })
        }
    })
    .await??;

    let row = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Definition> {
            let mut conn = pool.get()?;
            definitions
                .find("def-3")
                .first(&mut conn)
                .map_err(Into::into)
        }
    })
    .await??;

    assert_eq!(row.source_url.as_deref(), Some(registry_url.as_str()));

    pg_container.stop().await.ok();
    s3_container.stop().await.ok();

    Ok(())
}

#[tokio::test]
async fn update_definition_from_source_updates_when_digest_changes() -> Result<()> {
    let (pg_container, pool) = common::initialize_pg().await?;
    let (s3_container, s3_client) = common::initialize_s3().await?;

    s3_client
        .create_bucket()
        .bucket("definitions")
        .send()
        .await?;

    let mock = MockServer::start().await;

    let old_body = b"old-body";
    let old_digest = format!("sha256:{:x}", Sha256::digest(old_body));

    let new_body = b"new-body";
    let new_digest = format!("sha256:{:x}", Sha256::digest(new_body));

    Mock::given(method("GET"))
        .and(path("/file-new.json"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(new_body, "application/json"))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/meta.json"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(&DefinitionPayload {
                id: "def-4".into(),
                name: "New Name".into(),
                r#type: "new-type".into(),
                description: "New Desc".into(),
                file_url: format!("{}/file-new.json", mock.uri()),
                digest: new_digest.clone(),
                source_url: Some(format!("{}/meta.json", mock.uri())),
            }),
        )
        .mount(&mock)
        .await;

    tokio::task::spawn_blocking({
        let pool = pool.clone();
        let old_digest = old_digest.clone();

        move || -> Result<()> {
            let mut conn = pool.get()?;
            diesel::insert_into(definitions)
                .values(&NewDefinition {
                    id: "def-4".into(),
                    type_: "old-type".into(),
                    name: "Old Name".into(),
                    description: "Old Desc".into(),
                    definition_object_key: "def-4".into(),
                    configuration_object_key: "def-4".into(),
                    secrets_object_key: "def-4".into(),
                    digest: old_digest,
                    source_url: Some(format!("{}/meta.json", mock.uri())),
                })
                .execute(&mut conn)?;
            Ok(())
        }
    })
    .await??;

    let http_client = reqwest::Client::new();

    tokio::task::spawn_blocking({
        let pool = pool.clone();
        let http_client = http_client.clone();
        let s3_client = s3_client.clone();

        move || -> Result<Definition> {
            let mut conn = pool.get()?;
            tokio::runtime::Handle::current().block_on(async {
                update_definition_from_source(&mut conn, &http_client, &s3_client, "def-4").await
            })
        }
    })
    .await??;

    let updated = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Definition> {
            let mut conn = pool.get()?;
            definitions
                .find("def-4")
                .first(&mut conn)
                .map_err(Into::into)
        }
    })
    .await??;

    assert_eq!(updated.digest, new_digest);
    assert_eq!(updated.name, "New Name");
    assert_eq!(updated.description, "New Desc");
    assert_eq!(updated.type_, "new-type");

    pg_container.stop().await.ok();
    s3_container.stop().await.ok();

    Ok(())
}

#[tokio::test]
async fn list_definitions_filters_and_sorting() -> Result<()> {
    let (pg_container, pool) = common::initialize_pg().await?;

    tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<()> {
            let mut conn = pool.get()?;
            diesel::insert_into(definitions)
                .values(&vec![
                    NewDefinition {
                        id: "a1".into(),
                        type_: "t1".into(),
                        name: "Alpha".into(),
                        description: "First".into(),
                        definition_object_key: "k1".into(),
                        configuration_object_key: "k1".into(),
                        secrets_object_key: "k1".into(),
                        digest: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                        source_url: None,
                    },
                    NewDefinition {
                        id: "b2".into(),
                        type_: "t2".into(),
                        name: "Beta".into(),
                        description: "Second".into(),
                        definition_object_key: "k2".into(),
                        configuration_object_key: "k2".into(),
                        secrets_object_key: "k2".into(),
                        digest: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                        source_url: None,
                    },
                    NewDefinition {
                        id: "c3".into(),
                        type_: "t1".into(),
                        name: "Gamma".into(),
                        description: "Third".into(),
                        definition_object_key: "k3".into(),
                        configuration_object_key: "k3".into(),
                        secrets_object_key: "k3".into(),
                        digest: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
                        source_url: None,
                    },
                ])
                .execute(&mut conn)?;

            diesel::update(definitions)
                .set(is_enabled.eq(true))
                .execute(&mut conn)?;
            diesel::update(definitions.find("b2"))
                .set(is_enabled.eq(false))
                .execute(&mut conn)?;

            Ok(())
        }
    })
    .await??;

    let by_query = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Vec<Definition>> {
            let mut conn = pool.get()?;
            list_definitions(
                &mut conn,
                &DefinitionFilter {
                    query: Some("amm".into()),
                    ..Default::default()
                },
            )
            .map_err(Into::into)
        }
    })
    .await??;

    assert_eq!(by_query.len(), 1);
    assert_eq!(by_query[0].id, "c3");

    let disabled = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Vec<Definition>> {
            let mut conn = pool.get()?;
            list_definitions(
                &mut conn,
                &DefinitionFilter {
                    is_enabled: Some(false),
                    ..Default::default()
                },
            )
            .map_err(Into::into)
        }
    })
    .await??;

    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].id, "b2");

    let sorted = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Vec<Definition>> {
            let mut conn = pool.get()?;
            list_definitions(
                &mut conn,
                &DefinitionFilter {
                    r#type: Some("t1".into()),
                    sort_by: Some(SortBy::Name),
                    sort_order: Some(SortOrder::Desc),
                    ..Default::default()
                },
            )
            .map_err(Into::into)
        }
    })
    .await??;

    let ids: Vec<_> = sorted.iter().map(|d| d.id.as_str()).collect();
    assert_eq!(ids, vec!["c3", "a1"]);

    pg_container.stop().await.ok();

    Ok(())
}
