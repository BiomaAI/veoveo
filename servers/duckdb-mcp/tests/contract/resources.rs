use veoveo_artifact_contract::ArtifactId;
use veoveo_duckdb_mcp::{contract::*, uris};
use veoveo_types::{ResourceAddress, ResourceTemplateUri, ScopeName, TaskId};
const ID: &str = "01983da0-0000-7000-8000-000000000001";

#[test]
fn every_resource_family_round_trips_through_its_owner_and_wire() {
    let task: TaskId = ID.parse().unwrap();
    let resources = vec![
        DuckDbResource::Docs,
        DuckDbResource::Document(DuckDbDocument::Agents),
        DuckDbResource::Document(DuckDbDocument::Design),
        DuckDbResource::Contract,
        DuckDbResource::Workbench,
        DuckDbResource::Databases(None),
        DuckDbResource::Databases(Some(DuckDbDatabaseCursor::new("metrics".parse().unwrap()))),
        DuckDbResource::Database(DuckDbDatabaseUri::new("metrics".parse().unwrap())),
        DuckDbResource::Usage(DuckDbUsageIndexUri::new(None)),
        DuckDbResource::Usage(DuckDbUsageIndexUri::new(Some(
            &DuckDbUsageCursor::new(task).unwrap(),
        ))),
        DuckDbResource::TaskUsage(DuckDbTaskUsageUri::new(task).unwrap()),
        DuckDbResource::Artifact(ArtifactId::parse(ID).unwrap()),
    ];
    for resource in resources {
        let uri = resource.to_uri().unwrap();
        assert_eq!(DuckDbResource::parse(uri.as_str()).unwrap(), resource);
        assert_eq!(
            serde_json::from_value::<DuckDbResource>(serde_json::to_value(&resource).unwrap())
                .unwrap(),
            resource
        );
    }
    assert_eq!(
        uris::doc_uri(DuckDbDocument::Agents),
        "duckdb://docs/agents"
    );
    assert_eq!(
        uris::artifact_uri(ID.parse().unwrap()).to_string(),
        format!("duckdb://artifact/{ID}")
    );
    assert!(DuckDbScope::try_from(&ScopeName::new("installation:custom").unwrap()).is_err());
    assert_eq!(
        serde_json::to_value(schemars::schema_for!(DuckDbResource)).unwrap()["type"],
        "string"
    );
}

#[test]
fn templates_expand_to_the_same_addresses_as_typed_construction() {
    let db_cursor = DuckDbDatabaseCursor::new("metrics".parse().unwrap());
    let task: TaskId = ID.parse().unwrap();
    let usage_cursor = DuckDbUsageCursor::new(task).unwrap();
    for (template, name, value, resource) in [
        (
            uris::DOC_TEMPLATE,
            "doc_id",
            "agents",
            DuckDbResource::Document(DuckDbDocument::Agents),
        ),
        (
            uris::DB_TEMPLATE,
            "db_id",
            "metrics",
            DuckDbResource::Database(DuckDbDatabaseUri::new("metrics".parse().unwrap())),
        ),
        (
            uris::DBS_TEMPLATE,
            "cursor",
            db_cursor.as_str(),
            DuckDbResource::Databases(Some(db_cursor.clone())),
        ),
        (
            uris::ARTIFACT_TEMPLATE,
            "artifact_id",
            ID,
            DuckDbResource::Artifact(ID.parse().unwrap()),
        ),
        (
            DuckDbUsageIndexUri::TEMPLATE,
            "cursor",
            usage_cursor.as_str(),
            DuckDbResource::Usage(DuckDbUsageIndexUri::new(Some(&usage_cursor))),
        ),
        (
            DuckDbTaskUsageUri::TEMPLATE,
            "task_id",
            ID,
            DuckDbResource::TaskUsage(DuckDbTaskUsageUri::new(task).unwrap()),
        ),
    ] {
        let values = [(name.to_owned(), value.to_owned())].into();
        let expanded = ResourceTemplateUri::new(template)
            .unwrap()
            .expand_scalars(&values)
            .unwrap();
        assert_eq!(expanded, resource.to_uri().unwrap(), "{template}");
    }
}

#[test]
fn resources_reject_unknown_routes_aliases_and_unadmitted_parameters() {
    for raw in [
        "duckdb://db/",
        "duckdb://db/UPPER",
        "duckdb://db/a/b",
        "duckdb://db/%6Detrics",
        "duckdb://db/metrics?token=private",
        "duckdb://db/metrics#private",
        "duckdb://docs/unknown",
        "duckdb://dbs?",
        "duckdb://dbs?cursor=",
        "duckdb://dbs/",
        "duckdb://usage?cursor=invalid",
        "ui://duckdb/other.html",
        "duckdb://artifact/not-an-id",
        "other://db/metrics",
    ] {
        let error = DuckDbResource::parse(raw).unwrap_err();
        assert!(!error.to_string().contains(raw));
    }
    let cursor = DuckDbDatabaseCursor::new("metrics".parse().unwrap());
    for suffix in ["&cursor=duplicate", "&owner=another", "#fragment"] {
        let raw = format!("duckdb://dbs?cursor={}{}", cursor.as_str(), suffix);
        assert!(DuckDbResource::parse(raw).is_err());
    }
}
