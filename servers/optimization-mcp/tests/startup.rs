//! Read-only startup qualification on an isolated database; no GPU executor.
#[path = "support/reads.rs"]
mod fixture;
#[path = "../../../testing/fixtures/store.rs"]
mod store;
use std::{sync::Arc, time::Duration};
use surrealdb::types::Value;
use veoveo_modules::*;
use veoveo_optimization_mcp::composition::RuntimeInstallation;

async fn snapshot(store: &veoveo_platform_store::PlatformStore) -> Vec<Vec<Value>> {
    let mut response = store
        .client()
        .query(include_str!("queries/startup/snapshot.surql"))
        .await
        .unwrap()
        .check()
        .unwrap();
    (0..4).map(|index| response.take(index).unwrap()).collect()
}
async fn rejected(
    startup: RuntimeInstallation<'_>,
    runtime: &veoveo_platform_store::PlatformStore,
) {
    let before = snapshot(runtime).await;
    assert!(startup.require(runtime).await.is_err());
    assert_eq!(
        snapshot(runtime).await,
        before,
        "startup rejection mutated retained state"
    );
}
#[tokio::test]
async fn startup_requires_selected_histories_preparation_and_all_installation_identities() {
    tokio::time::timeout(Duration::from_secs(180), async {
        let optional = || {
            vec![
                veoveo_optimization_mcp::schema::module_setup(
                    store::module_lanes::execution("optimization").unwrap(),
                )
                .unwrap(),
            ]
        };
        let db = store::TestDb::with_modules(optional()).await;
        store::module_lanes::install(&db.a, optional())
            .await
            .unwrap();
        let tasks = fixture::runtime(db.a.clone(), "startup-fixture");
        let owner = fixture::owner(Some("fixture"), "owner", "startup", &[]);
        let retained = fixture::create(&tasks, &owner, 1).await;
        let saved = tasks.get(retained.task).await.unwrap().unwrap();
        assert_eq!(saved.task_id, retained.task);
        let result: rmcp::model::CallToolResult =
            serde_json::from_value(saved.result.unwrap()).unwrap();
        let output: veoveo_optimization_mcp::contract::OptimizationToolOutput =
            serde_json::from_value(
                serde_json::to_value(result.structured_content.unwrap()).unwrap(),
            )
            .unwrap();
        assert_eq!(output.result_uri, retained.solution);
        assert_eq!(
            output.problem_uri,
            veoveo_optimization_mcp::contract::OptimizationProblemUri::new(retained.problem)
                .unwrap()
        );
        assert_eq!(
            output.run_uri,
            veoveo_optimization_mcp::contract::OptimizationRunUri::new(retained.run).unwrap()
        );
        let registry = store::module_lanes::registry(optional()).unwrap();
        let enabled = vec![ModuleName::new("optimization").unwrap()];
        let selection = ModuleSelectionDocument::new(
            enabled.clone(),
            "1".parse().unwrap(),
            CredentialRevision::new("fixture-v1").unwrap(),
        )
        .unwrap();
        let plan = ModulePlanDocument::generate(
            &registry,
            &selection,
            CompositionIdentity::new(format!("sha256:{}", "a".repeat(64))).unwrap(),
            vec![],
        )
        .unwrap();
        let startup = RuntimeInstallation {
            plan: &plan,
            composition: plan.composition(),
            generation: plan.generation(),
            credential_revision: plan.credential_revision(),
            runtime_username: db.a.config().username(),
        };
        rejected(startup, &db.a).await;
        let admin = db.admin().await;
        let prepared = veoveo_modules::runner::prepare(registry.select(enabled).unwrap()).unwrap();
        let key = preparation_key(&plan, startup.runtime_username).unwrap();
        prepared
            .claim_preparation(admin.client(), &key)
            .await
            .unwrap();
        rejected(startup, &db.a).await;
        prepared
            .complete_preparation(
                admin.client(),
                &key,
                veoveo_modules::runner::DatabaseEditorCredentials::new(
                    startup.runtime_username,
                    "isolated-fixture-runtime-password",
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let runtime = veoveo_platform_store::PlatformStore::connect(
            veoveo_platform_store::StoreConfig::builder(
                db.a.config().endpoint().as_str(),
                db.a.config().namespace(),
                db.a.config().database(),
                veoveo_platform_store::StoreCredentials::database(
                    startup.runtime_username,
                    "isolated-fixture-runtime-password",
                ),
            )
            .audit_targets(Arc::new(db.a.audit_targets().clone()))
            .build()
            .unwrap(),
        )
        .await
        .unwrap();
        // Knowledge is installed by the kernel but outside Optimization's dependency
        // closure. Drift must neither block Optimization nor be repaired by startup.
        let mut response = admin
            .client()
            .query(include_str!("queries/startup/history.surql"))
            .bind(("module", "knowledge".to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let unrelated_rows: Vec<Value> = response.take(0).unwrap();
        assert!(
            !unrelated_rows.is_empty(),
            "fixture needs unrelated Knowledge history"
        );
        admin
            .client()
            .query(include_str!("queries/startup/drift.surql"))
            .bind(("module", "knowledge".to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
        let before = snapshot(&runtime).await;
        startup.require(&runtime).await.unwrap();
        assert_eq!(snapshot(&runtime).await, before);
        let composition = CompositionIdentity::new(format!("sha256:{}", "b".repeat(64))).unwrap();
        rejected(
            RuntimeInstallation {
                composition: &composition,
                ..startup
            },
            &runtime,
        )
        .await;
        rejected(
            RuntimeInstallation {
                generation: "2".parse().unwrap(),
                ..startup
            },
            &runtime,
        )
        .await;
        let revision = CredentialRevision::new("different-rotation").unwrap();
        rejected(
            RuntimeInstallation {
                credential_revision: &revision,
                ..startup
            },
            &runtime,
        )
        .await;
        rejected(
            RuntimeInstallation {
                runtime_username: "different-runtime",
                ..startup
            },
            &runtime,
        )
        .await;
        let selection = ModuleSelectionDocument::new(
            vec![],
            plan.generation(),
            plan.credential_revision().clone(),
        )
        .unwrap();
        let unselected =
            ModulePlanDocument::generate(&registry, &selection, plan.composition().clone(), vec![])
                .unwrap();
        rejected(
            RuntimeInstallation {
                plan: &unselected,
                ..startup
            },
            &runtime,
        )
        .await;
        for module in ["tasks", "optimization"] {
            let mut response = admin
                .client()
                .query(include_str!("queries/startup/history.surql"))
                .bind(("module", module.to_owned()))
                .await
                .unwrap()
                .check()
                .unwrap();
            let rows: Vec<Value> = response.take(0).unwrap();
            assert!(!rows.is_empty());
            admin
                .client()
                .query(include_str!("queries/startup/remove_history.surql"))
                .bind(("module", module.to_owned()))
                .await
                .unwrap()
                .check()
                .unwrap();
            rejected(startup, &runtime).await;
            admin
                .client()
                .query(include_str!("queries/startup/restore_history.surql"))
                .bind(("rows", rows.clone()))
                .await
                .unwrap()
                .check()
                .unwrap();
            startup.require(&runtime).await.unwrap();
            admin
                .client()
                .query(include_str!("queries/startup/drift.surql"))
                .bind(("module", module.to_owned()))
                .await
                .unwrap()
                .check()
                .unwrap();
            rejected(startup, &runtime).await;
            admin
                .client()
                .query(include_str!("queries/startup/remove_history.surql"))
                .bind(("module", module.to_owned()))
                .await
                .unwrap()
                .check()
                .unwrap();
            admin
                .client()
                .query(include_str!("queries/startup/restore_history.surql"))
                .bind(("rows", rows))
                .await
                .unwrap()
                .check()
                .unwrap();
        }
        let before = snapshot(&runtime).await;
        startup.require(&runtime).await.unwrap();
        assert_eq!(
            snapshot(&runtime).await,
            before,
            "valid startup changed unrelated drift"
        );
        admin
            .client()
            .query(include_str!("queries/startup/remove_history.surql"))
            .bind(("module", "knowledge".to_owned()))
            .await
            .unwrap()
            .check()
            .unwrap();
        admin
            .client()
            .query(include_str!("queries/startup/restore_history.surql"))
            .bind(("rows", unrelated_rows))
            .await
            .unwrap()
            .check()
            .unwrap();
        startup.require(&runtime).await.unwrap();
    })
    .await
    .expect("Optimization startup fixture exceeded 180 seconds");
}
