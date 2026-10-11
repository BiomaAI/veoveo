//! Actual read-only installation admission before recovery or publication.
use super::*;
use crate::server::test_support::fixture::{TestDb, module_lanes};
use veoveo_modules::*;
use veoveo_platform_store::{StoreCredentials, Value};

async fn snapshot(store: &PlatformStore) -> Value {
    store
        .client()
        .query("RETURN [(SELECT * FROM platform_module_lane ORDER BY id), (SELECT * FROM platform_module_migration ORDER BY id), (SELECT * FROM platform_module_installation ORDER BY id), (SELECT * FROM uav_live_view ORDER BY id), (SELECT * FROM uav_vehicle_control_grant ORDER BY id), (SELECT * FROM task ORDER BY id)];")
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}
async fn refuses_unchanged(startup: &UavStartup, runtime: &PlatformStore, admin: &PlatformStore) {
    let before = snapshot(admin).await;
    assert!(startup.require(runtime).await.is_err());
    assert_eq!(snapshot(admin).await, before);
}

#[tokio::test]
async fn startup_requires_current_uav_lane_prepared_plan_and_runtime_identity_before_effects() {
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        let owner =
            crate::schema::module_setup(module_lanes::execution("uav").unwrap()).unwrap();
        let agents = veoveo_agent_runtime::schema::module_setup(module_lanes::execution("agents").unwrap()).unwrap();
        let db = TestDb::with_modules(vec![agents.clone(), owner.clone()]).await;
        let admin = db.admin().await;
        let registry = module_lanes::registry(vec![agents, owner]).unwrap();
        let selected = vec![ModuleName::new("agents").unwrap(), ModuleName::new("uav").unwrap()];
        let selection = ModuleSelectionDocument::new(
            selected.clone(),
            InstallationGeneration::new(1).unwrap(),
            CredentialRevision::new("uav-fixture").unwrap(),
        )
        .unwrap();
        let plan = ModulePlanDocument::generate(
            &registry,
            &selection,
            CompositionIdentity::new(format!("sha256:{}", "1".repeat(64))).unwrap(),
            Vec::new(),
        )
        .unwrap();
        let runtime_config = StoreConfig::builder(
            admin.config().endpoint().as_str(),
            admin.config().namespace(),
            admin.config().database(),
            StoreCredentials::database("uav_runtime", "fixture-password"),
        )
        .build()
        .unwrap();
        let admit = |plan: &ModulePlanDocument| {
            UavStartup::new(
                plan,
                plan.composition().clone(),
                plan.generation(),
                plan.credential_revision().clone(),
                "uav_runtime",
                &runtime_config,
            )
        };
        let startup = admit(&plan).unwrap();
        // Database-editor access alone is not the admitted invocation account.
        refuses_unchanged(&startup, &db.a, &admin).await;
        let prepared = runner::prepare(registry.select(selected).unwrap()).unwrap();
        let key = preparation_key(&plan, "uav_runtime").unwrap();
        prepared
            .claim_preparation(admin.client(), &key)
            .await
            .unwrap();
        prepared
            .complete_preparation(
                admin.client(),
                &key,
                runner::DatabaseEditorCredentials::new("uav_runtime", "fixture-password")
                    .unwrap(),
            )
            .await
            .unwrap();
        let runtime = PlatformStore::connect(runtime_config.clone())
            .await
            .unwrap();
        startup.require(&runtime).await.unwrap();
        let removed: Vec<Value> = admin
            .client()
            .query("DELETE platform_module_migration WHERE module = 'uav' AND version = 1 RETURN BEFORE;")
            .await
            .unwrap()
            .check()
            .unwrap()
            .take(0)
            .unwrap();
        assert_eq!(removed.len(), 1);
        refuses_unchanged(&startup, &runtime, &admin).await;
        admin
            .client()
            .query("CREATE ONLY $row.id CONTENT $row;")
            .bind(("row", removed[0].clone()))
            .await
            .unwrap()
            .check()
            .unwrap();
        startup.require(&runtime).await.unwrap();
        admin
            .client()
            .query("DELETE platform_module_installation;")
            .await
            .unwrap()
            .check()
            .unwrap();
        refuses_unchanged(&startup, &runtime, &admin).await;
        prepared
            .claim_preparation(admin.client(), &key)
            .await
            .unwrap();
        refuses_unchanged(&startup, &runtime, &admin).await;
        prepared
            .complete_preparation(
                admin.client(),
                &key,
                runner::DatabaseEditorCredentials::new("uav_runtime", "fixture-password")
                    .unwrap(),
            )
            .await
            .unwrap();
        startup.require(&runtime).await.unwrap();
        let mut stale = serde_json::to_value(&plan).unwrap();
        stale["lanes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|lane| lane["module"] == "uav")
            .unwrap()["latest"] = 0.into();
        let stale: ModulePlanDocument = serde_json::from_value(stale).unwrap();
        assert!(admit(&stale).is_err());
        let mut absent = serde_json::to_value(&plan).unwrap();
        absent["lanes"]
            .as_array_mut()
            .unwrap()
            .retain(|lane| lane["module"] != "uav");
        assert!(serde_json::from_value::<ModulePlanDocument>(absent).is_err());
        let mut changed = serde_json::to_value(&plan).unwrap();
        changed["lanes"].as_array_mut().unwrap().iter_mut()
            .find(|lane| lane["module"] == "uav").unwrap()["laneSha256"] =
            serde_json::Value::String(format!("sha256:{}", "f".repeat(64)));
        let changed: ModulePlanDocument = serde_json::from_value(changed).unwrap();
        assert!(admit(&changed).is_err());
        assert!(UavStartup::new(
            &plan,
            CompositionIdentity::new(format!("sha256:{}", "2".repeat(64))).unwrap(),
            plan.generation(), plan.credential_revision().clone(), "uav_runtime", &runtime_config
        ).is_err());
        assert!(
            UavStartup::new(
                &plan,
                plan.composition().clone(),
                InstallationGeneration::new(2).unwrap(),
                plan.credential_revision().clone(),
                "uav_runtime",
                &runtime_config
            )
            .is_err()
        );
        assert!(
            UavStartup::new(
                &plan,
                plan.composition().clone(),
                plan.generation(),
                CredentialRevision::new("foreign").unwrap(),
                "uav_runtime",
                &runtime_config
            )
            .is_err()
        );
        assert!(
            UavStartup::new(
                &plan,
                plan.composition().clone(),
                plan.generation(),
                plan.credential_revision().clone(),
                "foreign_runtime",
                &runtime_config
            )
            .is_err()
        );
    })
    .await
    .expect("Uav startup admission exceeded 120 seconds");
}
