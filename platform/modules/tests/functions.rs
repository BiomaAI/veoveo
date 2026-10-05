#![cfg(feature = "runner")]
//! Private function versions and deferred effects use the upstream parser, not SQL text matching.
use veoveo_modules::runner::prepare;
use veoveo_modules::*;
fn module(
    name: &str,
    layer: ModuleLayer,
    bodies: Vec<&'static str>,
    requirements: Vec<LaneRequirement>,
) -> ModuleSetup {
    ModuleSetup::builder(ModuleName::new(name).unwrap(), layer)
        .ownership(vec![
            OwnershipClaim::Table(TableName::new(name).unwrap()),
            OwnershipClaim::TablePrefix(TablePrefix::new(format!("{name}_")).unwrap()),
            OwnershipClaim::FunctionPrefix(FunctionPrefix::new(format!("fn::{name}::")).unwrap()),
        ])
        .execution(
            LaneExecution::new(
                ExecutionImage::new("fixture").unwrap(),
                ExecutionCommand::new(vec!["fixture".into()]).unwrap(),
            )
            .unwrap(),
        )
        .requires(requirements.clone())
        .lane(
            MigrationLane::new(
                bodies
                    .into_iter()
                    .enumerate()
                    .map(|(version, sql)| {
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
                            "fixture must be native grammar: {sql}"
                        );
                        Migration::new(
                            MigrationVersion::new(version as u32),
                            MigrationName::new(format!("body_{version}")).unwrap(),
                            sql,
                        )
                        .unwrap()
                        .with_requirements(requirements.clone())
                        .unwrap()
                    })
                    .collect(),
            )
            .unwrap(),
        )
        .build()
        .unwrap()
}
fn admitted(sql: &'static str) -> bool {
    let registry =
        ModuleRegistry::new(vec![module("own", ModuleLayer::Kernel, vec![sql], vec![])]).unwrap();
    let result = prepare(registry.select(vec![]).unwrap());
    if let Err(error) = &result {
        eprintln!("{error}");
    }
    result.is_ok()
}
#[test]
fn local_calls_inspect_complete_callee_bodies_and_native_parameters() {
    for sql in [
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_1.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_2.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_3.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_4.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_5.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_6.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_7.surql"
        ),
    ] {
        assert!(admitted(sql), "{sql}");
    }
    for sql in [
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_8.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_9.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_10.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_11.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_12.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_13.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_14.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_15.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_16.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_17.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_18.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_19.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_20.surql"
        ),
        include_str!(
            "queries/functions/local_calls_inspect_complete_callee_bodies_and_native_parameters/statement_21.surql"
        ),
    ] {
        assert!(!admitted(sql), "{sql}");
    }
}
#[test]
fn optional_calls_require_readonly_callees_and_introducing_minimum() {
    for (base_layer, consumer_layer, body, dependency, minimum, expected) in [
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            include_str!(
                "queries/functions/optional_calls_require_readonly_callees_and_introducing_minimum/statement_22.surql"
            ),
            true,
            true,
            true,
        ),
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            include_str!(
                "queries/functions/optional_calls_require_readonly_callees_and_introducing_minimum/statement_23.surql"
            ),
            true,
            true,
            false,
        ),
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            include_str!(
                "queries/functions/optional_calls_require_readonly_callees_and_introducing_minimum/statement_24.surql"
            ),
            false,
            false,
            false,
        ),
        (
            ModuleLayer::Optional,
            ModuleLayer::Optional,
            include_str!(
                "queries/functions/optional_calls_require_readonly_callees_and_introducing_minimum/statement_25.surql"
            ),
            true,
            false,
            false,
        ),
        (
            ModuleLayer::Kernel,
            ModuleLayer::Optional,
            include_str!(
                "queries/functions/optional_calls_require_readonly_callees_and_introducing_minimum/statement_26.surql"
            ),
            true,
            true,
            false,
        ),
    ] {
        let requirement = LaneRequirement::AtLeast {
            module: ModuleName::new("base").unwrap(),
            version: MigrationVersion::new(0),
        };
        let base = module("base", base_layer, vec![body], vec![]);
        let consumer = module(
            "consumer",
            consumer_layer,
            vec![include_str!(
                "queries/functions/optional_calls_require_readonly_callees_and_introducing_minimum/statement_27.surql"
            )],
            if minimum {
                vec![requirement.clone()]
            } else {
                vec![]
            },
        );
        let consumer = if dependency && !minimum {
            ModuleSetup::builder(consumer.name().clone(), consumer.layer())
                .ownership(consumer.ownership().to_vec())
                .execution(consumer.execution().clone())
                .lane(consumer.lane().clone())
                .requires(vec![LaneRequirement::Satisfied(
                    ModuleName::new("base").unwrap(),
                )])
                .build()
                .unwrap()
        } else {
            consumer
        };
        let registry = ModuleRegistry::new(vec![base, consumer]).unwrap();
        assert_eq!(
            prepare(
                registry
                    .select(vec![ModuleName::new("consumer").unwrap()])
                    .unwrap()
            )
            .is_ok(),
            expected
        );
    }
}
#[test]
fn surviving_readonly_children_reject_writer_overwrites_and_removals() {
    for middle in [
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_28.surql"
        ),
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_29.surql"
        ),
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_30.surql"
        ),
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_31.surql"
        ),
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_32.surql"
        ),
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_33.surql"
        ),
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_34.surql"
        ),
        include_str!(
            "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_35.surql"
        ),
    ] {
        let sql = Box::leak(format!(include_str!("queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_36.surql"), middle = middle).into_boxed_str());
        assert!(!admitted(sql), "surviving readonly caller: {sql}");
    }
    assert!(!admitted(include_str!(
        "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_37.surql"
    )));
    assert!(admitted(include_str!(
        "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_38.surql"
    )));
    assert!(admitted(include_str!(
        "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_39.surql"
    )));
    assert!(!admitted(include_str!(
        "queries/functions/surviving_readonly_children_reject_writer_overwrites_and_removals/statement_40.surql"
    )));
}
#[test]
fn scalar_children_and_owned_views_keep_reads_and_mutations_separate() {
    for sql in [
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_41.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_42.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_43.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_44.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_45.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_46.surql"
        ),
    ] {
        assert!(admitted(sql), "{sql}");
    }
    for sql in [
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_47.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_48.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_49.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_50.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_51.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_52.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_53.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_54.surql"
        ),
        include_str!(
            "queries/functions/scalar_children_and_owned_views_keep_reads_and_mutations_separate/statement_55.surql"
        ),
    ] {
        assert!(!admitted(sql), "{sql}");
    }
}

#[test]
fn altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers() {
    assert!(!admitted(include_str!(
        "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_56.surql"
    )));
    assert!(!admitted(include_str!(
        "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_57.surql"
    )));
    assert!(admitted(include_str!(
        "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_58.surql"
    )));
    for middle in [
        include_str!(
            "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_59.surql"
        ),
        include_str!(
            "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_60.surql"
        ),
        include_str!(
            "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_61.surql"
        ),
    ] {
        let sql = Box::leak(format!(include_str!("queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_62.surql"), middle = middle).into_boxed_str());
        assert!(
            !admitted(sql),
            "conditional definition cannot erase stored callback: {sql}"
        );
    }
    assert!(!admitted(include_str!(
        "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_63.surql"
    )));
    assert!(admitted(include_str!(
        "queries/functions/altered_permissions_and_conditional_definitions_cannot_hide_surviving_callers/statement_64.surql"
    )));
}

#[test]
fn optional_minimum_tracks_introduction_across_safe_overwrites() {
    let requirement = LaneRequirement::AtLeast {
        module: ModuleName::new("base").unwrap(),
        version: MigrationVersion::new(0),
    };
    let base = module(
        "base",
        ModuleLayer::Optional,
        vec![
            include_str!(
                "queries/functions/optional_minimum_tracks_introduction_across_safe_overwrites/statement_65.surql"
            ),
            include_str!(
                "queries/functions/optional_minimum_tracks_introduction_across_safe_overwrites/statement_66.surql"
            ),
        ],
        vec![],
    );
    let consumer = module(
        "consumer",
        ModuleLayer::Optional,
        vec![include_str!(
            "queries/functions/optional_minimum_tracks_introduction_across_safe_overwrites/statement_67.surql"
        )],
        vec![requirement],
    );
    let registry = ModuleRegistry::new(vec![base, consumer]).unwrap();
    prepare(
        registry
            .select(vec![ModuleName::new("consumer").unwrap()])
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn ordered_reads_inspect_every_key_and_preserve_readonly_contexts() {
    assert!(admitted(include_str!(
        "queries/functions/ordered_reads/valid.surql"
    )));
    assert!(admitted(include_str!(
        "queries/functions/ordered_reads/field_object.surql"
    )));
    for invalid in [
        include_str!("queries/functions/ordered_reads/field_record.surql"),
        include_str!("queries/functions/ordered_reads/builtin_mutation.surql"),
        include_str!("queries/functions/ordered_reads/foreign.surql"),
        include_str!("queries/functions/ordered_reads/mutation.surql"),
    ] {
        assert!(!admitted(invalid));
    }
}
