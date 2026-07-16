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
    let source_dir = tmpdir.join("map-sources");
    fs::create_dir_all(&data_dir)?;
    fs::create_dir_all(&source_dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o777))?;
        fs::set_permissions(&source_dir, fs::Permissions::from_mode(0o777))?;
    }

    let repository_fixtures = fs::canonicalize("servers/map-mcp/data/tests/fixtures")?;
    prepare_source_fixtures(map_image, &repository_fixtures, &source_dir)?;

    let plane =
        spawn_artifact_service_smoke(artifact_service, &tmpdir.join("artifact-service.log"))
            .await?;
    let port = reserve_local_port()?;
    let base = format!("http://127.0.0.1:{port}");
    let container_name = format!("veoveo-map-smoke-{}", uuid::Uuid::new_v4());
    let data_mount = format!("{}:/var/lib/veoveo/map", data_dir.display());
    let fixture_mount = format!("{}:/data/map-sources/fixtures:ro", source_dir.display());
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
            "serve".into(),
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

    let admin_token = issue_map_token(&[
        "operator:use",
        "map:admin",
        "map:dataset:read",
        "map:route",
        "map:restriction:publish",
        "map:restriction:withdraw",
    ])?;
    let client = reqwest::Client::new();
    let authority_source = register_source(
        &client,
        &base,
        &admin_token,
        SourceFixture {
            name: "Authoritative smoke fixture",
            adapter_kind: "authority_vector",
            map_families: &["road_street", "intermodal"],
            relative_path: "authority.geojson",
            media_type: "application/geo+json",
            idempotency_key: "map-smoke-source-authority",
            maximum_elapsed_seconds: 60,
        },
    )
    .await?;
    let authority_release = acquire_and_activate(
        &client,
        &base,
        &admin_token,
        &authority_source,
        &source_dir.join("authority.geojson"),
        "map-smoke-acquisition-authority",
    )
    .await?;
    assert_digest_mismatch_fails(&client, &base, &admin_token, &authority_source.source_id).await?;

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

    let osm_source = register_source(
        &client,
        &base,
        &admin_token,
        SourceFixture {
            name: "OpenStreetMap routing smoke fixture",
            adapter_kind: "open_street_map",
            map_families: &["road_street", "active_mobility"],
            relative_path: "routing.osm.pbf",
            media_type: "application/vnd.openstreetmap.data+pbf",
            idempotency_key: "map-smoke-source-osm",
            maximum_elapsed_seconds: 120,
        },
    )
    .await?;
    let osm_release = acquire_and_activate(
        &client,
        &base,
        &admin_token,
        &osm_source,
        &source_dir.join("routing.osm.pbf"),
        "map-smoke-acquisition-osm",
    )
    .await?;
    let road_route_id =
        assert_road_route_workflow(conformance, &mcp_url, &client, &base, &admin_token).await?;

    let network_source = register_source(
        &client,
        &base,
        &admin_token,
        SourceFixture {
            name: "Governed network smoke fixture",
            adapter_kind: "authority_vector",
            map_families: &["maritime"],
            relative_path: "network.geojson",
            media_type: "application/geo+json",
            idempotency_key: "map-smoke-source-network",
            maximum_elapsed_seconds: 60,
        },
    )
    .await?;
    let network_release = acquire_and_activate(
        &client,
        &base,
        &admin_token,
        &network_source,
        &source_dir.join("network.geojson"),
        "map-smoke-acquisition-network",
    )
    .await?;
    let maritime_route_id =
        assert_governed_graph_workflow(conformance, &mcp_url, &client, &base, &admin_token).await?;

    cleanup.remove_on_drop();
    println!(
        "Map MCP smoke ok: activated authority {}, OSM {}, and network {}; planned road {} and governed maritime {}",
        authority_release.release_id,
        osm_release.release_id,
        network_release.release_id,
        road_route_id,
        maritime_route_id,
    );
    Ok(())
}

#[derive(Clone, Copy)]
struct SourceFixture<'a> {
    name: &'a str,
    adapter_kind: &'a str,
    map_families: &'a [&'a str],
    relative_path: &'a str,
    media_type: &'a str,
    idempotency_key: &'a str,
    maximum_elapsed_seconds: u64,
}

struct RegisteredFixture {
    source_id: String,
}

struct ActivatedFixture {
    release_id: String,
}

fn prepare_source_fixtures(
    map_image: &str,
    repository_fixtures: &Path,
    output: &Path,
) -> Result<()> {
    for name in ["authority.geojson", "network.geojson"] {
        fs::copy(repository_fixtures.join(name), output.join(name))?;
    }
    let input_mount = format!("{}:/fixtures:ro", repository_fixtures.display());
    let output_mount = format!("{}:/output", output.display());
    run_checked(
        Path::new("docker"),
        [
            "run".into(),
            "--rm".into(),
            "--user".into(),
            "10001:10001".into(),
            "--entrypoint".into(),
            "/usr/bin/osmium".into(),
            "-v".into(),
            input_mount.into(),
            "-v".into(),
            output_mount.into(),
            map_image.into(),
            "cat".into(),
            "/fixtures/routing.osm".into(),
            "-o".into(),
            "/output/routing.osm.pbf".into(),
        ],
        [],
    )?;
    if !output.join("routing.osm.pbf").is_file() {
        bail!("osmium did not create the routing fixture");
    }
    Ok(())
}

async fn register_source(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    fixture: SourceFixture<'_>,
) -> Result<RegisteredFixture> {
    let source_id = format!("source-{}", uuid::Uuid::now_v7());
    let dataset_id = format!("dataset-{}", uuid::Uuid::now_v7());
    let now = Utc::now();
    let response: Value = client
        .post(format!("{base}/map/admin/sources"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "source": {
                "source_id": source_id,
                "dataset_id": dataset_id,
                "name": fixture.name,
                "adapter_kind": fixture.adapter_kind,
                "authority": "synthetic_test",
                "acquisition_model": "snapshot",
                "map_families": fixture.map_families,
                "location": {
                    "kind": "mounted_exchange_set",
                    "mount_id": "fixtures",
                    "relative_path": fixture.relative_path
                },
                "publisher_key_refs": [],
                "expected_media_types": [fixture.media_type],
                "maximum_download_bytes": 67_108_864,
                "maximum_elapsed_seconds": fixture.maximum_elapsed_seconds,
                "license": {
                    "license_id": format!("{}-smoke", fixture.adapter_kind),
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
            },
            "idempotency_key": fixture.idempotency_key
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if response.get("source_id").and_then(Value::as_str) != Some(source_id.as_str()) {
        bail!("Map source response had the wrong identity: {response}");
    }
    Ok(RegisteredFixture { source_id })
}

async fn start_acquisition(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    source_id: &str,
    expected_digest: &str,
    idempotency_key: &str,
) -> Result<String> {
    let acquisition: Value = client
        .post(format!("{base}/map/admin/acquisitions"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "source_id": source_id,
            "requested_coverage": {
                "west": -89.23,
                "south": 13.67,
                "east": -89.20,
                "north": 13.71
            },
            "expected_source_digest_sha256": expected_digest,
            "idempotency_key": idempotency_key
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    acquisition
        .get("acquisition_id")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .context("Map acquisition response omitted acquisition_id")
}

async fn acquire_and_activate(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    source: &RegisteredFixture,
    source_path: &Path,
    idempotency_key: &str,
) -> Result<ActivatedFixture> {
    let digest = hex::encode(Sha256::digest(fs::read(source_path)?));
    let acquisition_id = start_acquisition(
        client,
        base,
        token,
        &source.source_id,
        &digest,
        idempotency_key,
    )
    .await?;
    let completed = wait_for_acquisition(client, base, token, &acquisition_id).await?;
    let release_id = completed
        .get("staged_release_id")
        .and_then(Value::as_str)
        .context("successful Map acquisition omitted staged_release_id")?
        .to_owned();
    let release: Value = client
        .get(format!("{base}/map/admin/releases/{release_id}"))
        .bearer_auth(token)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if release.get("source_digest_sha256").and_then(Value::as_str) != Some(digest.as_str()) {
        bail!("staged release did not retain the verified source digest: {release}");
    }
    let release_version = release
        .get("record_version")
        .and_then(Value::as_u64)
        .context("staged release omitted record_version")?;
    let activated: Value = client
        .post(format!("{base}/map/admin/releases/{release_id}/activate"))
        .bearer_auth(token)
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
    Ok(ActivatedFixture { release_id })
}

async fn assert_digest_mismatch_fails(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    source_id: &str,
) -> Result<()> {
    let acquisition_id = start_acquisition(
        client,
        base,
        token,
        source_id,
        &"0".repeat(64),
        "map-smoke-acquisition-bad-digest",
    )
    .await?;
    let failed = wait_for_acquisition_terminal(client, base, token, &acquisition_id).await?;
    if failed.get("status").and_then(Value::as_str) != Some("failed")
        || failed
            .get("staged_release_id")
            .is_some_and(|value| !value.is_null())
    {
        bail!("digest mismatch did not fail closed before staging: {failed}");
    }
    Ok(())
}

async fn create_profile(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    profile: Value,
    idempotency_key: &str,
) -> Result<String> {
    let expected_id = profile
        .pointer("/profile/metadata/profile_id")
        .and_then(Value::as_str)
        .context("test mobility profile omitted profile_id")?;
    let created: Value = client
        .post(format!("{base}/map/admin/mobility-profiles"))
        .bearer_auth(token)
        .json(&serde_json::json!({
            "profile": profile,
            "idempotency_key": idempotency_key
        }))
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    if created
        .pointer("/profile/metadata/profile_id")
        .and_then(Value::as_str)
        != Some(expected_id)
    {
        bail!("created mobility profile had the wrong identity: {created}");
    }
    Ok(expected_id.to_owned())
}

async fn assert_road_route_workflow(
    conformance: &Path,
    mcp_url: &str,
    client: &reqwest::Client,
    base: &str,
    token: &str,
) -> Result<String> {
    let profile_id = format!("mobility-{}", uuid::Uuid::now_v7());
    let profile = serde_json::json!({
        "family": "road_vehicle",
        "profile": {
            "metadata": {
                "profile_id": profile_id,
                "name": "smoke passenger car",
                "version": 1,
                "valid_from": Utc::now() - TimeDelta::minutes(5),
                "labels": []
            },
            "class": "passenger_car",
            "dimensions": {"length": 4.5, "width": 1.8, "height": 1.6},
            "gross_mass": 1800.0,
            "performance": {
                "maximum_speed": 35.0,
                "nominal_speed": 12.0,
                "maximum_range": 500000.0,
                "payload_capacity": 500.0
            },
            "energy": {
                "source": "battery",
                "battery_capacity": 80.0,
                "minimum_reserve": 0.1
            },
            "axle_count": 2,
            "maximum_axle_load": 1200.0,
            "minimum_turning_radius": 5.5,
            "hazardous_cargo": false,
            "unpaved_allowed": false
        }
    });
    create_profile(client, base, token, profile, "map-smoke-profile-road").await?;
    let request = serde_json::json!({
        "mobility_profile_id": profile_id,
        "mobility_profile_version": 1,
        "origin": {
            "kind": "position",
            "position": {"longitude_deg": -89.2190, "latitude_deg": 13.6920}
        },
        "destination": {
            "kind": "position",
            "position": {"longitude_deg": -89.2080, "latitude_deg": 13.6980}
        },
        "departure_time": Utc::now(),
        "objective": {"kind": "fastest"},
        "constraints": {},
        "alternatives": 0,
        "data_policy": {
            "allow_planning_advisory": true,
            "required_map_families": ["road_street"]
        }
    });
    let output = call_map_task(conformance, mcp_url, "route", &request)?;
    let route = structured_output(&output)?;
    if route.pointer("/legs/0/map_family").and_then(Value::as_str) != Some("road_street")
        || route
            .pointer("/summary/distance")
            .and_then(Value::as_f64)
            .is_none_or(|distance| distance <= 0.0)
    {
        bail!("Valhalla route did not contain a real road leg and cost: {route}");
    }
    let route_id = route
        .get("route_id")
        .and_then(Value::as_str)
        .context("Valhalla route omitted route_id")?
        .to_owned();
    let validation = call_map_tool(
        conformance,
        mcp_url,
        "validate_route",
        &serde_json::json!({"route": route}),
    )?;
    if structured_output(&validation)?
        .get("valid")
        .and_then(Value::as_bool)
        != Some(true)
    {
        bail!("persisted Valhalla route did not validate: {validation}");
    }
    let resource = run_map_mcp(
        conformance,
        mcp_url,
        ["resource".into(), format!("map://route/{route_id}").into()],
    )?;
    contains(&resource, &route_id)?;
    Ok(route_id)
}

async fn assert_governed_graph_workflow(
    conformance: &Path,
    mcp_url: &str,
    client: &reqwest::Client,
    base: &str,
    token: &str,
) -> Result<String> {
    let profile_id = format!("mobility-{}", uuid::Uuid::now_v7());
    let profile = serde_json::json!({
        "family": "surface_vessel",
        "profile": {
            "metadata": {
                "profile_id": profile_id,
                "name": "smoke workboat",
                "version": 1,
                "valid_from": Utc::now() - TimeDelta::minutes(5),
                "labels": []
            },
            "class": "tug_workboat",
            "dimensions": {"length": 18.0, "width": 6.0, "height": 8.0},
            "displacement": 90000.0,
            "performance": {
                "maximum_speed": 12.0,
                "nominal_speed": 5.0,
                "maximum_range": 500000.0,
                "payload_capacity": 20000.0
            },
            "energy": {
                "source": "diesel",
                "liquid_fuel_capacity": 10000.0,
                "minimum_reserve": 0.2
            },
            "draft": 2.0,
            "air_draft": 7.0,
            "minimum_under_keel_clearance": 1.0,
            "minimum_turning_radius": 30.0,
            "berth_requirements": []
        }
    });
    create_profile(client, base, token, profile, "map-smoke-profile-vessel").await?;

    let restriction_id = format!("restriction-{}", uuid::Uuid::now_v7());
    let restriction = serde_json::json!({
        "restriction": {
            "restriction_id": restriction_id,
            "kind": "navigational_warning",
            "geometry": {
                "exterior": [
                    {"longitude_deg": -89.2200, "latitude_deg": 13.6790},
                    {"longitude_deg": -89.2120, "latitude_deg": 13.6790},
                    {"longitude_deg": -89.2120, "latitude_deg": 13.6860},
                    {"longitude_deg": -89.2200, "latitude_deg": 13.6860},
                    {"longitude_deg": -89.2200, "latitude_deg": 13.6790}
                ],
                "interiors": []
            },
            "affected_mobility_families": ["surface_vessel"],
            "effect": {
                "kind": "penalize",
                "explanation": "smoke risk area"
            },
            "valid_from": Utc::now() - TimeDelta::minutes(5),
            "authority": "synthetic_test",
            "issued_at": Utc::now() - TimeDelta::minutes(5),
            "record_version": 1
        }
    });
    let published = call_map_tool(conformance, mcp_url, "publish_restriction", &restriction)?;
    contains(&published, &restriction_id)?;

    let request = serde_json::json!({
        "mobility_profile_id": profile_id,
        "mobility_profile_version": 1,
        "origin": {
            "kind": "position",
            "position": {"longitude_deg": -89.2190, "latitude_deg": 13.6800}
        },
        "destination": {
            "kind": "position",
            "position": {"longitude_deg": -89.2080, "latitude_deg": 13.6880}
        },
        "departure_time": Utc::now(),
        "objective": {"kind": "fastest"},
        "constraints": {},
        "alternatives": 0,
        "data_policy": {
            "allow_planning_advisory": true,
            "required_map_families": ["maritime"]
        }
    });
    let output = call_map_task(conformance, mcp_url, "route", &request)?;
    let route = structured_output(&output)?;
    if route.pointer("/legs/0/map_family").and_then(Value::as_str) != Some("maritime")
        || route.pointer("/summary/distance").and_then(Value::as_f64) != Some(2700.0)
        || route.pointer("/summary/risk").and_then(Value::as_f64) != Some(0.1)
    {
        bail!("governed graph route did not preserve network costs and risk: {route}");
    }
    let route_id = route
        .get("route_id")
        .and_then(Value::as_str)
        .context("governed graph route omitted route_id")?
        .to_owned();
    if !route.to_string().contains(&restriction_id) {
        bail!("governed graph route did not pin its effective restriction: {route}");
    }

    let cancellation_id = format!("restriction-{}", uuid::Uuid::now_v7());
    let withdrawn = call_map_tool(
        conformance,
        mcp_url,
        "withdraw_restriction",
        &serde_json::json!({
            "restriction_id": restriction_id,
            "expected_record_version": 1,
            "effective_at": Utc::now(),
            "cancellation_restriction_id": cancellation_id
        }),
    )?;
    if structured_output(&withdrawn)?
        .get("invalidated_route_count")
        .and_then(Value::as_u64)
        != Some(1)
    {
        bail!("restriction withdrawal did not invalidate its dependent route: {withdrawn}");
    }
    let resource = run_map_mcp(
        conformance,
        mcp_url,
        ["resource".into(), format!("map://route/{route_id}").into()],
    )?;
    contains(&resource, r#""status": "invalidated""#)?;
    Ok(route_id)
}

fn call_map_tool(
    conformance: &Path,
    mcp_url: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<String> {
    run_map_mcp(
        conformance,
        mcp_url,
        [
            "call".into(),
            "--tool-name".into(),
            tool_name.into(),
            "--arguments".into(),
            serde_json::to_string(arguments)?.into(),
        ],
    )
}

fn call_map_task(
    conformance: &Path,
    mcp_url: &str,
    tool_name: &str,
    arguments: &Value,
) -> Result<String> {
    let output = run_map_mcp(
        conformance,
        mcp_url,
        [
            "task-call".into(),
            "--tool-name".into(),
            tool_name.into(),
            "--arguments".into(),
            serde_json::to_string(arguments)?.into(),
        ],
    )?;
    contains(&output, " created (status ")?;
    contains(&output, "poll: Completed")?;
    Ok(output)
}

fn structured_output(output: &str) -> Result<Value> {
    let json = output
        .lines()
        .find_map(|line| line.strip_prefix("structured: "))
        .context("MCP call did not return structured content")?;
    serde_json::from_str(json).context("decoding MCP structured content")
}

async fn wait_for_acquisition(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    acquisition_id: &str,
) -> Result<Value> {
    let value = wait_for_acquisition_terminal(client, base, token, acquisition_id).await?;
    if value.get("status").and_then(Value::as_str) != Some("succeeded") {
        bail!("Map acquisition reached a terminal failure: {value}");
    }
    Ok(value)
}

async fn wait_for_acquisition_terminal(
    client: &reqwest::Client,
    base: &str,
    token: &str,
    acquisition_id: &str,
) -> Result<Value> {
    for _ in 0..600 {
        let value: Value = client
            .get(format!("{base}/map/admin/acquisitions/{acquisition_id}"))
            .bearer_auth(token)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        match value.get("status").and_then(Value::as_str) {
            Some("succeeded" | "failed" | "cancelled") => return Ok(value),
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
        "--internal-scope".into(),
        "map:route".into(),
        "--internal-scope".into(),
        "map:restriction:publish".into(),
        "--internal-scope".into(),
        "map:restriction:withdraw".into(),
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
