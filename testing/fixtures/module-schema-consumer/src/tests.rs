use super::*;
use veoveo_modules::{
    AnalyzerName, ExecutionCommand, ExecutionImage, FunctionName, ModuleLayer, ModuleName,
    ModuleRegistry, TableName,
};

#[path = "expected.rs"]
mod expected;

fn execution(name: &str) -> Result<LaneExecution, DeclarationError> {
    // A logical composition host and pending command template, not proof of an executable.
    LaneExecution::new(
        ExecutionImage::new("gateway")?,
        ExecutionCommand::new(vec![
            "gateway".into(),
            "module-migrate".into(),
            "--module".into(),
            name.into(),
        ])?,
    )
}

#[test]
fn every_owner_exports_an_independent_lane_and_keeps_supplied_execution() {
    let owners = declarations(execution).unwrap();
    assert_eq!(owners.len(), 19);
    assert_eq!(
        owners
            .iter()
            .filter(|owner| owner.layer() == ModuleLayer::Kernel)
            .count(),
        7
    );
    for owner in &owners {
        assert_eq!(owner.lane().latest().unwrap().get(), 0);
        assert_eq!(
            owner.execution(),
            &execution(owner.name().as_str()).unwrap()
        );
        assert!(owner.extensions().is_empty());
    }
}

// These expectations name objects; they do not parse or approve migration bodies.
#[test]
fn reviewed_catalog_objects_resolve_to_their_declaring_owners() {
    let registry = ModuleRegistry::new(declarations(execution).unwrap()).unwrap();
    assert_eq!(expected::TABLES.len(), 166);
    assert_eq!(expected::FUNCTIONS.len(), 39);
    assert_eq!(expected::ANALYZERS.len(), 2);
    assert_eq!(
        registry
            .owner_of_table(&TableName::new("optimization_task").unwrap())
            .unwrap()
            .name()
            .as_str(),
        "optimization"
    );
    assert_eq!(
        registry
            .owner_of_function(&FunctionName::new("fn::kernel::tasks::selection_v1").unwrap())
            .unwrap()
            .name()
            .as_str(),
        "tasks"
    );
    for &(name, owner) in expected::TABLES {
        assert_eq!(
            registry
                .owner_of_table(&TableName::new(name).unwrap())
                .unwrap()
                .name()
                .as_str(),
            owner,
            "table {name}"
        );
    }
    for &(name, owner) in expected::FUNCTIONS {
        assert_eq!(
            registry
                .owner_of_function(&FunctionName::new(name).unwrap())
                .unwrap()
                .name()
                .as_str(),
            owner,
            "function {name}"
        );
    }
    for &(name, owner) in expected::ANALYZERS {
        assert_eq!(
            registry
                .owner_of_analyzer(&AnalyzerName::new(name).unwrap())
                .unwrap()
                .name()
                .as_str(),
            owner,
            "analyzer {name}"
        );
    }
    for table in [
        veoveo_modules::LANE_TABLE,
        veoveo_modules::MIGRATION_TABLE,
        veoveo_modules::PREPARATION_TABLE,
    ] {
        assert_eq!(
            registry
                .owner_of_table(&TableName::new(table).unwrap())
                .unwrap()
                .name()
                .as_str(),
            "store"
        );
    }
    assert!(
        registry
            .owner_of_table(&TableName::new("independent_table").unwrap())
            .is_none()
    );
    assert!(
        registry
            .owner_of_function(&FunctionName::new("fn::independent").unwrap())
            .is_none()
    );
    assert!(
        registry
            .owner_of_analyzer(&AnalyzerName::new("independent_text").unwrap())
            .is_none()
    );
}

#[test]
fn target_dependencies_order_regardless_of_declaration_order() {
    let mut owners = declarations(execution).unwrap();
    owners.reverse();
    let registry = ModuleRegistry::new(owners).unwrap();
    let ordered = registry.ordered();
    assert_eq!(ordered.len(), 19);
    for (position, owner) in ordered.iter().enumerate() {
        for requirement in owner.requires() {
            let prerequisite = ordered
                .iter()
                .position(|other| other.name() == requirement.module())
                .unwrap();
            assert!(
                prerequisite < position,
                "{} requires {}",
                owner.name(),
                requirement.module()
            );
        }
    }
    let kernels = registry.select(Vec::new()).unwrap();
    assert_eq!(
        kernels
            .ordered()
            .iter()
            .map(|owner| owner.name().as_str())
            .collect::<Vec<_>>(),
        [
            "store",
            "identity",
            "gateway",
            "artifacts",
            "tasks",
            "audit",
            "knowledge"
        ]
    );
}

#[test]
fn optional_selection_adds_only_declared_optional_prerequisites() {
    let registry = ModuleRegistry::new(declarations(execution).unwrap()).unwrap();
    for enabled in ["workspace", "uav"] {
        let selected = registry
            .select(vec![ModuleName::new(enabled).unwrap()])
            .unwrap();
        assert_eq!(selected.ordered().len(), 9);
        assert!(selected.contains(&ModuleName::new("agents").unwrap()));
        assert!(selected.contains(&ModuleName::new(enabled).unwrap()));
        for absent in ["computers", "map", "time", "recordings", "frames", "media"] {
            assert!(
                !selected.contains(&ModuleName::new(absent).unwrap()),
                "{absent}"
            );
        }
    }
    let time = registry
        .select(vec![ModuleName::new("time").unwrap()])
        .unwrap();
    assert_eq!(time.ordered().len(), 8);
    assert!(!time.contains(&ModuleName::new("agents").unwrap()));
    assert!(
        registry
            .select(vec![ModuleName::new("unknown").unwrap()])
            .is_err()
    );
}

#[test]
fn generated_plan_matches_real_owner_catalog_without_runner_dependencies() {
    use veoveo_modules::{ModulePlanDocument, ModuleSelectionDocument};
    let plan: ModulePlanDocument =
        serde_json::from_str(include_str!("../module-plan.json")).unwrap();
    let execution = |name: &str| {
        LaneExecution::new(
            ExecutionImage::new("gateway")?,
            ExecutionCommand::new(vec![
                "/usr/local/bin/gateway".into(),
                "module-migrate".into(),
                "--module".into(),
                name.into(),
            ])?,
        )
    };
    let registry = ModuleRegistry::new(declarations(execution).unwrap()).unwrap();
    let selection: ModuleSelectionDocument =
        serde_json::from_str(include_str!("../selection.json")).unwrap();
    let regenerated = ModulePlanDocument::generate(
        &registry,
        &selection,
        plan.composition().clone(),
        plan.runtime_bindings().to_vec(),
    )
    .unwrap();
    assert_eq!(plan, regenerated);
}

#[test]
fn owner_observation_declarations_resolve_to_their_schema_owner() {
    let registry = ModuleRegistry::new(declarations(execution).unwrap()).unwrap();
    fn verify<T: Copy + Into<veoveo_modules::ObservationTable>>(
        registry: &ModuleRegistry,
        owner: &str,
        tables: &[T],
    ) {
        for table in tables {
            let table: veoveo_modules::ObservationTable = (*table).into();
            assert_eq!(
                registry
                    .owner_of_table(table.name())
                    .unwrap()
                    .name()
                    .as_str(),
                owner
            );
        }
    }
    verify(
        &registry,
        "agents",
        veoveo_agent_runtime::AgentObservationTable::ALL,
    );
    verify(
        &registry,
        "computers",
        veoveo_computers::schema::ComputerObservationTable::ALL,
    );
    verify(
        &registry,
        "recordings",
        veoveo_recording_mcp::schema::RecordingObservationTable::ALL,
    );
    verify(&registry, "map", veoveo_map_mcp::MapObservationTable::ALL);
    verify(
        &registry,
        "time",
        veoveo_time_mcp::TimeObservationTable::ALL,
    );
    verify(
        &registry,
        "frames",
        veoveo_frames_mcp::FramesObservationTable::ALL,
    );
    verify(
        &registry,
        "media",
        veoveo_media_mcp::MediaObservationTable::ALL,
    );
    verify(
        &registry,
        "uav",
        veoveo_uav_sim_mcp::UavObservationTable::ALL,
    );
}
