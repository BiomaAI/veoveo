#![cfg(feature = "serialization")]
use veoveo_modules::*;
fn selection() -> ModuleSelectionDocument {
    ModuleSelectionDocument::new(
        vec![ModuleName::new("optional").unwrap()],
        InstallationGeneration::new(u64::MAX).unwrap(),
        CredentialRevision::new("fixture-rotation").unwrap(),
    )
    .unwrap()
}
fn registry() -> ModuleRegistry {
    let setup = |name, layer, requires| {
        ModuleSetup::builder(ModuleName::new(name).unwrap(), layer)
            .lane(MigrationLane::empty())
            .ownership(vec![])
            .requires(requires)
            .execution(
                LaneExecution::new(
                    ExecutionImage::new("gateway").unwrap(),
                    ExecutionCommand::new(vec!["/gateway".into(), "module-migrate".into()])
                        .unwrap(),
                )
                .unwrap(),
            )
            .build()
            .unwrap()
    };
    ModuleRegistry::new(vec![
        setup("base", ModuleLayer::Kernel, vec![]),
        setup(
            "optional",
            ModuleLayer::Optional,
            vec![LaneRequirement::Satisfied(ModuleName::new("base").unwrap())],
        ),
    ])
    .unwrap()
}
#[test]
fn generation_round_trips_without_json_number_precision_and_rejects_aliases() {
    let selection = selection();
    let json = serde_json::to_value(&selection).unwrap();
    assert_eq!(json["generation"], u64::MAX.to_string());
    assert_eq!(
        serde_json::from_value::<ModuleSelectionDocument>(json.clone()).unwrap(),
        selection
    );
    for bad in [
        serde_json::json!(0),
        serde_json::json!("0"),
        serde_json::json!("01"),
        serde_json::json!("18446744073709551616"),
    ] {
        let mut value = json.clone();
        value["generation"] = bad;
        assert!(serde_json::from_value::<ModuleSelectionDocument>(value).is_err());
    }
}
#[test]
fn plan_admission_checks_ordered_dependencies_and_complete_host_predicates() {
    let registry = registry();
    let binding = ModuleRuntimeBinding {
        module: ModuleName::new("optional").unwrap(),
        component: Some(RuntimeBindingKey::new("optional-host").unwrap()),
        mcp_server: None,
    };
    let plan = ModulePlanDocument::generate(
        &registry,
        &selection(),
        CompositionIdentity::new(format!("sha256:{}", "a".repeat(64))).unwrap(),
        vec![binding.clone()],
    )
    .unwrap();
    let mut wrong = serde_json::to_value(&plan).unwrap();
    wrong["lanes"].as_array_mut().unwrap().reverse();
    assert!(serde_json::from_value::<ModulePlanDocument>(wrong).is_err());
    let empty = ModuleSelectionDocument::new(
        vec![],
        selection().generation(),
        selection().credential_revision().clone(),
    )
    .unwrap();
    let unselected =
        ModulePlanDocument::generate(&registry, &empty, plan.composition().clone(), vec![binding])
            .unwrap();
    assert_eq!(unselected.lanes().len(), 1);
    assert_eq!(unselected.runtime_bindings().len(), 1);
    let mut wrong = serde_json::to_value(unselected).unwrap();
    wrong["runtimeBindings"][0]["component"] = serde_json::Value::Null;
    assert!(serde_json::from_value::<ModulePlanDocument>(wrong).is_err());
}
