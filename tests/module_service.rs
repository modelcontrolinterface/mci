use anyhow::Result;
use diesel::prelude::*;
use mci::{
    models::{Module, ModuleType, NewModule},
    schema::modules::dsl::*,
    services::modules_services::{list_modules, ModuleFilter, SortBy, SortOrder},
};

mod common;

#[tokio::test]
async fn list_modules_filters_and_sorting() -> Result<()> {
    let (pg_container, pool) = common::initialize_pg().await?;

    tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<()> {
            let mut conn = pool.get()?;
            diesel::insert_into(modules)
                .values(&vec![
                    NewModule {
                        id: "m1".into(),
                        type_: ModuleType::Language,
                        name: "Alpha".into(),
                        description: "First".into(),
                        module_object_key: "k1.wasm".into(),
                        configuration_object_key: "k1.wasm".into(),
                        secrets_object_key: "k1.wasm".into(),
                        digest: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                            .into(),
                        source_url: None,
                    },
                    NewModule {
                        id: "m2".into(),
                        type_: ModuleType::Sandbox,
                        name: "Beta".into(),
                        description: "Second".into(),
                        module_object_key: "k2.wasm".into(),
                        configuration_object_key: "k2.wasm".into(),
                        secrets_object_key: "k2.wasm".into(),
                        digest: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                            .into(),
                        source_url: None,
                    },
                    NewModule {
                        id: "m3".into(),
                        type_: ModuleType::Language,
                        name: "Gamma".into(),
                        description: "Third".into(),
                        module_object_key: "k3.wasm".into(),
                        configuration_object_key: "k3.wasm".into(),
                        secrets_object_key: "k3.wasm".into(),
                        digest: "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                            .into(),
                        source_url: None,
                    },
                ])
                .execute(&mut conn)?;

            diesel::update(modules)
                .set(is_enabled.eq(true))
                .execute(&mut conn)?;
            diesel::update(modules.find("m2"))
                .set(is_enabled.eq(false))
                .execute(&mut conn)?;

            Ok(())
        }
    })
    .await??;

    let by_query = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Vec<Module>> {
            let mut conn = pool.get()?;
            list_modules(
                &mut conn,
                &ModuleFilter {
                    query: Some("amm".into()),
                    ..Default::default()
                },
            )
            .map_err(Into::into)
        }
    })
    .await??;

    assert_eq!(by_query.len(), 1);
    assert_eq!(by_query[0].id, "m3");

    let disabled = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Vec<Module>> {
            let mut conn = pool.get()?;
            list_modules(
                &mut conn,
                &ModuleFilter {
                    is_enabled: Some(false),
                    ..Default::default()
                },
            )
            .map_err(Into::into)
        }
    })
    .await??;

    assert_eq!(disabled.len(), 1);
    assert_eq!(disabled[0].id, "m2");

    let sorted = tokio::task::spawn_blocking({
        let pool = pool.clone();
        move || -> Result<Vec<Module>> {
            let mut conn = pool.get()?;
            list_modules(
                &mut conn,
                &ModuleFilter {
                    r#type: Some(ModuleType::Language),
                    sort_by: Some(SortBy::Name),
                    sort_order: Some(SortOrder::Desc),
                    ..Default::default()
                },
            )
            .map_err(Into::into)
        }
    })
    .await??;

    let ids: Vec<_> = sorted.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, vec!["m3", "m1"]);

    pg_container.stop().await.ok();

    Ok(())
}
