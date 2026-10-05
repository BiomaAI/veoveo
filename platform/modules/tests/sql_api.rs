//! API provenance is qualified independently of any product owner's vocabulary.
#![cfg(feature = "runner")]
use veoveo_modules::runner::prepare;
use veoveo_modules::*;
const SAFE: &str = "DEFINE FUNCTION fn::kernel::own::read_v1($id: record<own>, $key: string) -> option<object> { LET $rows = SELECT * FROM own WHERE id=$id LIMIT 1; LET $row = array::first($rows); RETURN IF type::is_object($row) THEN { LET $payload=$row.payload; RETURN IF type::is_object($payload) THEN { RETURN IF $payload.key=$key THEN {key:$payload.key} ELSE NONE END; } ELSE NONE END; } ELSE NONE END; } PERMISSIONS FULL;";
fn host() -> LaneExecution {
    LaneExecution::new(
        ExecutionImage::new("fixture").unwrap(),
        ExecutionCommand::new(vec!["fixture".into()]).unwrap(),
    )
    .unwrap()
}
fn kernel(sql: &'static str) -> ModuleSetup {
    ModuleSetup::builder(ModuleName::new("own").unwrap(), ModuleLayer::Kernel)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new("own").unwrap()),
            OwnershipClaim::Function(FunctionName::new("fn::kernel::own::read_v1").unwrap()),
        ])
        .execution(host())
        .lane(
            MigrationLane::new(vec![
                Migration::new(
                    MigrationVersion::new(0),
                    MigrationName::new("api").unwrap(),
                    sql,
                )
                .unwrap(),
            ])
            .unwrap(),
        )
        .sql_apis(vec![
            KernelSqlApi::new(
                FunctionName::new("fn::kernel::own::read_v1").unwrap(),
                MigrationVersion::new(0),
                SqlSignature::new(
                    vec![
                        SqlParameter::new("id", SqlType::Record(TableName::new("own").unwrap()))
                            .unwrap(),
                        SqlParameter::new("key", SqlType::String).unwrap(),
                    ],
                    SqlType::Option(Box::new(SqlType::Object)),
                )
                .unwrap(),
                SqlReadProfile::new(vec![TableName::new("own").unwrap()]).unwrap(),
                sql,
            )
            .unwrap(),
        ])
        .build()
        .unwrap()
}
fn admitted(sql: &'static str) -> bool {
    let registry = ModuleRegistry::new(vec![kernel(sql)]).unwrap();
    let result = prepare(registry.select(vec![]).unwrap());
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    result.is_ok()
}
fn leaked(sql: String) -> &'static str {
    Box::leak(sql.into_boxed_str())
}
#[test]
fn guarded_persisted_row_and_object_fields_are_admitted() {
    assert!(admitted(SAFE));
}
#[test]
fn field_proof_needs_positive_guard_and_cannot_escape_or_be_redefined() {
    for sql in [
        SAFE.replace("IF type::is_object($payload)", "IF true"),
        SAFE.replace("LET $payload=$row.payload", "LET $payload=$auth"),
        SAFE.replace("LET $payload=$row.payload", "LET $payload=$session"),
        SAFE.replace("LET $payload=$row.payload", "LET $payload=$unknown"),
        SAFE.replace("$payload.key=$key", "$payload.child.key=$key"),
        SAFE.replace(
            "THEN { RETURN IF $payload.key",
            "THEN { LET $payload=own:foreign; RETURN IF $payload.key",
        ),
        SAFE.replace(
            "RETURN IF type::is_object($payload) THEN { RETURN IF $payload.key=$key THEN {key:$payload.key} ELSE NONE END; } ELSE NONE END;",
            "RETURN IF type::is_object($payload) THEN NONE ELSE $payload.key END;",
        ),
        SAFE.replace(
            "RETURN IF type::is_object($payload)",
            "IF type::is_object($payload) THEN true ELSE false END; RETURN IF true",
        ),
    ] {
        assert!(!admitted(leaked(sql.clone())), "must reject: {sql}");
    }
}
#[test]
fn read_only_bodies_reject_writes_private_calls_and_foreign_reads() {
    for sql in [
        SAFE.replace("LET $rows =", "DELETE own; LET $rows ="),
        SAFE.replace("SELECT * FROM own", "SELECT * FROM foreign"),
        SAFE.replace("array::first($rows)", "fn::kernel::own::read_v1(own:x,'x')"),
        SAFE.replace("$payload.key=$key", "fn::private((DELETE own))=$key"),
    ] {
        assert!(!admitted(leaked(sql.clone())), "must reject: {sql}");
    }
}
fn caller(sql: &'static str, minimum: bool, dependency: bool) -> ModuleSetup {
    let requirement = LaneRequirement::AtLeast {
        module: ModuleName::new("own").unwrap(),
        version: MigrationVersion::new(0),
    };
    ModuleSetup::builder(ModuleName::new("consumer").unwrap(), ModuleLayer::Optional)
        .ownership(vec![OwnershipClaim::Table(
            TableName::new("consumer").unwrap(),
        )])
        .execution(host())
        .requires(if dependency {
            vec![requirement.clone()]
        } else {
            vec![]
        })
        .lane(
            MigrationLane::new(vec![
                Migration::new(
                    MigrationVersion::new(0),
                    MigrationName::new("consumer").unwrap(),
                    sql,
                )
                .unwrap()
                .with_requirements(if minimum { vec![requirement] } else { vec![] })
                .unwrap(),
            ])
            .unwrap(),
        )
        .build()
        .unwrap()
}
#[test]
fn exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments() {
    let valid = "LET $id: record<own> = own:x; RETURN fn::kernel::own::read_v1($id,'key');";
    for (sql, minimum, dependency, expected) in [
        (valid, true, true, true),
        (valid, false, true, false),
        (
            "RETURN fn::kernel::own::read_v1(own:x,'key');",
            false,
            false,
            false,
        ),
        (
            "RETURN fn::kernel::own::read_v2(own:x,'key');",
            true,
            true,
            false,
        ),
        (
            "RETURN fn::kernel::own::read_v1({id:own:x},'key');",
            true,
            true,
            false,
        ),
        (
            "RETURN fn::kernel::own::read_v1(consumer:x,'key');",
            true,
            true,
            false,
        ),
        (
            "RETURN fn::kernel::own::read_v1($untyped,'key');",
            true,
            true,
            false,
        ),
        ("RETURN fn::kernel::own::read_v1(own:x);", true, true, false),
        (
            "RETURN fn::kernel::own::read_v1(own:x,(DELETE consumer));",
            true,
            true,
            false,
        ),
    ] {
        let registry =
            ModuleRegistry::new(vec![kernel(SAFE), caller(sql, minimum, dependency)]).unwrap();
        let result = prepare(
            registry
                .select(vec![ModuleName::new("consumer").unwrap()])
                .unwrap(),
        );
        assert_eq!(result.is_ok(), expected, "{sql}: {result:?}");
    }
}
#[test]
fn declared_signature_and_introduction_cannot_substitute_a_different_definition() {
    let mut owner = kernel(SAFE);
    // Missing actual export is not made valid by its declaration.
    let absent = ModuleSetup::builder(owner.name().clone(), owner.layer())
        .ownership(owner.ownership().to_vec())
        .execution(host())
        .lane(
            MigrationLane::new(vec![
                Migration::new(
                    MigrationVersion::new(0),
                    MigrationName::new("api").unwrap(),
                    "DEFINE TABLE own;",
                )
                .unwrap(),
            ])
            .unwrap(),
        )
        .sql_apis(owner.sql_apis().to_vec())
        .build()
        .unwrap();
    let registry = ModuleRegistry::new(vec![absent]).unwrap();
    assert!(prepare(registry.select(vec![]).unwrap()).is_err());
    let changed = SAFE.replace("$key: string", "$key: bool");
    owner = kernel(leaked(changed));
    let registry = ModuleRegistry::new(vec![owner]).unwrap();
    assert!(prepare(registry.select(vec![]).unwrap()).is_err());
}

#[test]
fn api_cannot_be_exported_by_an_optional_or_differently_named_owner() {
    let owner = kernel(SAFE);
    for (name, layer) in [
        ("own", ModuleLayer::Optional),
        ("different", ModuleLayer::Kernel),
    ] {
        assert!(
            ModuleSetup::builder(ModuleName::new(name).unwrap(), layer)
                .ownership(owner.ownership().to_vec())
                .execution(host())
                .lane(owner.lane().clone())
                .sql_apis(owner.sql_apis().to_vec())
                .build()
                .is_err()
        );
    }
    let different_name = SAFE.replace("kernel::own::read_v1", "kernel::own::private_v1");
    // Export metadata cannot relabel another function's otherwise matching definition.
    let changed = kernel(leaked(different_name));
    let registry = ModuleRegistry::new(vec![changed]).unwrap();
    assert!(prepare(registry.select(vec![]).unwrap()).is_err());
}
#[test]
fn ownership_is_reused_without_an_execution_host_and_rejects_overlap() {
    let owner = ModuleOwnership::new(
        ModuleName::new("consumer").unwrap(),
        ModuleLayer::Optional,
        vec![OwnershipClaim::Table(TableName::new("consumer").unwrap())],
    )
    .unwrap();
    let setup = ModuleSetup::from_ownership(owner.clone())
        .execution(host())
        .build()
        .unwrap();
    assert_eq!(setup.ownership_declaration(), &owner);
    assert!(
        ModuleOwnership::new(
            owner.name().clone(),
            owner.layer(),
            vec![
                OwnershipClaim::Table(TableName::new("consumer").unwrap()),
                OwnershipClaim::TablePrefix(TablePrefix::new("consumer_").unwrap()),
                OwnershipClaim::Table(TableName::new("consumer_child").unwrap()),
            ]
        )
        .is_err()
    );
}

const UPDATE_API: &str = "DEFINE FUNCTION fn::kernel::own::read_v1($id: record<own>, $key: string) -> option<object> { LET $rows = SELECT * FROM $id; LET $row = array::first($rows); RETURN IF type::is_object($row) THEN { UPDATE ONLY $id SET payload = {key:$key} RETURN NONE; RETURN {key:$key}; } ELSE NONE END; } PERMISSIONS FULL;";
fn updating_kernel(sql: &'static str, effects: SqlEffectProfile) -> ModuleSetup {
    let owner = kernel(sql);
    ModuleSetup::builder(owner.name().clone(), owner.layer())
        .ownership(
            owner
                .ownership()
                .iter()
                .cloned()
                .chain([OwnershipClaim::Table(TableName::new("own_other").unwrap())])
                .collect(),
        )
        .execution(host())
        .lane(owner.lane().clone())
        .sql_apis(vec![owner.sql_apis()[0].clone().with_effects(effects)])
        .build()
        .unwrap()
}
fn update_effects() -> SqlEffectProfile {
    SqlEffectProfile::OwnedUpdate(
        SqlUpdateProfile::new(
            TableName::new("own").unwrap(),
            vec![SqlFieldName::new("payload").unwrap()],
        )
        .unwrap(),
    )
}
fn update_admitted(sql: &'static str) -> bool {
    let registry = ModuleRegistry::new(vec![updating_kernel(sql, update_effects())]).unwrap();
    let result = prepare(registry.select(vec![]).unwrap());
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    result.is_ok()
}
#[test]
fn owned_update_profile_admits_only_declared_set_fields() {
    assert!(update_admitted(UPDATE_API));
    let registry = ModuleRegistry::new(vec![kernel(UPDATE_API)]).unwrap();
    assert!(prepare(registry.select(vec![]).unwrap()).is_err());
    for replacement in [
        "UPDATE ONLY $id SET other = {key:$key} RETURN NONE",
        "UPDATE ONLY $id SET payload.key = $key RETURN NONE",
        "UPDATE ONLY $id SET payload.* = $key RETURN NONE",
        "UPDATE ONLY $id CONTENT {payload:{key:$key}} RETURN NONE",
        "UPDATE ONLY $id MERGE {payload:{key:$key}} RETURN NONE",
        "UPDATE ONLY $id UNSET payload RETURN NONE",
        "UPDATE foreign SET payload = {key:$key} RETURN NONE",
        "UPDATE own_other SET payload = {key:$key} RETURN NONE",
        "UPDATE $unknown SET payload = {key:$key} RETURN NONE",
        "CREATE own CONTENT {payload:{key:$key}}",
        "DELETE $id",
        "DEFINE FIELD payload ON own TYPE object",
        "ALTER TABLE own SCHEMAFULL",
        "REMOVE TABLE own",
        "UPDATE ONLY $id SET payload = (DELETE own) RETURN NONE",
        "UPDATE ONLY $id SET payload = (SELECT * FROM foreign) RETURN NONE",
        "UPDATE ONLY $id SET payload = fn::kernel::own::read_v1($id,$key) RETURN NONE",
    ] {
        let sql = UPDATE_API.replace(
            "UPDATE ONLY $id SET payload = {key:$key} RETURN NONE",
            replacement,
        );
        // Every negative case uses the same upstream syntax parser as the runner.
        assert!(
            surrealdb_syn::parse_with_settings(
                sql.as_bytes(),
                surrealdb_syn::ParserSettings::default(),
                async |parser, stack| {
                    let ast = parser.parse_query(stack).await?;
                    parser.assert_finished()?;
                    Ok(ast)
                }
            )
            .is_ok(),
            "{sql}"
        );
        assert!(!update_admitted(leaked(sql.clone())), "{sql}");
    }
}
#[test]
fn updating_calls_require_dependencies_and_cannot_mutate_arguments_or_caller_sql() {
    for (sql, minimum, dependency, expected) in [
        (
            "RETURN fn::kernel::own::read_v1(own:x,'key');",
            true,
            true,
            true,
        ),
        (
            "RETURN fn::kernel::own::read_v1(own:x,'key');",
            false,
            true,
            false,
        ),
        (
            "RETURN fn::kernel::own::read_v1(own:x,'key');",
            false,
            false,
            false,
        ),
        ("UPDATE own:x SET payload = {};", true, true, false),
        (
            "RETURN fn::kernel::own::read_v1(own:x,<string>(UPDATE consumer SET value=1));",
            true,
            true,
            false,
        ),
        (
            "RETURN fn::kernel::own::read_v1(own:x,<string>fn::kernel::own::read_v1(own:x,'nested'));",
            true,
            true,
            false,
        ),
    ] {
        let registry = ModuleRegistry::new(vec![
            updating_kernel(UPDATE_API, update_effects()),
            caller(sql, minimum, dependency),
        ])
        .unwrap();
        let result = prepare(
            registry
                .select(vec![ModuleName::new("consumer").unwrap()])
                .unwrap(),
        );
        assert_eq!(result.is_ok(), expected, "{sql}: {result:?}");
    }
}
#[test]
fn update_declarations_validate_field_names_and_table_ownership() {
    for field in ["", "payload.key", "payload*", "$dynamic", "Upper"] {
        assert!(SqlFieldName::new(field).is_err());
    }
    assert!(SqlUpdateProfile::new(TableName::new("own").unwrap(), vec![]).is_err());
    assert!(
        SqlUpdateProfile::new(
            TableName::new("own").unwrap(),
            vec![SqlFieldName::new("payload").unwrap(); 2]
        )
        .is_err()
    );
    let owner = kernel(UPDATE_API);
    let foreign = SqlEffectProfile::OwnedUpdate(
        SqlUpdateProfile::new(
            TableName::new("foreign").unwrap(),
            vec![SqlFieldName::new("payload").unwrap()],
        )
        .unwrap(),
    );
    assert!(
        ModuleSetup::builder(owner.name().clone(), owner.layer())
            .ownership(owner.ownership().to_vec())
            .execution(host())
            .lane(owner.lane().clone())
            .sql_apis(vec![owner.sql_apis()[0].clone().with_effects(foreign)])
            .build()
            .is_err()
    );
}

#[test]
fn permissions_and_comments_cannot_inherit_api_body_updates() {
    for metadata in [
        "PERMISSIONS WHERE { UPDATE own SET payload = {}; RETURN true; };",
        "COMMENT (UPDATE own SET payload = {}) PERMISSIONS FULL;",
    ] {
        let sql = UPDATE_API.replace("PERMISSIONS FULL;", metadata);
        assert!(
            surrealdb_syn::parse_with_settings(
                sql.as_bytes(),
                surrealdb_syn::ParserSettings::default(),
                async |parser, stack| {
                    let ast = parser.parse_query(stack).await?;
                    parser.assert_finished()?;
                    Ok(ast)
                }
            )
            .is_ok(),
            "{sql}"
        );
        assert!(!update_admitted(leaked(sql.clone())), "{sql}");
    }
}

#[test]
fn ordinary_schema_permissions_and_comments_cannot_call_updating_exports() {
    for sql in [
        "DEFINE TABLE consumer PERMISSIONS FOR select WHERE fn::kernel::own::read_v1(own:x,'key');",
        "DEFINE FIELD payload ON consumer PERMISSIONS FOR select WHERE fn::kernel::own::read_v1(own:x,'key');",
        "DEFINE FUNCTION fn::consumer() { RETURN true; } PERMISSIONS WHERE fn::kernel::own::read_v1(own:x,'key');",
        "DEFINE TABLE consumer COMMENT fn::kernel::own::read_v1(own:x,'key');",
        "DEFINE FIELD payload ON consumer COMMENT fn::kernel::own::read_v1(own:x,'key');",
        "DEFINE FUNCTION fn::consumer() { RETURN true; } COMMENT fn::kernel::own::read_v1(own:x,'key');",
        "DEFINE TABLE consumer PERMISSIONS FOR select WHERE { UPDATE consumer SET value=1; RETURN true; };",
        "DEFINE TABLE consumer COMMENT (UPDATE consumer SET value=1);",
    ] {
        assert!(
            surrealdb_syn::parse_with_settings(
                sql.as_bytes(),
                surrealdb_syn::ParserSettings::default(),
                async |parser, stack| {
                    let ast = parser.parse_query(stack).await?;
                    parser.assert_finished()?;
                    Ok(ast)
                }
            )
            .is_ok(),
            "{sql}"
        );
        let original = caller(sql, true, true);
        let consumer = ModuleSetup::builder(original.name().clone(), original.layer())
            .ownership(vec![
                OwnershipClaim::Table(TableName::new("consumer").unwrap()),
                OwnershipClaim::Function(FunctionName::new("fn::consumer").unwrap()),
            ])
            .execution(host())
            .requires(original.requires().to_vec())
            .lane(original.lane().clone())
            .build()
            .unwrap();
        let registry = ModuleRegistry::new(vec![
            updating_kernel(UPDATE_API, update_effects()),
            consumer,
        ])
        .unwrap();
        let error = prepare(
            registry
                .select(vec![ModuleName::new("consumer").unwrap()])
                .unwrap(),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("read-only SQL context"), "{sql}: {error}");
    }
}
