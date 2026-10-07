//! Actual native admission before the plane's object-store effect.
use super::{native_database::Database, native_store::module_lanes};
use crate::{ObjectStoreConfig, startup::ArtifactStartup};
use veoveo_modules::*;
use veoveo_platform_store::{PlatformStore, StoreAuthLevel, StoreConfig, StoreCredentials};

async fn snapshot(admin: &PlatformStore) -> veoveo_platform_store::Value {
    admin
        .client()
        .query(include_str!(
            "../../../tests/queries/service/tests/startup/snapshot.surql"
        ))
        .await
        .unwrap()
        .check()
        .unwrap()
        .take(0)
        .unwrap()
}
async fn refuses_without_effects(
    startup: &ArtifactStartup,
    runtime: &PlatformStore,
    admin: &PlatformStore,
    objects: &ObjectStoreConfig,
    destination: &std::path::Path,
) {
    let before = snapshot(admin).await;
    assert!(
        startup
            .prepare_object_store(runtime, objects)
            .await
            .is_err()
    );
    assert!(!destination.exists());
    assert_eq!(snapshot(admin).await, before);
}

#[tokio::test]
async fn startup_refuses_unprepared_foreign_and_incomplete_lanes_before_filesystem_effects() {
    tokio::time::timeout(std::time::Duration::from_secs(90), async {
        let mut database = Database::start();
        let admin = database.connect_unmigrated().await;
        let registry = module_lanes::registry(Vec::new()).unwrap();
        let selection = ModuleSelectionDocument::new(
            Vec::new(),
            InstallationGeneration::new(1).unwrap(),
            CredentialRevision::new("startup-fixture").unwrap(),
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
            StoreCredentials::new(
                StoreAuthLevel::Database,
                "artifact_runtime",
                "fixture-password",
            ),
        )
        .build()
        .unwrap();
        let admit = |plan: &ModulePlanDocument| {
            ArtifactStartup::new(
                plan,
                plan.composition().clone(),
                plan.generation(),
                plan.credential_revision().clone(),
                "artifact_runtime",
                &runtime_config,
            )
        };
        let startup = admit(&plan).unwrap();
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("objects");
        let objects = ObjectStoreConfig::Filesystem {
            root: destination.clone(),
        };
        // A foreign administrative connection cannot stand in for the admitted runtime.
        assert!(
            startup
                .prepare_object_store(&admin, &objects)
                .await
                .is_err()
        );
        assert!(!destination.exists());
        let prepared = runner::prepare(registry.select(Vec::new()).unwrap()).unwrap();
        let key = preparation_key(&plan, "artifact_runtime").unwrap();
        prepared
            .claim_preparation(admin.client(), &key)
            .await
            .unwrap();
        prepared
            .complete_preparation(
                admin.client(),
                &key,
                runner::DatabaseEditorCredentials::new("artifact_runtime", "fixture-password")
                    .unwrap(),
            )
            .await
            .unwrap();
        let runtime = PlatformStore::connect(runtime_config.clone())
            .await
            .unwrap();
        assert!(
            startup
                .prepare_object_store(&runtime, &objects)
                .await
                .is_err()
        );
        assert!(!destination.exists());
        prepared.apply(admin.client()).await.unwrap();
        startup
            .prepare_object_store(&runtime, &objects)
            .await
            .unwrap();
        assert!(destination.is_dir());
        std::fs::remove_dir(&destination).unwrap();
        // The actual runtime account exists and authenticates, but no completed
        // preparation is retained. Startup may neither prepare nor repair it.
        admin.client().query(include_str!("../../../tests/queries/service/tests/startup/remove_preparation.surql"))
            .await.unwrap().check().unwrap();
        refuses_without_effects(&startup, &runtime, &admin, &objects, &destination).await;
        prepared.claim_preparation(admin.client(), &key).await.unwrap();
        refuses_without_effects(&startup, &runtime, &admin, &objects, &destination).await;
        prepared.complete_preparation(admin.client(), &key,
            runner::DatabaseEditorCredentials::new("artifact_runtime", "fixture-password").unwrap())
            .await.unwrap();
        // Schema-admitted missing current history cannot authorize runtime repair.
        let removed: Vec<veoveo_platform_store::Value> = admin.client()
            .query(include_str!("../../../tests/queries/service/tests/startup/remove_current_artifact_migration.surql"))
            .await.unwrap().check().unwrap().take(0).unwrap();
        assert_eq!(removed.len(), 1);
        refuses_without_effects(&startup, &runtime, &admin, &objects, &destination).await;
        admin.client().query(include_str!("../../../tests/queries/service/tests/startup/restore_migration.surql"))
            .bind(("row", removed[0].clone())).await.unwrap().check().unwrap();
        startup.require(&runtime).await.unwrap();
        // The prepared full plan identity fences even a semantically unrelated identity change.
        let foreign = ModulePlanDocument::generate(
            &registry,
            &selection,
            CompositionIdentity::new(format!("sha256:{}", "2".repeat(64))).unwrap(),
            Vec::new(),
        )
        .unwrap();
        assert!(
            admit(&foreign)
                .unwrap()
                .prepare_object_store(&runtime, &objects)
                .await
                .is_err()
        );
        assert!(!destination.exists());
        let mut old_lane = serde_json::to_value(&plan).unwrap();
        let artifacts = old_lane["lanes"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|lane| lane["module"] == "artifacts")
            .unwrap();
        artifacts["latest"] = serde_json::json!(0);
        let old_lane: ModulePlanDocument = serde_json::from_value(old_lane).unwrap();
        assert!(admit(&old_lane).is_err());
        let mut omitted = serde_json::to_value(&plan).unwrap();
        omitted["lanes"].as_array_mut().unwrap().retain(|lane| lane["module"] != "tasks");
        let before = snapshot(&admin).await;
        // Removing a required lane also invalidates the plan's own dependency
        // closure. The shared plan decoder refuses it before startup effects.
        assert!(serde_json::from_value::<ModulePlanDocument>(omitted).is_err());
        assert_eq!(snapshot(&admin).await, before);

        assert!(!destination.exists());
        assert!(
            ArtifactStartup::new(
                &plan,
                plan.composition().clone(),
                InstallationGeneration::new(2).unwrap(),
                plan.credential_revision().clone(),
                "artifact_runtime",
                &runtime_config
            )
            .is_err()
        );
        assert!(
            ArtifactStartup::new(
                &plan,
                plan.composition().clone(),
                plan.generation(),
                CredentialRevision::new("other-revision").unwrap(),
                "artifact_runtime",
                &runtime_config
            )
            .is_err()
        );
        assert!(
            ArtifactStartup::new(
                &plan,
                plan.composition().clone(),
                plan.generation(),
                plan.credential_revision().clone(),
                "other_runtime",
                &runtime_config
            )
            .is_err()
        );
        let rotated_selection = ModuleSelectionDocument::new(Vec::new(),
            InstallationGeneration::new(2).unwrap(), CredentialRevision::new("rotated-credentials").unwrap()).unwrap();
        let rotated = ModulePlanDocument::generate(&registry, &rotated_selection,
            plan.composition().clone(), Vec::new()).unwrap();
        let rotated_key = preparation_key(&rotated, "artifact_runtime").unwrap();
        prepared.claim_preparation(admin.client(), &rotated_key).await.unwrap();
        prepared.complete_preparation(admin.client(), &rotated_key,
            runner::DatabaseEditorCredentials::new("artifact_runtime", "rotated-password").unwrap())
            .await.unwrap();
        refuses_without_effects(&startup, &runtime, &admin, &objects, &destination).await;
        assert!(PlatformStore::connect(runtime_config.clone()).await.is_err());
        let rotated_config = StoreConfig::builder(admin.config().endpoint().as_str(),
            admin.config().namespace(), admin.config().database(),
            StoreCredentials::new(StoreAuthLevel::Database, "artifact_runtime", "rotated-password"))
            .build().unwrap();
        let rotated_runtime = PlatformStore::connect(rotated_config.clone()).await.unwrap();
        let rotated_startup = ArtifactStartup::new(&rotated, rotated.composition().clone(),
            rotated.generation(), rotated.credential_revision().clone(), "artifact_runtime", &rotated_config).unwrap();
        rotated_startup.require(&rotated_runtime).await.unwrap();
        admin.client().query(include_str!("../../../tests/queries/service/tests/startup/corrupt_checksum.surql"))
            .bind(("row", removed[0].clone())).await.unwrap().check().unwrap();
        refuses_without_effects(&rotated_startup, &rotated_runtime, &admin, &objects, &destination).await;
        database.finish();
    })
    .await
    .expect("Artifact startup fixture exceeded90seconds");
}
