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
    assert!(admitted(include_str!(
        "queries/admission/owned_nested_schema_and_mutations_are_admitted/statement_1.surql"
    )));
    assert!(admitted(include_str!(
        "queries/admission/owned_nested_schema_and_mutations_are_admitted/statement_2.surql"
    )));
    assert!(admitted(include_str!(
        "queries/admission/owned_nested_schema_and_mutations_are_admitted/statement_3.surql"
    )));
}
#[test]
fn recursive_effects_and_privileged_or_dynamic_syntax_fail_closed() {
    for sql in [
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/explicit_transaction.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/session_selection.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_5.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_6.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_7.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_8.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_9.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_10.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_11.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_12.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_13.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_14.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_15.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_16.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_17.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_18.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_19.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_20.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_21.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_22.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_23.surql"
        ),
        include_str!(
            "queries/admission/recursive_effects_and_privileged_or_dynamic_syntax_fail_closed/statement_24.surql"
        ),
    ] {
        assert!(!admitted(sql), "unexpected admission of {sql}");
    }
}
#[test]
fn read_layers_and_analyzer_dependencies_are_distinct() {
    let required = vec![LaneRequirement::Satisfied(ModuleName::new("base").unwrap())];
    for (layer, sql, expected) in [
        (
            ModuleLayer::Kernel,
            include_str!(
                "queries/admission/read_layers_and_analyzer_dependencies_are_distinct/statement_25.surql"
            ),
            true,
        ),
        (
            ModuleLayer::Kernel,
            include_str!(
                "queries/admission/read_layers_and_analyzer_dependencies_are_distinct/statement_26.surql"
            ),
            false,
        ),
        (
            ModuleLayer::Optional,
            include_str!(
                "queries/admission/read_layers_and_analyzer_dependencies_are_distinct/statement_27.surql"
            ),
            false,
        ),
        (
            ModuleLayer::Optional,
            include_str!(
                "queries/admission/read_layers_and_analyzer_dependencies_are_distinct/statement_28.surql"
            ),
            true,
        ),
        (
            ModuleLayer::Optional,
            include_str!(
                "queries/admission/read_layers_and_analyzer_dependencies_are_distinct/statement_29.surql"
            ),
            true,
        ),
    ] {
        let registry = ModuleRegistry::new(vec![
            module("base", ModuleLayer::Kernel, include_str!("queries/admission/read_layers_and_analyzer_dependencies_are_distinct/statement_30.surql"), vec![]),
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
            module("base", ModuleLayer::Optional, include_str!("queries/admission/optional_reads_require_declared_optional_owner/statement_31.surql"), vec![]),
            module("own", ModuleLayer::Optional, include_str!("queries/admission/optional_reads_require_declared_optional_owner/statement_32.surql"), deps),
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
    let registry = ModuleRegistry::new(vec![module("base", ModuleLayer::Kernel, include_str!("queries/admission/native_reference_cleanup_links_to_declared_kernel_without_data_read/statement_33.surql"), vec![]), module("own", ModuleLayer::Optional, include_str!("queries/admission/native_reference_cleanup_links_to_declared_kernel_without_data_read/statement_34.surql"), vec![LaneRequirement::Satisfied(ModuleName::new("base").unwrap())])]).unwrap();
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
        include_str!("queries/admission/diagnostics_identify_owner_and_statement_without_dumping_body_values/statement_35.surql"),
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
        include_str!("queries/admission/malformed_selected_sql_reports_syntax_admission_failure/statement_37.surql"),
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
        include_str!(
            "queries/admission/valid_nested_policy_failures_identify_the_offending_owner_object/statement_38.surql"
        ),
        include_str!(
            "queries/admission/valid_nested_policy_failures_identify_the_offending_owner_object/statement_39.surql"
        ),
        include_str!(
            "queries/admission/valid_nested_policy_failures_identify_the_offending_owner_object/statement_40.surql"
        ),
        include_str!(
            "queries/admission/valid_nested_policy_failures_identify_the_offending_owner_object/statement_41.surql"
        ),
        include_str!(
            "queries/admission/valid_nested_policy_failures_identify_the_offending_owner_object/statement_42.surql"
        ),
        include_str!(
            "queries/admission/valid_nested_policy_failures_identify_the_offending_owner_object/statement_43.surql"
        ),
        include_str!(
            "queries/admission/valid_nested_policy_failures_identify_the_offending_owner_object/statement_44.surql"
        ),
    ] {
        assert_valid_syntax(sql);
        let registry =
            ModuleRegistry::new(vec![module("own", ModuleLayer::Kernel, sql, vec![])]).unwrap();
        let error = prepare(registry.select(vec![]).unwrap())
            .unwrap_err()
            .to_string();
        assert!(!error.contains("invalid migration syntax"));
        if sql.contains("PERMISSIONS") || sql.contains("DEFAULT (CREATE") {
            assert!(error.contains("read-only SQL context"), "{error}");
        } else {
            assert!(error.contains("foreign"), "{error}");
        }
    }
}

#[test]
fn object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies() {
    for sql in [
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_45.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_46.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_47.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_48.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/wildcard_preserves_media_siblings.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/wildcard_preserves_ancestor_index.surql"
        ),
    ] {
        assert!(admitted(sql), "{sql}");
    }
    for sql in [
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_49.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_50.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_51.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_52.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_53.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_54.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_55.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_56.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_57.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_58.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_59.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_60.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_61.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_62.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_63.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_64.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_65.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_66.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_67.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/statement_68.surql"
        ),
        include_str!(
            "queries/admission/object_path_indexes_require_closed_preceding_schema_and_preserve_dependencies/wildcard_invalidates_dependent_index.surql"
        ),
    ] {
        assert!(!admitted(sql), "{sql}");
    }
}

#[test]
fn reciprocal_kernel_field_schema_links_do_not_order_execution() {
    let left = module(
        "left",
        ModuleLayer::Kernel,
        include_str!(
            "queries/admission/reciprocal_kernel_field_schema_links_do_not_order_execution/statement_69.surql"
        ),
        vec![],
    );
    let right = module(
        "right",
        ModuleLayer::Kernel,
        include_str!(
            "queries/admission/reciprocal_kernel_field_schema_links_do_not_order_execution/statement_70.surql"
        ),
        vec![],
    );
    for modules in [vec![left.clone(), right.clone()], vec![right, left]] {
        let registry = ModuleRegistry::new(modules).unwrap();
        prepare(registry.select(vec![]).unwrap()).unwrap();
    }
}

#[test]
fn field_schema_links_do_not_admit_foreign_executable_or_table_types() {
    for sql in [
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_71.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_72.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_73.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_74.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_75.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_76.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_77.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_78.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_79.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_80.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_81.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_82.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_83.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_84.surql"
        ),
        include_str!(
            "queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_85.surql"
        ),
    ] {
        assert_valid_syntax(sql);
        let registry = ModuleRegistry::new(vec![
            module("base", ModuleLayer::Kernel, include_str!("queries/admission/field_schema_links_do_not_admit_foreign_executable_or_table_types/statement_86.surql"), vec![]),
            module("own", ModuleLayer::Kernel, sql, vec![]),
        ])
        .unwrap();
        let error = prepare(registry.select(vec![]).unwrap())
            .unwrap_err()
            .to_string();
        if sql.contains("COMMENT") || sql.contains("DEFAULT (CREATE") {
            assert!(error.contains("read-only SQL context"), "{sql}: {error}");
        } else {
            assert!(error.contains("base"), "{sql}: {error}");
        }
        assert!(
            !error.contains("invalid migration syntax"),
            "{sql}: {error}"
        );
    }
}

#[test]
fn nested_field_schema_links_validate_every_owner_and_exclude_history() {
    for sql in [
        include_str!(
            "queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_87.surql"
        ),
        include_str!(
            "queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_88.surql"
        ),
        include_str!(
            "queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_89.surql"
        ),
        include_str!(
            "queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_90.surql"
        ),
        include_str!(
            "queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_91.surql"
        ),
    ] {
        assert_valid_syntax(sql);
        let registry = ModuleRegistry::new(vec![
            module("base", ModuleLayer::Kernel, include_str!("queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_92.surql"), vec![]),
            module("own", ModuleLayer::Kernel, sql, vec![]),
            module(
                "optional",
                ModuleLayer::Optional,
                include_str!("queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_93.surql"),
                vec![],
            ),
        ])
        .unwrap();
        assert!(
            prepare(
                registry
                    .select(vec![ModuleName::new("optional").unwrap()])
                    .unwrap()
            )
            .is_err(),
            "{sql}"
        );
    }
    let registry = ModuleRegistry::new(vec![
        module("base", ModuleLayer::Kernel, include_str!("queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_94.surql"), vec![]),
        module(
            "own",
            ModuleLayer::Optional,
            include_str!("queries/admission/nested_field_schema_links_validate_every_owner_and_exclude_history/statement_95.surql"),
            vec![],
        ),
    ])
    .unwrap();
    assert!(
        prepare(
            registry
                .select(vec![ModuleName::new("own").unwrap()])
                .unwrap()
        )
        .is_err()
    );
}

#[test]
fn current_field_kinds_defaults_and_relations_inspect_nested_targets() {
    for sql in [
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_96.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_97.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_98.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_99.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_100.surql"
        ),
    ] {
        assert!(admitted(sql), "current schema profile rejected {sql}");
    }
    for sql in [
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_101.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_102.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_103.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_104.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_105.surql"
        ),
        include_str!(
            "queries/admission/current_field_kinds_defaults_and_relations_inspect_nested_targets/statement_106.surql"
        ),
    ] {
        assert!(
            !admitted(sql),
            "nested or executable reference admitted {sql}"
        );
    }
}

#[test]
fn pure_byte_length_and_regex_inspect_every_argument() {
    assert!(admitted(include_str!(
        "queries/admission/pure_byte_length_and_regex_inspect_every_argument/admitted.surql"
    )));
    for sql in [
        include_str!(
            "queries/admission/pure_byte_length_and_regex_inspect_every_argument/bytes_foreign.surql"
        ),
        include_str!(
            "queries/admission/pure_byte_length_and_regex_inspect_every_argument/bytes_write.surql"
        ),
        include_str!(
            "queries/admission/pure_byte_length_and_regex_inspect_every_argument/regex_foreign.surql"
        ),
        include_str!(
            "queries/admission/pure_byte_length_and_regex_inspect_every_argument/regex_write.surql"
        ),
    ] {
        assert!(!admitted(sql));
    }
}

#[test]
fn closed_object_value_fields_preserve_owner_and_target_checks() {
    assert!(admitted(include_str!(
        "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/agent_envelopes.surql"
    )));
    for sql in [
        include_str!(
            "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/scalar_union.surql"
        ),
        include_str!(
            "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/record_union.surql"
        ),
        include_str!(
            "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/deeper_dereference.surql"
        ),
        include_str!(
            "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/foreign_target.surql"
        ),
        include_str!(
            "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/unproven_target.surql"
        ),
        include_str!(
            "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/graph_traversal.surql"
        ),
        include_str!(
            "queries/admission/closed_object_value_fields_preserve_owner_and_target_checks/foreign_kind.surql"
        ),
    ] {
        assert!(!admitted(sql));
    }
}
