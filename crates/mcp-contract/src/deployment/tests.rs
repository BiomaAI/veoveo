use super::*;

#[test]
fn builds_server_public_endpoint_under_domain() {
    let deployment = PublicDeployment::new("https://veoveo.bioma.ai/").expect("valid deployment");
    let media = deployment.server("media").expect("valid server");

    assert_eq!(deployment.base_url(), "https://veoveo.bioma.ai");
    assert_eq!(deployment.host_authority(), "veoveo.bioma.ai");
    assert_eq!(media.mount_path(), "/media");
    assert_eq!(media.public_url(), "https://veoveo.bioma.ai/media");
    assert_eq!(media.path("mcp"), "/media/mcp");
    assert_eq!(
        media.url("webhooks"),
        "https://veoveo.bioma.ai/media/webhooks"
    );
}

#[test]
fn base_url_can_have_arbitrary_subdomain_depth() {
    let deployment = PublicDeployment::new("https://deep.staging.enterprise.example.com")
        .expect("valid deployment");
    let media = deployment.server("media").expect("valid server");

    assert_eq!(
        deployment.base_url(),
        "https://deep.staging.enterprise.example.com"
    );
    assert_eq!(
        deployment.host_authority(),
        "deep.staging.enterprise.example.com"
    );
    assert_eq!(media.mount_path(), "/media");
    assert_eq!(
        media.public_url(),
        "https://deep.staging.enterprise.example.com/media"
    );
}

#[test]
fn preserves_explicit_public_port_for_host_validation() {
    let deployment =
        PublicDeployment::new("https://veoveo.bioma.ai:8443").expect("valid deployment");

    assert_eq!(deployment.base_url(), "https://veoveo.bioma.ai:8443");
    assert_eq!(deployment.host_authority(), "veoveo.bioma.ai:8443");
}

#[test]
fn rejects_base_url_paths() {
    let err = PublicDeployment::new("https://veoveo.bioma.ai/base")
        .expect_err("base URL path should fail");

    assert!(err.to_string().contains("must not include a path"));
}

#[test]
fn self_hosted_deployment_plan_validates_from_json() {
    let plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");

    plan.validate().expect("valid deployment plan");
}

#[test]
fn enterprise_deployment_rejects_env_secret_source() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    harden_profile_for_enterprise(&mut plan.profiles[0]);
    plan.profiles[0].kind = DeploymentProfileKind::Enterprise;
    plan.profiles[0].secret_sources = BTreeSet::from([SecretSource::Env]);

    let err = plan.validate().expect_err("env secret source must fail");

    assert!(err.to_string().contains("cannot use env secrets"));
}

#[test]
fn deployment_rejects_unimplemented_secret_source() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    plan.profiles[0].secret_sources = BTreeSet::from([SecretSource::CloudSecretManager]);

    let err = plan
        .validate()
        .expect_err("unimplemented secret source must fail");

    assert!(
        err.to_string()
            .contains("not implemented by the gateway resolver")
    );
}

#[test]
fn deployment_requires_gateway_and_server_state_stores() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    plan.profiles[0].state_stores.clear();

    let err = plan.validate().expect_err("state stores must be explicit");

    assert!(err.to_string().contains("state_stores"));

    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    plan.profiles[0].state_stores[0].owners = BTreeSet::from([StateStoreOwner::Gateway]);
    plan.profiles[0].state_stores[1].owners = BTreeSet::from([StateStoreOwner::Gateway]);

    let err = plan
        .validate()
        .expect_err("hosted server state store must be explicit");

    assert!(err.to_string().contains("hosted-server state store"));
}

#[test]
fn deployment_requires_state_store_for_every_deployed_server() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    plan.profiles[0].object_stores[0]
        .servers
        .insert(ServerSlug::new("simulation").unwrap());

    let err = plan
        .validate()
        .expect_err("each deployed server must have explicit state");

    assert!(
        err.to_string()
            .contains("must declare a state store for hosted server `simulation`")
    );

    plan.profiles[0].state_stores.push(StateStoreDeployment {
        id: DeploymentRequirementId::new("simulation-duckdb").unwrap(),
        kind: StateStoreKind::DuckDb,
        owners: BTreeSet::from([StateStoreOwner::Server {
            server: ServerSlug::new("simulation").unwrap(),
        }]),
        endpoint: None,
        durable_volume_required: true,
        encrypted_at_rest_required: false,
        customer_managed_keys_required: false,
    });

    plan.validate()
        .expect("server state-store coverage should validate");
}

#[test]
fn deployment_rejects_state_store_for_undeployed_server() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    plan.profiles[0].state_stores.push(StateStoreDeployment {
        id: DeploymentRequirementId::new("simulation-duckdb").unwrap(),
        kind: StateStoreKind::DuckDb,
        owners: BTreeSet::from([StateStoreOwner::Server {
            server: ServerSlug::new("simulation").unwrap(),
        }]),
        endpoint: None,
        durable_volume_required: true,
        encrypted_at_rest_required: false,
        customer_managed_keys_required: false,
    });

    let err = plan
        .validate()
        .expect_err("undeployed server state store owner must fail");

    assert!(
        err.to_string()
            .contains("state store owner references undeployed hosted server `simulation`")
    );
}

#[test]
fn deployment_requires_network_coverage_for_core_services() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    plan.profiles[0]
        .egress
        .retain(|rule| rule.target_kind != NetworkTargetKind::HostedMcpServer);

    let err = plan
        .validate()
        .expect_err("hosted server egress must be explicit");

    assert!(err.to_string().contains("HostedMcpServer"));
}

#[test]
fn enterprise_deployment_requires_secret_manager_network() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    harden_profile_for_enterprise(&mut plan.profiles[0]);
    plan.profiles[0].kind = DeploymentProfileKind::Enterprise;
    plan.profiles[0]
        .egress
        .retain(|rule| rule.target_kind != NetworkTargetKind::SecretManager);

    let err = plan
        .validate()
        .expect_err("secret manager egress must be explicit");

    assert!(err.to_string().contains("SecretManager"));
}

#[test]
fn deployment_rejects_plaintext_external_provider_network() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    plan.profiles[0].egress.push(NetworkBoundaryRule {
        id: DeploymentRequirementId::new("provider").unwrap(),
        target_kind: NetworkTargetKind::ExternalProviderApi,
        target: NetworkTarget::new("media-provider-api").unwrap(),
        ports: BTreeSet::from([443]),
        tls_required: false,
    });

    let err = plan
        .validate()
        .expect_err("external provider egress must require TLS");

    assert!(err.to_string().contains("external provider"));
}

#[test]
fn regulated_deployment_requires_controls() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    harden_profile_for_enterprise(&mut plan.profiles[0]);
    plan.profiles[0].kind = DeploymentProfileKind::Regulated;
    plan.profiles[0].regulated_controls = None;

    let err = plan.validate().expect_err("regulated controls must fail");

    assert!(err.to_string().contains("must declare regulated controls"));
}

#[test]
fn enterprise_deployment_rejects_plaintext_service_transport() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    harden_profile_for_enterprise(&mut plan.profiles[0]);
    plan.profiles[0].kind = DeploymentProfileKind::Enterprise;
    plan.profiles[0].service_to_service.transport =
        ServiceToServiceTransport::PrivateNetworkPlaintext;

    let err = plan
        .validate()
        .expect_err("enterprise service transport must fail");

    assert!(err.to_string().contains("requires mTLS"));
}

#[test]
fn enterprise_deployment_requires_hardened_state_store() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    harden_profile_for_enterprise(&mut plan.profiles[0]);
    plan.profiles[0].kind = DeploymentProfileKind::Enterprise;
    plan.profiles[0].state_stores[0].encrypted_at_rest_required = false;
    plan.profiles[0].state_stores[0].customer_managed_keys_required = false;

    let err = plan
        .validate()
        .expect_err("enterprise state store encryption must fail");

    assert!(err.to_string().contains("must require encryption at rest"));
}

#[test]
fn enterprise_deployment_requires_identity_and_authorization_services() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    harden_profile_for_enterprise(&mut plan.profiles[0]);
    plan.profiles[0].kind = DeploymentProfileKind::Enterprise;
    plan.profiles[0]
        .required_services
        .remove(&DeploymentServiceKind::AuthorizationServer);

    let err = plan
        .validate()
        .expect_err("enterprise auth service boundary must fail");

    assert!(
        err.to_string()
            .contains("must declare required service `AuthorizationServer`")
    );
}

#[test]
fn regulated_deployment_requires_us_person_gating() {
    let mut plan: SelfHostedDeploymentPlan =
        serde_json::from_str(valid_deployment_plan_json()).expect("valid json");
    harden_profile_for_enterprise(&mut plan.profiles[0]);
    plan.profiles[0].kind = DeploymentProfileKind::Regulated;
    plan.profiles[0].regulated_controls = Some(RegulatedDataControls {
        allowed_labels: BTreeSet::from([DataLabelId::new("cui").unwrap()]),
        require_us_person: false,
        require_private_network: true,
        require_customer_managed_keys: true,
        require_audit_export: true,
    });

    let err = plan
        .validate()
        .expect_err("regulated US-person gating must fail");

    assert!(err.to_string().contains("require US-person gating"));
}

fn harden_profile_for_enterprise(profile: &mut SelfHostedDeploymentProfile) {
    profile.required_services.extend([
        DeploymentServiceKind::SecretManager,
        DeploymentServiceKind::IdentityProvider,
        DeploymentServiceKind::AuthorizationServer,
    ]);
    profile.secret_sources = BTreeSet::from([SecretSource::Vault]);
    profile.service_to_service.transport = ServiceToServiceTransport::MutualTls;
    for object_store in &mut profile.object_stores {
        object_store.server_side_encryption_required = true;
        object_store.customer_managed_keys_required = true;
    }
    for state_store in &mut profile.state_stores {
        state_store.durable_volume_required = true;
        state_store.encrypted_at_rest_required = true;
        state_store.customer_managed_keys_required = true;
    }
    for sink in &mut profile.telemetry_sinks {
        sink.signals.insert(TelemetrySignal::AuditEvents);
    }
    for rule in profile.ingress.iter_mut().chain(&mut profile.egress) {
        rule.tls_required = true;
    }
    profile.egress.extend([
        NetworkBoundaryRule {
            id: DeploymentRequirementId::new("secret-manager").unwrap(),
            target_kind: NetworkTargetKind::SecretManager,
            target: NetworkTarget::new("enterprise-vault").unwrap(),
            ports: BTreeSet::from([443]),
            tls_required: true,
        },
        NetworkBoundaryRule {
            id: DeploymentRequirementId::new("identity-provider").unwrap(),
            target_kind: NetworkTargetKind::IdentityProvider,
            target: NetworkTarget::new("enterprise-idp").unwrap(),
            ports: BTreeSet::from([443]),
            tls_required: true,
        },
    ]);
}

fn valid_deployment_plan_json() -> &'static str {
    r#"{
      "profiles": [
        {
          "id": "local",
          "kind": "local",
          "required_services": [
            "gateway",
            "hosted_mcp_server",
            "object_store",
            "state_store",
            "telemetry_collector",
            "tunnel_or_ingress"
          ],
          "service_to_service": {
            "gateway_identity": "gateway_signed_jwt",
            "transport": "private_network_plaintext"
          },
          "secret_sources": ["env"],
          "object_stores": [
            {
              "id": "rustfs",
              "kind": "s3_compatible",
              "servers": ["media"],
              "endpoint": "http://rustfs:9000",
              "server_side_encryption_required": false,
              "customer_managed_keys_required": false
            }
          ],
          "state_stores": [
            {
              "id": "gateway-duckdb",
              "kind": "duckdb",
              "owners": [
                {
                  "kind": "gateway"
                }
              ],
              "durable_volume_required": true,
              "encrypted_at_rest_required": false,
              "customer_managed_keys_required": false
            },
            {
              "id": "media-duckdb",
              "kind": "duckdb",
              "owners": [
                {
                  "kind": "server",
                  "server": "media"
                }
              ],
              "durable_volume_required": true,
              "encrypted_at_rest_required": false,
              "customer_managed_keys_required": false
            }
          ],
          "telemetry_sinks": [
            {
              "id": "otel",
              "kind": "open_telemetry_collector",
              "endpoint": "http://otel-collector:4318",
              "signals": ["logs", "traces", "audit_events"]
            }
          ],
          "ingress": [
            {
              "id": "gateway",
              "target_kind": "gateway",
              "target": "mcp-gateway",
              "ports": [443],
              "tls_required": true
            }
          ],
          "egress": [
            {
              "id": "hosted-media-mcp",
              "target_kind": "hosted_mcp_server",
              "target": "media-mcp",
              "ports": [8787],
              "tls_required": false
            },
            {
              "id": "object-store",
              "target_kind": "object_store",
              "target": "rustfs",
              "ports": [9000],
              "tls_required": false
            },
            {
              "id": "telemetry",
              "target_kind": "telemetry_collector",
              "target": "otel-collector",
              "ports": [4318],
              "tls_required": false
            }
          ],
          "retention": {
            "task_metadata_days": 30,
            "artifact_metadata_days": 30,
            "artifact_bytes_days": 30,
            "usage_analytics_days": 365,
            "audit_event_days": 365
          }
        }
      ]
    }"#
}
