use std::{collections::BTreeSet, fs, process::Command};

use tempfile::tempdir;
use veoveo_mcp_contract::{
    ArtifactAudience, CompletionExposure, Exposure, GatewayBinding, GatewayBindingSchema,
    GatewayCompositionError, GatewayControlPlane, GatewayControlPlaneError,
    GatewayProfileServerBinding, GatewayServerFragment, GatewayServerFragmentSchema, MountPath,
    PlatformCapabilityId, ResourceScheme, ResourceSelector, ServerSlug, TaskExposure,
    UpstreamTransportSecurity, UpstreamUrl, compose_gateway_control_plane,
};

fn base() -> GatewayControlPlane {
    serde_json::from_str(include_str!("../../../configs/gateway.smoke.json"))
        .expect("gateway smoke control plane")
}

fn fragment() -> GatewayServerFragment {
    let mut server = base().servers.remove(0);
    server.slug = ServerSlug::new("anonymous").expect("server slug");
    server.uri_scheme = ResourceScheme::new("anonymous").expect("resource scheme");
    server.mount_path = MountPath::new("/anonymous").expect("mount path");
    server.mcp_path = MountPath::new("/anonymous/mcp").expect("MCP path");
    server.upstream.url =
        UpstreamUrl::new("http://anonymous-mcp:8811/anonymous/mcp").expect("upstream URL");
    server.upstream.security = UpstreamTransportSecurity::ClusterInternalHttp;
    server.compatibility_helpers.clear();
    server.owned_routes.clear();
    GatewayServerFragment {
        schema_version: GatewayServerFragmentSchema::V1,
        server,
        required_platform_capabilities: BTreeSet::from([
            PlatformCapabilityId::new("artifact").expect("capability"),
            PlatformCapabilityId::new("frames").expect("capability"),
            PlatformCapabilityId::new("recording").expect("capability"),
        ]),
        required_artifact_audiences: BTreeSet::from([
            ArtifactAudience::new("anonymous").expect("audience")
        ]),
        recording_producer_required: false,
        metadata: serde_json::json!({}),
    }
}

fn binding() -> GatewayBinding {
    GatewayBinding {
        schema_version: GatewayBindingSchema::V1,
        server: ServerSlug::new("anonymous").expect("server"),
        profiles: vec![GatewayProfileServerBinding {
            profile: veoveo_mcp_contract::GatewayProfileId::new("operator").expect("profile"),
            tools: Exposure::All,
            resources: Exposure::Listed(vec![ResourceSelector::Scheme {
                scheme: ResourceScheme::new("anonymous").expect("resource scheme"),
            }]),
            prompts: Exposure::All,
            completions: CompletionExposure::Enabled,
            tasks: TaskExposure::Enabled,
        }],
        policy_rules: vec![],
        allowed_artifact_audiences: BTreeSet::from([
            ArtifactAudience::new("anonymous").expect("audience")
        ]),
        recording_producers: vec![],
        metadata: serde_json::json!({}),
    }
}

#[test]
fn composition_is_order_independent_and_preserves_requirements() {
    let first =
        compose_gateway_control_plane(base(), vec![fragment()], vec![binding()]).expect("compose");
    let second =
        compose_gateway_control_plane(base(), vec![fragment()], vec![binding()]).expect("compose");
    assert_eq!(first, second);
    assert!(
        first
            .control_plane
            .servers
            .iter()
            .any(|server| server.slug.as_str() == "anonymous")
    );
    assert_eq!(first.requirements.platform_capabilities.len(), 3);
    assert_eq!(first.requirements.artifact_audiences.len(), 1);
}

#[test]
fn extension_capability_does_not_grant_its_required_artifact_audience() {
    let mut denied = binding();
    denied.allowed_artifact_audiences.clear();
    assert!(matches!(
        compose_gateway_control_plane(base(), vec![fragment()], vec![denied]),
        Err(GatewayCompositionError::ArtifactAudienceNotAllowed { .. })
    ));
}

#[test]
fn canonical_validator_rejects_route_collisions() {
    let mut control_plane = base();
    let mut duplicate = control_plane.servers[0].clone();
    duplicate.slug = ServerSlug::new("duplicate").expect("slug");
    duplicate.uri_scheme = ResourceScheme::new("duplicate").expect("scheme");
    control_plane.servers.push(duplicate);
    assert!(matches!(
        control_plane.validate(),
        Err(GatewayControlPlaneError::DuplicateMountPath { .. })
    ));
}

#[test]
fn canonical_validator_rejects_mcp_and_owned_route_collisions() {
    let mut mcp_collision = base();
    let mut duplicate = mcp_collision.servers[0].clone();
    duplicate.slug = ServerSlug::new("duplicate").expect("slug");
    duplicate.uri_scheme = ResourceScheme::new("duplicate").expect("scheme");
    duplicate.mount_path = MountPath::new("/duplicate").expect("mount");
    duplicate.owned_routes.clear();
    mcp_collision.servers.push(duplicate);
    assert!(matches!(
        mcp_collision.validate(),
        Err(GatewayControlPlaneError::DuplicateMcpPath { .. })
    ));

    let mut route_collision = base();
    let mut duplicate = route_collision.servers[0].clone();
    duplicate.slug = ServerSlug::new("duplicate").expect("slug");
    duplicate.uri_scheme = ResourceScheme::new("duplicate").expect("scheme");
    duplicate.mount_path = MountPath::new("/duplicate").expect("mount");
    duplicate.mcp_path = MountPath::new("/duplicate/mcp").expect("MCP path");
    duplicate.owned_routes.truncate(1);
    route_collision.servers.push(duplicate);
    assert!(matches!(
        route_collision.validate(),
        Err(GatewayControlPlaneError::DuplicateGatewayRoute { .. })
    ));
}

#[test]
fn standalone_command_emits_stable_path_free_provenance() {
    let workspace = tempdir().expect("temporary workspace");
    let base_path = workspace.path().join("base.json");
    let fragment_path = workspace.path().join("fragment.json");
    let binding_path = workspace.path().join("binding.json");
    let output_path = workspace.path().join("control-plane.json");
    let requirements_path = workspace.path().join("requirements.json");
    let provenance_path = workspace.path().join("provenance.json");
    fs::write(
        &base_path,
        serde_json::to_vec_pretty(&base()).expect("base JSON"),
    )
    .expect("write base");
    fs::write(
        &fragment_path,
        serde_json::to_vec_pretty(&fragment()).expect("fragment JSON"),
    )
    .expect("write fragment");
    fs::write(
        &binding_path,
        serde_json::to_vec_pretty(&binding()).expect("binding JSON"),
    )
    .expect("write binding");

    let run = || {
        Command::new(env!("CARGO_BIN_EXE_gateway-compose"))
            .args([
                "--base",
                base_path.to_str().expect("base path"),
                "--fragment",
                fragment_path.to_str().expect("fragment path"),
                "--binding",
                binding_path.to_str().expect("binding path"),
                "--output",
                output_path.to_str().expect("output path"),
                "--requirements",
                requirements_path.to_str().expect("requirements path"),
                "--provenance",
                provenance_path.to_str().expect("provenance path"),
            ])
            .status()
            .expect("run composer")
    };
    assert!(run().success());
    let first = fs::read(&provenance_path).expect("first provenance");
    assert!(run().success());
    assert_eq!(
        first,
        fs::read(&provenance_path).expect("second provenance")
    );
    let provenance = String::from_utf8(first).expect("UTF-8 provenance");
    assert!(!provenance.contains(workspace.path().to_str().expect("workspace path")));
    assert!(provenance.contains("\"identity\": \"anonymous\""));
    let output: GatewayControlPlane =
        serde_json::from_slice(&fs::read(output_path).expect("control plane")).expect("decode");
    output.validate().expect("composed control plane");
}
