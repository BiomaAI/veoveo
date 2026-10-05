//! API provenance is qualified independently of any product owner's vocabulary.
#![cfg(feature = "runner")]
use veoveo_modules::runner::prepare;
use veoveo_modules::*;
const SAFE: &str = include_str!("queries/sql_api/declarations/statement_1.surql");
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
    let valid = include_str!(
        "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_2.surql"
    );
    for (sql, minimum, dependency, expected) in [
        (valid, true, true, true),
        (valid, false, true, false),
        (
            include_str!(
                "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_3.surql"
            ),
            false,
            false,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_4.surql"
            ),
            true,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_5.surql"
            ),
            true,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_6.surql"
            ),
            true,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_7.surql"
            ),
            true,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_8.surql"
            ),
            true,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/exact_api_calls_require_dependency_minimum_signature_and_all_safe_arguments/statement_9.surql"
            ),
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
                    include_str!("queries/sql_api/declared_signature_and_introduction_cannot_substitute_a_different_definition/statement_10.surql"),
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

const UPDATE_API: &str = include_str!(
    "queries/sql_api/ownership_is_reused_without_an_execution_host_and_rejects_overlap/statement_11.surql"
);
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
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_12.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_13.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_14.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_15.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_16.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_17.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_18.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_19.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_20.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_21.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_22.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_23.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_24.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_25.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_26.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_27.surql"
        ),
        include_str!(
            "queries/sql_api/owned_update_profile_admits_only_declared_set_fields/statement_28.surql"
        ),
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
            include_str!(
                "queries/sql_api/updating_calls_require_dependencies_and_cannot_mutate_arguments_or_caller_sql/statement_29.surql"
            ),
            true,
            true,
            true,
        ),
        (
            include_str!(
                "queries/sql_api/updating_calls_require_dependencies_and_cannot_mutate_arguments_or_caller_sql/statement_30.surql"
            ),
            false,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/updating_calls_require_dependencies_and_cannot_mutate_arguments_or_caller_sql/statement_31.surql"
            ),
            false,
            false,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/updating_calls_require_dependencies_and_cannot_mutate_arguments_or_caller_sql/statement_32.surql"
            ),
            true,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/updating_calls_require_dependencies_and_cannot_mutate_arguments_or_caller_sql/statement_33.surql"
            ),
            true,
            true,
            false,
        ),
        (
            include_str!(
                "queries/sql_api/updating_calls_require_dependencies_and_cannot_mutate_arguments_or_caller_sql/statement_34.surql"
            ),
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
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_35.surql"
        ),
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_36.surql"
        ),
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_37.surql"
        ),
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_38.surql"
        ),
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_39.surql"
        ),
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_40.surql"
        ),
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_41.surql"
        ),
        include_str!(
            "queries/sql_api/ordinary_schema_permissions_and_comments_cannot_call_updating_exports/statement_42.surql"
        ),
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
