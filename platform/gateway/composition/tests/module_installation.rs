//! Real gateway command lifecycle against the digest-pinned isolated SurrealDB fixture.
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use surrealdb::{Surreal, engine::remote::ws::Ws, opt::auth::Root, types::SurrealValue};
use tokio::process::Command;
use veoveo_modules::ModulePlanDocument;
#[path = "../../../../testing/fixtures/store/container.rs"]
mod container;
struct Directory(PathBuf);
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
async fn process(args: &[&str], env: &[(&str, String)]) -> std::process::Output {
    let directory = env
        .iter()
        .find(|(key, _)| *key == "VEOVEO_MODULE_PLAN")
        .map(|(_, value)| Path::new(value).parent().unwrap().to_owned())
        .or_else(|| {
            args.iter()
                .position(|arg| *arg == "--modules")
                .map(|index| Path::new(args[index + 1]).parent().unwrap().to_owned())
        })
        .expect("each fixture command has an owned isolated directory");
    let mut command = Command::new(env!("CARGO_BIN_EXE_gateway"));
    command
        .env_clear()
        .current_dir(directory)
        .args(args)
        .envs(env.iter().map(|(k, v)| (*k, v)))
        .kill_on_drop(true);
    // Cargo supplies its native dependency search path; retain no installation environment.
    if let Some(value) = std::env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", value);
    }
    tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .expect("gateway command exceeded 120 seconds")
        .expect("launch gateway process")
}
fn success(output: std::process::Output) -> String {
    assert!(
        output.status.success(),
        "gateway command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn path(path: &Path) -> &str {
    path.to_str().unwrap()
}
#[tokio::test]
#[ignore = "requires Docker and locally available digest-pinned SurrealDB 3.3.0"]
async fn process_preparation_lanes_publication_and_stale_generation_fail_closed() {
    tokio::time::timeout(Duration::from_secs(300), async {
        let root_password = format!("fixture-{}", uuid::Uuid::now_v7().simple());
        let (_container, endpoint) = container::Container::start(container::Docker::default(), "memory", &root_password).await.unwrap();
        let directory = Directory(std::env::temp_dir().join(format!("veoveo-installation-process-{}", uuid::Uuid::now_v7().simple())));
        std::fs::create_dir(&directory.0).unwrap();
        let selection = directory.0.join("selection.json"); let plan_path = directory.0.join("plan.json"); let seed = directory.0.join("control-plane.json");
        std::fs::write(&selection, r#"{"format":"veoveo.ai/module-selection/v1","enabled":["time"],"generation":"1","credentialRevision":"fixture-v1"}"#).unwrap();
        let composition = format!("sha256:{}", "1".repeat(64));
        let generated = success(process(&["module-plan","--modules",path(&selection),"--composition",&composition], &[]).await);
        let plan: ModulePlanDocument = serde_json::from_str(&generated).unwrap();
        let exported: serde_json::Value = serde_json::from_str(&success(process(&["module-plan", "--modules", path(&selection), "--composition", &composition, "--helm-values"], &[]).await)).unwrap();
        assert_eq!(exported["moduleInstallation"]["planJson"], generated.trim());
 std::fs::write(&plan_path, &generated).unwrap();
        let control = serde_json::json!({"identityProviders":[],"authorizationServers":[],"servers":[],"profiles":[],"tenants":[{"id":"fixture","metadata":{}}],"workContexts":[{"id":"mission","tenant":"fixture","title":"Mission","policyRevision":"policy-fixture","outputPolicy":{"owner":{"kind":"group","id":"operations"},"initialGrants":[],"classification":null,"dataLabels":[]},"memberships":[{"level":"contributor","groups":["operations"]}]}],"policies":[{"version":"policy-fixture","rules":[],"metadata":{}}],"dataLabels":[],"oidcClients":[]});
        // Decode the real owner model before the process runs; the fixture is not an open JSON contract.
        let control: veoveo_mcp_contract::GatewayControlPlane = serde_json::from_value(control).unwrap();
        std::fs::write(&seed, serde_json::to_vec(&control).unwrap()).unwrap();
        let mut root = vec![("VEOVEO_SURREAL_ENDPOINT", endpoint.clone()),("VEOVEO_SURREAL_NAMESPACE","module_cli".into()),("VEOVEO_SURREAL_DATABASE","fresh_install".into()),("VEOVEO_SURREAL_AUTH_LEVEL","root".into()),("VEOVEO_SURREAL_USERNAME","fixture_admin".into()),("VEOVEO_SURREAL_PASSWORD",root_password.clone()),("VEOVEO_SURREAL_RUNTIME_USERNAME","runtime".into()),("VEOVEO_SURREAL_RUNTIME_PASSWORD","runtime-secret".into()),("VEOVEO_MODULE_PLAN",path(&plan_path).into()),("VEOVEO_MODULE_COMPOSITION",composition.clone()),("VEOVEO_INSTALLATION_GENERATION","1".into()),("VEOVEO_CREDENTIAL_REVISION","fixture-v1".into())];
        // Await transport readiness without creating the namespace/database: prepare owns that proof.
        let address = endpoint.strip_prefix("ws://").unwrap();
        let db = tokio::time::timeout(Duration::from_secs(30), async { loop {
            if let Ok(db) = Surreal::new::<Ws>(address).await
                && db.signin(Root {
                    username: "fixture_admin".into(),
                    password: root_password.clone(),
                }).await.is_ok()
            {
                break db;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }}).await.unwrap();
        let mut runtime = root.clone();
        for (name,value) in &mut runtime { match *name {"VEOVEO_SURREAL_AUTH_LEVEL" => *value="database".into(), "VEOVEO_SURREAL_USERNAME" => *value="runtime".into(), "VEOVEO_SURREAL_PASSWORD" => *value="runtime-secret".into(), _ => {} } }
        let waiting_lane_env = root.clone();
        let waiting_lane = tokio::spawn(async move { process(&["module-migrate", "--module", "time", "--wait-seconds", "60"], &waiting_lane_env).await });
        let waiting_publish_env = runtime.clone(); let waiting_seed = seed.clone();
        let waiting_publish = tokio::spawn(async move { process(&["control-plane-publish", "--control-plane", path(&waiting_seed), "--applied-by", "declared-operator", "--wait-seconds", "60"], &waiting_publish_env).await });
        tokio::time::sleep(Duration::from_millis(300)).await;
        if waiting_lane.is_finished() { let output = waiting_lane.await.unwrap(); panic!("lane exited before preparation: status {} stdout {} stderr {}", output.status, String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)); }
        if waiting_publish.is_finished() { let output = waiting_publish.await.unwrap(); panic!("publisher exited before preparation: status {} stdout {} stderr {}", output.status, String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr)); }
        success(process(&["installation-prepare"], &root).await);
        db.use_ns("module_cli").use_db("fresh_install").await.unwrap();
        let publish = ["control-plane-publish","--control-plane",path(&seed),"--applied-by","declared-operator","--wait-seconds","0"];
        assert!(!process(&publish, &runtime).await.status.success(), "publication must wait for selected lane headers");
        assert!(!process(&["module-migrate","--module","map","--wait-seconds","0"], &root).await.status.success());
        assert!(!process(&["module-migrate","--module","time","--wait-seconds","0"], &root).await.status.success(), "dependency lane is not complete");
        for lane in plan.lanes() { if lane.module.as_str() == "time" { continue; } success(process(&["module-migrate","--module",lane.module.as_str(),"--wait-seconds","0"], &root).await); }
        success(waiting_lane.await.unwrap());
        let original_publication = success(waiting_publish.await.unwrap());
        let original: serde_json::Value = serde_json::from_str(&original_publication).unwrap();
        assert_eq!(original["status"], "published");
        success(process(&["module-status"], &runtime).await);
        let first: serde_json::Value = serde_json::from_str(&success(process(&publish,&runtime).await)).unwrap();
        let repeated: serde_json::Value = serde_json::from_str(&success(process(&publish,&runtime).await)).unwrap();
        assert_eq!(original["revisionId"], first["revisionId"]);
        assert_eq!(first["revisionId"], repeated["revisionId"]); assert_eq!(repeated["status"], "unchanged");
        let mut result = db.query(include_str!("queries/module_installation/process_preparation_lanes_publication_and_stale_generation_fail_closed/statement_1.surql")).await.unwrap().check().unwrap();
        let actors: Vec<String> = result.take(0).unwrap(); let applied: Vec<String> = result.take(1).unwrap();
        assert_eq!(actors, vec!["runtime"]); assert_eq!(applied, vec!["declared-operator"]);
        #[derive(Clone, surrealdb::types::SurrealValue)]
        struct Marker { id: surrealdb::types::RecordId, generation: String, identity: String, complete: bool }
        let mut marker_response = db.query(include_str!("queries/module_installation/process_preparation_lanes_publication_and_stale_generation_fail_closed/statement_2.surql")).await.unwrap();
        let marker: Marker = marker_response.take::<Option<Marker>>(0).unwrap().unwrap();
        let old_key = veoveo_modules::PreparationKey::new(marker.generation.parse().unwrap(), marker.identity.clone()).unwrap();
        let control_store = veoveo_mcp_gateway::GatewayControlStore::connect(veoveo_platform_store::StoreConfig::builder(&endpoint, "module_cli", "fresh_install", veoveo_platform_store::StoreCredentials::database("runtime", "runtime-secret")).build().unwrap(), veoveo_mcp_gateway::GatewayCatalogAdmission::unbound().bind(veoveo_gateway_catalog::registry().unwrap()).unwrap()).await.unwrap();
        let attempted = veoveo_mcp_contract::GatewayControlPlaneRevision { revision_id: veoveo_mcp_gateway::new_gateway_control_plane_revision_id().unwrap(), sha256: "c".repeat(64), source: veoveo_mcp_contract::GatewayControlPlaneRevisionSource::SeedFile, applied_at: chrono::Utc::now(), applied_by: veoveo_types::PrincipalId::parse("declared-operator").unwrap(), tenant: None, control_plane: control.clone() };
        let context = veoveo_mcp_contract::audit::AuditContext { actor: veoveo_mcp_contract::audit::AuditActor { principal: veoveo_types::PrincipalId::parse("runtime").unwrap(), kind: veoveo_mcp_contract::audit::AuditPrincipalKind::Service, tenant:None,oauth_client:None,session_family:None,delegating_principal:None,managed_agent:None }, authority:Default::default(), request:veoveo_mcp_contract::audit::AuditRequest::background() };
        // Exercise the publication write method directly. CLI readiness is bypassed
        // here deliberately: the transaction must reject absent proof itself.
        db.query(include_str!("queries/module_installation/process_preparation_lanes_publication_and_stale_generation_fail_closed/statement_3.surql")).await.unwrap().check().unwrap();
        assert!(control_store.record_installation_revision(&attempted, &context, &old_key).await.is_err());
        assert!(control_store.load_installation_revision(&old_key).await.is_err());
        assert_eq!(control_store.revision_count().await.unwrap(), 1);
        db.query(include_str!("queries/module_installation/process_preparation_lanes_publication_and_stale_generation_fail_closed/statement_4.surql")).bind(("record",marker.id.clone())).bind(("content",marker)).await.unwrap().check().unwrap();
        // A newer preparation can have identical empty lane histories. Its generation
        // still fences publication by a delayed client carrying the previous plan.
        std::fs::write(&selection, r#"{"format":"veoveo.ai/module-selection/v1","enabled":["time"],"generation":"2","credentialRevision":"fixture-v2"}"#).unwrap();
        let next = directory.0.join("next-plan.json");
        std::fs::write(&next, success(process(&["module-plan","--modules",path(&selection),"--composition",&composition], &[]).await)).unwrap();
        for (name,value) in &mut root { match *name {"VEOVEO_MODULE_PLAN" => *value=path(&next).into(),"VEOVEO_INSTALLATION_GENERATION" => *value="2".into(),"VEOVEO_CREDENTIAL_REVISION" => *value="fixture-v2".into(), _ => {} } }
        success(process(&["installation-prepare"], &root).await);
        assert!(control_store.record_installation_revision(&attempted, &context, &old_key).await.is_err());
        assert!(control_store.load_installation_revision(&old_key).await.is_err());
        assert_eq!(control_store.revision_count().await.unwrap(), 1);
        assert!(!process(&publish,&runtime).await.status.success(), "stale publisher must not reuse old lane readiness");
        // Each old catalog marker is sufficient for refusal. These are empty marker
        // tables, not a historical migration or a backfill fixture.
        for (database, marker) in [
            ("mixed_platform", "platform_schema_migration"),
            ("mixed_downstream", "platform_downstream_migration"),
        ] {
            db.query(include_str!("queries/module_installation/define_database.surql")).bind(("database", database.to_owned())).await.unwrap().check().unwrap();
            db.use_db(database).await.unwrap();
            db.query(include_str!("queries/module_installation/define_marker.surql")).bind(("marker", marker.to_owned())).await.unwrap().check().unwrap();
            let mut mixed = root.clone();
            for (name, value) in &mut mixed {
                if *name == "VEOVEO_SURREAL_DATABASE" { *value = database.into(); }
            }
            let mut response = db.query(include_str!("queries/module_installation/marker_state.surql")).bind(("marker", marker.to_owned())).await.unwrap().check().unwrap();
            let before: Vec<surrealdb::types::Value> = (0..3).map(|index| response.take(index).unwrap()).collect();
            for command in [
                vec!["module-status", "--wait-seconds", "60"],
                vec!["module-migrate", "--module", "time", "--wait-seconds", "60"],
            ] {
                let output = tokio::time::timeout(Duration::from_secs(10), process(&command, &mixed)).await
                    .expect("mixed catalog refusal must not wait for the 60-second readiness interval");
                assert!(!output.status.success(), "mixed catalog {marker} admitted {command:?}");
                let diagnostic = String::from_utf8_lossy(&output.stderr);
                assert!(diagnostic.contains("database uses the mixed schema catalog")
                    && diagnostic.contains("create a fresh installation with selected module lanes"),
                    "missing actionable marker refusal for {command:?}: {diagnostic}");
                assert!(!diagnostic.contains("did not become ready"), "marker refusal was replaced by readiness timeout: {diagnostic}");
                let mut response = db.query(include_str!("queries/module_installation/marker_state.surql")).bind(("marker", marker.to_owned())).await.unwrap().check().unwrap();
                let after: Vec<surrealdb::types::Value> = (0..3).map(|index| response.take(index).unwrap()).collect();
                assert_eq!(before, after, "marker refusal changed schema or records for {command:?}");
            }
        }
    }).await.expect("gateway installation lifecycle exceeded 300 seconds");
}
