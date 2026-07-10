use super::*;

#[test]
fn builds_server_public_endpoint_for_operator_owned_domain() {
    let deployment =
        PublicDeployment::new("https://veoveo.enterprise.example/").expect("valid deployment");
    let media = deployment.server("media").expect("valid server");

    assert_eq!(deployment.base_url(), "https://veoveo.enterprise.example");
    assert_eq!(deployment.host_authority(), "veoveo.enterprise.example");
    assert_eq!(media.mount_path(), "/media");
    assert_eq!(
        media.public_url(),
        "https://veoveo.enterprise.example/media"
    );
    assert_eq!(media.path("mcp"), "/media/mcp");
}

#[test]
fn rejects_base_url_paths() {
    let error = PublicDeployment::new("https://veoveo.enterprise.example/base")
        .expect_err("base URL path should fail");
    assert!(error.to_string().contains("must not include a path"));
}

#[test]
fn canonical_plan_covers_compose_helm_connected_and_offline() {
    let plan = SelfHostedDeploymentPlan::load_json(canonical_plan_path())
        .expect("canonical deployment plan");

    let shapes = plan
        .profiles
        .iter()
        .map(|profile| (profile.installation_form, profile.connectivity))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        shapes,
        BTreeSet::from([
            (InstallationForm::Compose, ConnectivityMode::Connected),
            (InstallationForm::Compose, ConnectivityMode::Offline),
            (InstallationForm::Helm, ConnectivityMode::Connected),
            (InstallationForm::Helm, ConnectivityMode::Offline),
        ])
    );
}

#[test]
fn rejects_duckdb_as_durable_platform_state() {
    let mut plan = canonical_plan();
    plan.profiles[0].analytical_runtime.durable_platform_state = true;

    let error = plan.validate().expect_err("DuckDB control state must fail");
    assert!(
        error
            .to_string()
            .contains("never as the durable platform store")
    );
}

#[test]
fn preserves_arbitrary_sql_in_the_sandboxed_runtime() {
    let mut plan = canonical_plan();
    plan.profiles[0].analytical_runtime.arbitrary_sql = false;

    let error = plan.validate().expect_err("arbitrary SQL is required");
    assert!(
        error
            .to_string()
            .contains("arbitrary-SQL analytical workspace")
    );
}

#[test]
fn rejects_database_ha_claims() {
    let json = serde_json::to_value(canonical_plan()).expect("serialize plan");
    let mut profile = json["profiles"][0].clone();
    profile["platform_store"]["database_ha"] = serde_json::json!("enabled");

    let error = serde_json::from_value::<SelfHostedDeploymentProfile>(profile)
        .expect_err("unsupported DB HA value must fail");
    assert!(error.to_string().contains("unknown variant"));
}

#[test]
fn offline_install_requires_internal_oidc_reachability() {
    let mut plan = canonical_plan();
    plan.profiles[0].connectivity = ConnectivityMode::Offline;
    plan.profiles[0]
        .identity_provider
        .discovery_available_offline = false;

    let error = plan
        .validate()
        .expect_err("online IdP dependency must fail");
    assert!(error.to_string().contains("online OIDC discovery"));
}

#[test]
fn rejects_wrong_secret_surface_for_installation_form() {
    let mut plan = canonical_plan();
    plan.profiles[0].installation_form = InstallationForm::Compose;
    plan.profiles[0].secret_manager.kind = SecretManagerKind::KubernetesExistingSecret;

    let error = plan
        .validate()
        .expect_err("Kubernetes secret in Compose must fail");
    assert!(
        error
            .to_string()
            .contains("does not match its installation form")
    );
}

#[test]
fn rejects_incomplete_autonomous_service_set() {
    let mut plan = canonical_plan();
    plan.profiles[0]
        .services
        .remove(&DeploymentServiceKind::ConsoleBff);

    let error = plan.validate().expect_err("BFF is required");
    assert!(
        error
            .to_string()
            .contains("canonical autonomous installation")
    );
}

fn canonical_plan() -> SelfHostedDeploymentPlan {
    SelfHostedDeploymentPlan::load_json(canonical_plan_path()).expect("canonical deployment plan")
}

fn canonical_plan_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../configs/deployments.json")
}
