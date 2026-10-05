use veoveo_modules::*;
fn setup(
    name: &str,
    layer: ModuleLayer,
    requires: Vec<LaneRequirement>,
    claims: Vec<OwnershipClaim>,
    lane: MigrationLane,
) -> ModuleSetup {
    ModuleSetup::builder(ModuleName::new(name).unwrap(), layer)
        .execution(
            LaneExecution::new(
                ExecutionImage::new("gateway").unwrap(),
                ExecutionCommand::new(vec!["module-lane".into(), name.into()]).unwrap(),
            )
            .unwrap(),
        )
        .requires(requires)
        .ownership(claims)
        .lane(lane)
        .build()
        .unwrap()
}
fn required(name: &str) -> LaneRequirement {
    LaneRequirement::Satisfied(ModuleName::new(name).unwrap())
}
fn migration(version: u32, requirements: Vec<LaneRequirement>) -> Migration {
    Migration::new(
        MigrationVersion::new(version),
        MigrationName::new(format!("step_{version}")).unwrap(),
        include_str!("queries/declarations/migration/statement_1.surql"),
    )
    .unwrap()
    .with_requirements(requirements)
    .unwrap()
}
#[test]
fn names_execution_and_zero_based_lane_are_checked() {
    assert!(ModuleName::new("Bad").is_err());
    assert!(TableName::new("x; DELETE y").is_err());
    assert!(FunctionName::new("x::y").is_err());
    assert!(ExecutionImage::new("gateway command").is_err());
    assert!(ExecutionCommand::new(vec![]).is_err());
    let lane = MigrationLane::new(vec![migration(0, vec![])]).unwrap();
    assert_eq!(lane.latest(), Some(MigrationVersion::new(0)));
    assert_eq!(lane.migrations()[0].filename(), "0000_step_0.surql");
    assert_eq!(MigrationLane::empty().latest(), None);
    assert!(MigrationLane::new(vec![migration(1, vec![])]).is_err());
}
#[test]
fn prerequisites_cannot_be_lowered_or_removed() {
    let req = LaneRequirement::AtLeast {
        module: ModuleName::new("base").unwrap(),
        version: MigrationVersion::new(1),
    };
    assert!(
        MigrationLane::new(vec![migration(0, vec![req.clone()]), migration(1, vec![])]).is_err()
    );
    assert!(
        MigrationLane::new(vec![
            migration(0, vec![req]),
            migration(1, vec![required("base")])
        ])
        .is_err()
    );
}
#[test]
fn catalog_rejects_overlaps_cycles_unknowns_and_layer_inversions() {
    let module =
        |name: &str, layer, deps, claims| setup(name, layer, deps, claims, MigrationLane::empty());
    assert!(
        ModuleRegistry::new(vec![
            module(
                "a",
                ModuleLayer::Kernel,
                vec![],
                vec![OwnershipClaim::TablePrefix(
                    TablePrefix::new("thing_").unwrap()
                )]
            ),
            module(
                "b",
                ModuleLayer::Kernel,
                vec![],
                vec![OwnershipClaim::Table(
                    TableName::new("thing_detail").unwrap()
                )]
            )
        ])
        .is_err()
    );
    assert!(
        ModuleRegistry::new(vec![
            module("a", ModuleLayer::Kernel, vec![required("b")], vec![]),
            module("b", ModuleLayer::Kernel, vec![required("a")], vec![])
        ])
        .is_err()
    );
    assert!(
        ModuleRegistry::new(vec![module(
            "a",
            ModuleLayer::Kernel,
            vec![required("missing")],
            vec![]
        )])
        .is_err()
    );
    assert!(
        ModuleRegistry::new(vec![
            module("a", ModuleLayer::Kernel, vec![required("b")], vec![]),
            module("b", ModuleLayer::Optional, vec![], vec![])
        ])
        .is_err()
    );
    assert!(
        ModuleRegistry::new(vec![module(
            "a",
            ModuleLayer::Optional,
            vec![],
            vec![OwnershipClaim::Table(TableName::new(LANE_TABLE).unwrap())]
        )])
        .is_err()
    );
}
#[test]
fn selection_closes_dependencies_and_preserves_complete_catalog() {
    let registry = ModuleRegistry::new(vec![
        setup(
            "optional",
            ModuleLayer::Optional,
            vec![required("dependency")],
            vec![],
            MigrationLane::empty(),
        ),
        setup(
            "other",
            ModuleLayer::Optional,
            vec![],
            vec![],
            MigrationLane::empty(),
        ),
        setup(
            "dependency",
            ModuleLayer::Optional,
            vec![required("kernel")],
            vec![OwnershipClaim::Analyzer(
                AnalyzerName::new("search").unwrap(),
            )],
            MigrationLane::empty(),
        ),
        setup(
            "kernel",
            ModuleLayer::Kernel,
            vec![],
            vec![],
            MigrationLane::empty(),
        ),
    ])
    .unwrap();
    let selection = registry
        .select(vec![ModuleName::new("optional").unwrap()])
        .unwrap();
    let names: Vec<_> = selection
        .ordered()
        .iter()
        .map(|s| s.name().as_str())
        .collect();
    assert_eq!(names, ["kernel", "dependency", "optional"]);
    assert!(
        registry
            .module(&ModuleName::new("other").unwrap())
            .is_some()
    );
    assert!(!selection.contains(&ModuleName::new("other").unwrap()));
    assert_eq!(
        registry
            .owner_of_analyzer(&AnalyzerName::new("search").unwrap())
            .unwrap()
            .name()
            .as_str(),
        "dependency"
    );
}
