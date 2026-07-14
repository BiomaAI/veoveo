use std::{collections::BTreeSet, ffi::OsString, path::Path};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{TimeDelta, Utc};
use reqwest::header::HOST;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    GATEWAY_INTERNAL_TOKEN_ISSUER, GatewayInternalSigningKey, GatewayInternalTokenIssuer,
    GatewayProfileId, Principal, PrincipalId, PrincipalKind, ScopeName, ServerSlug, TenantId,
    TokenIssuer, TokenSubject,
};

use super::*;

pub(crate) async fn map_mcp(
    conformance: &Path,
    artifact_service: &Path,
    map_image: &str,
) -> Result<()> {
    assert_executable(conformance)?;
    assert_executable(artifact_service)?;
    run_checked(
        Path::new("docker"),
        ["image".into(), "inspect".into(), map_image.into()],
        [],
    )?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());
    let data_dir = tmpdir.join("map-data");
    fs::create_dir_all(&data_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o777))?;
    }

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let port = reserve_local_port()?;
    let base = format!("http://127.0.0.1:{port}");
    let fixture_dir = fs::canonicalize("servers/map-mcp/data/tests/fixtures")?;
    let fixture = fixture_dir.join("authority.geojson");
    let fixture_digest = hex::encode(Sha256::digest(fs::read(&fixture)?));
    let container_name = format!("veoveo-map-smoke-{}", uuid::Uuid::new_v4());
    let data_mount = format!("{}:/var/lib/veoveo/map", data_dir.display());
    let fixture_mount = format!("{}:/data/map-sources/fixtures:ro", fixture_dir.display());
    let platform = &plane.platform;
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "-d".into(),
            "--name".into(),
            container_name.clone().into(),
            "--network".into(),
            "host".into(),
            "--user".into(),
            "10001:10001".into(),
            "-v".into(),
            data_mount.into(),
            "-v".into(),
            fixture_mount.into(),
            "-e".into(),
            format!("PUBLIC_BASE_URL={base}").into(),
            "-e".into(),
            format!("VEOVEO_SURREAL_ENDPOINT={}", platform.endpoint).into(),
            "-e".into(),
            format!("VEOVEO_SURREAL_NAMESPACE={}", platform.namespace).into(),
            "-e".into(),
            format!("VEOVEO_SURREAL_DATABASE={}", platform.database).into(),
            "-e".into(),
            "VEOVEO_SURREAL_AUTH_LEVEL=database".into(),
            "-e".into(),
            format!("VEOVEO_SURREAL_USERNAME={SURREAL_RUNTIME_USER}").into(),
            "-e".into(),
            format!("VEOVEO_SURREAL_PASSWORD={SURREAL_RUNTIME_PASSWORD}").into(),
            "-e".into(),
            format!("VEOVEO_INTERNAL_TRUST_JWKS={INTERNAL_TRUST_JWKS}").into(),
            map_image.into(),
            "--port".into(),
            port.to_string().into(),
            "--public-base-url".into(),
            base.clone().into(),
            "--allow-loopback-hosts".into(),
            "--artifact-service-url".into(),
            plane.url.clone().into(),
            "--valhalla-startup-timeout-seconds".into(),
            "30".into(),
        ],
        [],
    )?;
    let _container = ContainerGuard::new(&container_name);
    if let Err(error) = wait_for_http(&format!("{base}/map/healthz")).await {
        let logs = run_checked(
            Path::new("docker"),
            ["logs".into(), container_name.clone().into()],
            [],
        )
        .unwrap_or_else(|log_error| format!("could not read container logs: {log_error}"));
        bail!("Map container did not become healthy: {error}\n{logs}");
    }

    let health: Value = reqwest::get(format!("{base}/map/healthz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    if health != serde_json::json!({"routing": true, "spatial": true}) {
        bail!("Map health did not verify routing and Spatial: {health}");
    }
    assert_http_status(&format!("{base}/map/mcp"), StatusCode::UNAUTHORIZED).await?;
    let untrusted_host = reqwest::Client::new()
        .get(format!("{base}/map/healthz"))
        .header(HOST, "evil.example")
        .send()
        .await?
        .status();
    if untrusted_host != StatusCode::MISDIRECTED_REQUEST {
        bail!("Map untrusted Host status was {untrusted_host}, expected 421");
    }

    let mcp_url = format!("{base}/map/mcp");
    let info = run_map_mcp(conformance, &mcp_url, ["info".into()])?;
    for expected in [
        "server: map",
        "tool `route`",
        "tool `route_matrix`",
        "tool `geodesic_inverse`",
        "prompt `prepare_route_request`",
        "template: map://dataset/{dataset_id}",
    ] {
        contains(&info, expected)?;
    }
    assert_direct_mcp_denied(
        conformance,
        &mcp_url,
        [
            "--scheme".into(),
            "map".into(),
            "--internal-server".into(),
            "map".into(),
            "call".into(),
            "--tool-name".into(),
            "geodesic_inverse".into(),
            "--arguments".into(),
            r#"{"start":{"latitude_deg":13.6929,"longitude_deg":-89.2182},"end":{"latitude_deg":13.705,"longitude_deg":-89.19}}"#.into(),
        ],
        [("VEOVEO_INTERNAL_SIGNING_KEY_DER_B64", INTERNAL_SIGNING_KEY_DER_B64.into())],
    )?;
    let geodesic = run_map_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "geodesic_inverse".into(),
            "--arguments".into(),
            r#"{"start":{"latitude_deg":13.6929,"longitude_deg":-89.2182},"end":{"latitude_deg":13.705,"longitude_deg":-89.19}}"#.into(),
        ],
    )?;
    contains(&geodesic, "distance")?;
    contains(&geodesic, "geographiclib-rs:wgs84")?;

    let admin_token =
        issue_map_token(&["operator:use", "map:admin", "map:dataset:read", "map:route"])?;
    let client = reqwest::Client::new();
    let source_id = format!("source-{}", uuid::Uuid::now_v7());
    let dataset_id = format!("dataset-{}", uuid::Uuid::now_v7());
    let now = Utc::now();
    let source = serde_json::json!({
        "source_id": source_id,
        "dataset_id": dataset_id,
        "name": "Authoritative smoke fixture",
        "adapter_kind": "authority_vector",
        "authority": "synthetic_test",
        "acquisition_model": "snapshot",
        "map_families": ["road_street", "intermodal"],
        "location": {
            "kind": "mounted_exchange_set",
            "mount_id": "fixtures",
            "relative_path": "authority.geojson"
        },
        "publisher_key_refs": [],
        "expected_media_types": ["application/geo+json"],
        "maximum_download_bytes": 16_777_216,
        "maximum_elapsed_seconds": 60,
        "license": {
            "license_id": "smoke-fixture",
            "source_terms_uri": "https://example.com/map-fixture-terms",
            "attribution": "Veoveo synthetic test fixture",
            "redistribution_allowed": true,
            "derivatives_allowed": true,
            "offline_bundle_allowed": true
        },
        "enabled": true,
        "record_version": 1,
        "created_at": now,
        "updated_at": now
    });
    let created_source: Value = client
        .post(format!("{base}/map/admin/sources"))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({
            "source": source,
            "idempotency_key": "map-smoke-source-001"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if created_source.get("source_id").and_then(Value::as_str) != Some(source_id.as_str()) {
        bail!("Map source response had the wrong identity: {created_source}");
    }

    let acquisition: Value = client
        .post(format!("{base}/map/admin/acquisitions"))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({
            "source_id": source_id,
            "requested_coverage": {"west": -89.23, "south": 13.68, "east": -89.20, "north": 13.71},
            "expected_source_digest_sha256": fixture_digest,
            "idempotency_key": "map-smoke-acquisition-001"
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let acquisition_id = acquisition
        .get("acquisition_id")
        .and_then(Value::as_str)
        .context("Map acquisition response omitted acquisition_id")?
        .to_owned();
    let completed = wait_for_acquisition(&client, &base, &admin_token, &acquisition_id).await?;
    let release_id = completed
        .get("staged_release_id")
        .and_then(Value::as_str)
        .context("successful Map acquisition omitted staged_release_id")?
        .to_owned();
    let release: Value = client
        .get(format!("{base}/map/admin/releases/{release_id}"))
        .bearer_auth(&admin_token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if release.get("source_digest_sha256").and_then(Value::as_str) != Some(fixture_digest.as_str())
    {
        bail!("staged release did not retain the verified source digest: {release}");
    }
    let release_version = release
        .get("record_version")
        .and_then(Value::as_u64)
        .context("staged release omitted record_version")?;
    let activated: Value = client
        .post(format!("{base}/map/admin/releases/{release_id}/activate"))
        .bearer_auth(&admin_token)
        .json(&serde_json::json!({
            "expected_record_version": release_version,
            "expected_active_pointer_version": 0
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if activated.pointer("/release/state").and_then(Value::as_str) != Some("active") {
        bail!("Map activation did not return an active release: {activated}");
    }

    let search = run_map_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "search_locations".into(),
            "--arguments".into(),
            r#"{"query":"Warehouse","coverage":{"west":-89.23,"south":13.68,"east":-89.20,"north":13.71},"include_facilities":true,"limit":10}"#.into(),
        ],
    )?;
    contains(&search, "Warehouse Alpha")?;
    contains(&search, "facility-")?;
    let corridor = run_map_mcp(
        conformance,
        &mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            "inspect_corridor".into(),
            "--arguments".into(),
            format!(
                r#"{{"corridor":{{"coordinates":[{{"latitude_deg":13.69,"longitude_deg":-89.225}},{{"latitude_deg":13.70,"longitude_deg":-89.205}}]}},"width":1500.0,"departure_time":"{}"}}"#,
                Utc::now().to_rfc3339()
            )
            .into(),
        ],
    )?;
    contains(&corridor, "boundary-")?;
    let facilities = run_map_mcp(
        conformance,
        &mcp_url,
        ["resource".into(), "map://facilities".into()],
    )?;
    contains(&facilities, "Warehouse Alpha")?;

    cleanup.remove_on_drop();
    println!(
        "Map MCP smoke ok: acquired {acquisition_id}, activated {release_id}, and queried its spatial projection"
    );
    Ok(())
}

async fn wait_for_acquisition(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    acquisition_id: &str,
) -> Result<Value> {
    for _ in 0..240 {
        let value: Value = client
            .get(format!("{base}/map/admin/acquisitions/{acquisition_id}"))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        match value.get("status").and_then(Value::as_str) {
            Some("succeeded") => return Ok(value),
            Some("failed" | "cancelled") => {
                bail!("Map acquisition reached a terminal failure: {value}")
            }
            _ => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
    bail!("timed out waiting for Map acquisition {acquisition_id}")
}

fn run_map_mcp(
    conformance: &Path,
    mcp_url: &str,
    args: impl IntoIterator<Item = OsString>,
) -> Result<String> {
    let mut all_args = vec![
        "--scheme".into(),
        "map".into(),
        "--internal-server".into(),
        "map".into(),
        "--internal-scope".into(),
        "operator:use".into(),
        "--internal-scope".into(),
        "map:dataset:read".into(),
    ];
    all_args.extend(args);
    run_direct_mcp(
        conformance,
        mcp_url,
        all_args,
        [(
            "VEOVEO_INTERNAL_SIGNING_KEY_DER_B64",
            INTERNAL_SIGNING_KEY_DER_B64.into(),
        )],
    )
}

fn issue_map_token(scopes: &[&str]) -> Result<String> {
    let private_key = STANDARD.decode(INTERNAL_SIGNING_KEY_DER_B64)?;
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new("veoveo-internal-1", private_key)?,
    );
    let principal_issuer = TokenIssuer::new("https://conformance.veoveo.local")?;
    let subject = TokenSubject::new("conformance")?;
    let principal = Principal {
        id: PrincipalId::new(format!("{principal_issuer}#{subject}"))?,
        kind: PrincipalKind::Service,
        issuer: principal_issuer,
        subject,
        tenant: Some(TenantId::new("local")?),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: scopes
            .iter()
            .map(|scope| ScopeName::new(*scope))
            .collect::<Result<_, _>>()?,
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(Utc::now()),
    };
    Ok(issuer
        .issue(
            GatewayProfileId::new("admin")?,
            ServerSlug::new("map")?,
            principal,
            Utc::now() + TimeDelta::minutes(30),
        )?
        .bearer_token)
}
