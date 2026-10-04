#![cfg(feature = "runner")]
use veoveo_modules::runner::prepare;
use veoveo_modules::*;
fn module(
    name: &str,
    layer: ModuleLayer,
    sql: &'static str,
    requires: Vec<LaneRequirement>,
) -> ModuleSetup {
    ModuleSetup::builder(ModuleName::new(name).unwrap(), layer)
        .execution(
            LaneExecution::new(
                ExecutionImage::new("gateway").unwrap(),
                ExecutionCommand::new(vec!["module-lane".into()]).unwrap(),
            )
            .unwrap(),
        )
        .ownership(vec![
            OwnershipClaim::Table(TableName::new(name).unwrap()),
            OwnershipClaim::Function(FunctionName::new(format!("fn::{name}")).unwrap()),
            OwnershipClaim::Analyzer(AnalyzerName::new(format!("{name}_search")).unwrap()),
        ])
        .requires(requires)
        .lane(
            MigrationLane::new(vec![
                Migration::new(
                    MigrationVersion::new(0),
                    MigrationName::new("initial").unwrap(),
                    sql,
                )
                .unwrap(),
            ])
            .unwrap(),
        )
        .build()
        .unwrap()
}
fn assert_valid_syntax(sql: &str) {
    let parsed = surrealdb_syn::parse_with_settings(
        sql.as_bytes(),
        surrealdb_syn::ParserSettings::default(),
        async |parser, stack| {
            let ast = parser.parse_query(stack).await?;
            parser.assert_finished()?;
            Ok(ast)
        },
    );
    assert!(
        parsed.is_ok(),
        "policy fixture must first be valid SurrealQL: {sql}"
    );
}
fn admitted(sql: &'static str) -> bool {
    assert_valid_syntax(sql);
    let registry =
        ModuleRegistry::new(vec![module("own", ModuleLayer::Kernel, sql, vec![])]).unwrap();
    let result = prepare(registry.select(vec![]).unwrap());
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    result.is_ok()
}
#[test]
fn owned_nested_schema_and_mutations_are_admitted() {
    assert!(admitted(
        "DEFINE TABLE own SCHEMAFULL; DEFINE FIELD value ON own TYPE int ASSERT $value > 0; DEFINE INDEX by_value ON own FIELDS value UNIQUE; CREATE own:one CONTENT { value: 1 }; UPDATE own:one SET value = 2; DEFINE FUNCTION fn::own() { RETURN (SELECT * FROM own); };"
    ));
    assert!(admitted(
        "DEFINE TABLE own; ALTER TABLE own SCHEMAFULL; REMOVE TABLE own;"
    ));
    assert!(admitted(
        "DEFINE ANALYZER own_search TOKENIZERS blank FILTERS lowercase; DEFINE INDEX text ON own FIELDS value FULLTEXT ANALYZER own_search BM25;"
    ));
}
#[test]
fn recursive_effects_and_privileged_or_dynamic_syntax_fail_closed() {
    for sql in [
        "BEGIN; CREATE own; COMMIT;",
        "USE NS other DB other;",
        "DEFINE USER root ON ROOT PASSWORD 'secret' ROLES OWNER;",
        "CREATE own CONTENT { nested: (DELETE foreign) };",
        "DEFINE FIELD bad ON own VALUE (CREATE foreign);",
        "DEFINE TABLE own COMMENT (DELETE foreign);",
        "DEFINE FUNCTION fn::own() { CREATE foreign; };",
        "DEFINE EVENT evil ON own WHEN true THEN DELETE foreign;",
        "DEFINE FIELD v ON own PERMISSIONS FOR select WHERE (DELETE foreign);",
        "DEFINE ANALYZER own_search TOKENIZERS blank FILTERS MAPPER('/tmp/map.txt');",
        "RETURN http::get('https://example.com');",
        "RETURN fn::own();",
        "DEFINE ANALYZER own_search FUNCTION fn::own;",
        "CREATE platform_module_lane;",
        "SELECT * FROM platform_module_migration;",
        "LET $target = 'own'; DELETE $target;",
        "RETURN $value.link.foreign_field;",
        "SELECT own.link.foreign_field FROM own;",
        "RETURN <record<foreign>> $value;",
        "DEFINE FIELD link ON own TYPE record<own> REFERENCE ON DELETE THEN { DELETE foreign; };",
        "ALTER TABLE foreign SCHEMAFULL;",
        "REMOVE TABLE foreign;",
    ] {
        assert!(!admitted(sql), "unexpected admission of {sql}");
    }
}
#[test]
fn read_layers_and_analyzer_dependencies_are_distinct() {
    let required = vec![LaneRequirement::Satisfied(ModuleName::new("base").unwrap())];
    for (layer, sql, expected) in [
        (ModuleLayer::Kernel, "SELECT * FROM base;", true),
        (ModuleLayer::Kernel, "DELETE base;", false),
        (ModuleLayer::Optional, "SELECT * FROM base;", false),
        (
            ModuleLayer::Optional,
            "DEFINE FIELD link ON own TYPE record<base>;",
            true,
        ),
        (
            ModuleLayer::Optional,
            "DEFINE INDEX search ON own FIELDS text FULLTEXT ANALYZER base_search BM25;",
            true,
        ),
    ] {
        let registry = ModuleRegistry::new(vec![
            module("base", ModuleLayer::Kernel, "DEFINE TABLE base;", vec![]),
            module("own", layer, sql, required.clone()),
        ])
        .unwrap();
        assert_eq!(
            prepare(
                registry
                    .select(vec![ModuleName::new("own").unwrap()])
                    .unwrap()
            )
            .is_ok(),
            expected
        );
    }
}

#[test]
fn optional_reads_require_declared_optional_owner() {
    for (declared, expected) in [(true, true), (false, false)] {
        let deps = if declared {
            vec![LaneRequirement::Satisfied(ModuleName::new("base").unwrap())]
        } else {
            vec![]
        };
        let registry = ModuleRegistry::new(vec![
            module("base", ModuleLayer::Optional, "DEFINE TABLE base;", vec![]),
            module("own", ModuleLayer::Optional, "SELECT * FROM base;", deps),
        ])
        .unwrap();
        assert_eq!(
            prepare(
                registry
                    .select(vec![ModuleName::new("own").unwrap()])
                    .unwrap()
            )
            .is_ok(),
            expected
        );
    }
}
#[test]
fn native_reference_cleanup_links_to_declared_kernel_without_data_read() {
    let registry = ModuleRegistry::new(vec![module("base", ModuleLayer::Kernel, "DEFINE TABLE base;", vec![]), module("own", ModuleLayer::Optional, "DEFINE TABLE own; DEFINE FIELD link ON own TYPE record<base> REFERENCE ON DELETE CASCADE;", vec![LaneRequirement::Satisfied(ModuleName::new("base").unwrap())])]).unwrap();
    assert!(
        prepare(
            registry
                .select(vec![ModuleName::new("own").unwrap()])
                .unwrap()
        )
        .is_ok()
    );
}
#[test]
fn diagnostics_identify_owner_and_statement_without_dumping_body_values() {
    let registry = ModuleRegistry::new(vec![module(
        "own",
        ModuleLayer::Kernel,
        "CREATE foreign CONTENT { token: 'distinctive-secret-value' };",
        vec![],
    )])
    .unwrap();
    let error = prepare(registry.select(vec![]).unwrap())
        .unwrap_err()
        .to_string();
    assert!(error.contains("own"));
    assert!(error.contains("0000_initial.surql"));
    assert!(error.contains("foreign"));
    assert!(error.contains("CREATE"));
    assert!(!error.contains("distinctive-secret-value"));
}

#[test]
fn malformed_selected_sql_reports_syntax_admission_failure() {
    let registry = ModuleRegistry::new(vec![module(
        "own",
        ModuleLayer::Kernel,
        "DEFINE TABLE own; CREATE own CONTENT { broken: ;",
        vec![],
    )])
    .unwrap();
    let error = prepare(registry.select(vec![]).unwrap())
        .unwrap_err()
        .to_string();
    assert!(error.contains("invalid migration syntax"));
    assert!(error.contains("own / 0000_initial.surql"));
}
#[test]
fn valid_nested_policy_failures_identify_the_offending_owner_object() {
    for sql in [
        "CREATE own CONTENT { nested: [{ bad: (DELETE foreign) }] };",
        "DEFINE FIELD v ON own DEFAULT (CREATE foreign);",
        "DEFINE FIELD v ON own PERMISSIONS FOR select WHERE (DELETE foreign);",
        "DEFINE EVENT evil ON own WHEN true THEN DELETE foreign;",
        "DEFINE FUNCTION fn::own() { CREATE foreign; };",
        "DEFINE FIELD link ON own TYPE record<own> REFERENCE ON DELETE THEN { DELETE foreign; };",
        "RETURN <record<foreign>> $value;",
    ] {
        assert_valid_syntax(sql);
        let registry =
            ModuleRegistry::new(vec![module("own", ModuleLayer::Kernel, sql, vec![])]).unwrap();
        let error = prepare(registry.select(vec![]).unwrap())
            .unwrap_err()
            .to_string();
        assert!(!error.contains("invalid migration syntax"));
        assert!(error.contains("foreign"), "{error}");
    }
}

#[test]
fn object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies() {
    for sql in [
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE FIELD identity.key ON own TYPE string; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE option<object>; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE FIELD identity.child ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.child.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.key; REMOVE INDEX by_key ON own; DEFINE FIELD OVERWRITE identity ON own TYPE record<own>;",
    ] {
        assert!(admitted(sql), "{sql}");
    }
    for sql in [
        "DEFINE TABLE own; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE record<own>; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE option<record<own>>; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE any; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.child.key;",
        "DEFINE TABLE own; DEFINE FIELD IF NOT EXISTS identity ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE FIELD OVERWRITE identity ON own TYPE record<own>; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; REMOVE FIELD identity ON own; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; REMOVE TABLE own; DEFINE TABLE own; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; IF false THEN { DEFINE FIELD identity ON own TYPE object; } END; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.key; IF false THEN { REMOVE INDEX by_key ON own; } END; DEFINE FIELD OVERWRITE identity ON own TYPE record<own>;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.key; IF false THEN { REMOVE TABLE own; } END; DEFINE FIELD OVERWRITE identity ON own TYPE record<own>;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.key; DEFINE FIELD OVERWRITE identity ON own TYPE any;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE FIELD other ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.key; DEFINE INDEX IF NOT EXISTS by_key ON own FIELDS other.key; DEFINE FIELD OVERWRITE identity ON own TYPE record<own>;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE FIELD identity.child ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.child.key; DEFINE FIELD OVERWRITE identity ON own TYPE object;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE INDEX by_key ON own FIELDS identity.key; DEFINE FIELD identity.* ON own TYPE record<own>;",
        "DEFINE TABLE own; DEFINE FIELD identity ON own TYPE object; DEFINE FIELD identity.* ON own TYPE record<own>; DEFINE INDEX by_key ON own FIELDS identity.key;",
        "DEFINE TABLE own; DEFINE INDEX by_key ON own FIELDS string::lowercase((SELECT * FROM foreign));",
        "DEFINE TABLE own; DEFINE INDEX by_key ON own FIELDS string::lowercase(<string>(DELETE own));",
        "DEFINE TABLE own; DEFINE FUNCTION fn::own() { DEFINE FIELD identity ON own TYPE object; }; DEFINE INDEX by_key ON own FIELDS identity.key;",
    ] {
        assert!(!admitted(sql), "{sql}");
    }
}
