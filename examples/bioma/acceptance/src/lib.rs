//! Owner-local acceptance for the Bioma enterprise composition.

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use serde_json::Value;
    use veoveo_mcp_contract::{GatewayControlPlane, GatewayProfileId, ScopeName};
    use veoveo_mcp_gateway::{GatewayCatalog, www_authenticate_challenge};

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .and_then(|path| path.parent())
            .expect("Bioma acceptance crate lives under <repository>/examples/bioma")
            .to_owned()
    }

    fn load(path: &str) -> Value {
        serde_json::from_slice(&fs::read(repository_root().join(path)).expect("read control plane"))
            .expect("parse control plane")
    }

    fn normalize_bioma(value: &mut Value) {
        match value {
            Value::Array(values) => values.iter_mut().for_each(normalize_bioma),
            Value::Object(values) => values.values_mut().for_each(normalize_bioma),
            Value::String(value) if value == "bioma" => *value = "enterprise".to_owned(),
            Value::String(value) => {
                *value = value.replace("https://veoveo.bioma.ai", "https://veoveo.example");
            }
            _ => {}
        }
    }

    fn canonical_client_shape(control_plane: &Value) -> Value {
        let mut clients = control_plane["oauth_clients"].clone();
        for client in clients.as_array_mut().expect("oauth_clients is an array") {
            let client = client.as_object_mut().expect("OAuth client is an object");
            client.remove("tenant");
            client.remove("jwks");
            client.remove("redirect_uris");
        }
        clients
    }

    #[test]
    fn gateway_control_plane_is_valid_and_preserves_the_canonical_surface() {
        let local = load("configs/gateway.local.json");
        let mut bioma = load("examples/bioma/gateway.json");
        serde_json::from_value::<GatewayControlPlane>(bioma.clone())
            .expect("Bioma control plane contract")
            .validate()
            .expect("valid Bioma control plane");

        let identity = &bioma["identity_providers"][0];
        let tenant_id = "e0ee3c6a-4f58-4f66-8de4-253226eeed5f";
        assert_eq!(identity["claim_mapping"]["subject"], "oid");
        assert_eq!(identity["claim_mapping"]["tenant"]["claim"], "tid");
        assert_eq!(
            identity["claim_mapping"]["tenant"]["values"][tenant_id],
            "bioma"
        );
        assert_eq!(
            identity["issuer"],
            format!("https://login.microsoftonline.com/{tenant_id}/v2.0")
        );
        assert_eq!(
            bioma["oidc_clients"][0]["redirect_uri"],
            "https://veoveo.bioma.ai/oauth/callback"
        );

        normalize_bioma(&mut bioma);
        for key in [
            "servers",
            "profiles",
            "recording_ingest_resources",
            "policies",
            "data_labels",
            "secrets",
        ] {
            assert_eq!(
                bioma[key], local[key],
                "Bioma `{key}` drifted from the canonical platform surface"
            );
        }
        assert_eq!(
            canonical_client_shape(&bioma),
            canonical_client_shape(&local),
            "Bioma OAuth client capabilities drifted from the canonical platform surface"
        );
        assert_eq!(
            bioma["metadata"]["environment"],
            local["metadata"]["environment"]
        );
    }

    #[test]
    fn uav_registration_preserves_cross_server_resource_identities() {
        let control_plane = load("examples/bioma/gateway.json");
        let uav = control_plane["servers"]
            .as_array()
            .expect("servers is an array")
            .iter()
            .find(|server| server["slug"] == "uav-sim")
            .expect("Bioma registers the UAV server");
        assert_eq!(uav["resource_projection"], "server_owned");
        assert_eq!(
            uav["referenced_resource_schemes"],
            serde_json::json!(["frames", "map", "recording"])
        );
    }

    #[test]
    fn operator_profiles_expose_view_and_its_complete_scope_bundle() {
        let path = repository_root().join("examples/bioma/gateway.json");
        let catalog = GatewayCatalog::load_json(&path).expect("load Bioma control plane");
        for profile in ["operator", "admin"] {
            let profile_id = GatewayProfileId::new(profile).expect("profile ID");
            let owner = catalog
                .server_for_resource_uri(&profile_id, "ui://view/preview.html")
                .map(|(_, server)| server.slug.to_string());
            assert_eq!(owner.as_deref(), Some("view"));
        }

        let profile_id = GatewayProfileId::new("operator").expect("operator profile ID");
        let profile = catalog.profile(&profile_id).expect("operator profile");
        let scopes = profile
            .required_scopes
            .iter()
            .map(ScopeName::as_str)
            .collect::<Vec<_>>();
        assert_eq!(
            scopes,
            [
                "operator:use",
                "uav-sim:control",
                "uav-sim:stream",
                "view:read",
                "view:write",
                "view:capture",
                "map:dataset:read",
                "map:route",
                "time:read",
            ]
        );
        assert_eq!(
            www_authenticate_challenge(
                "https://veoveo.bioma.ai/.well-known/oauth-protected-resource/mcp/operator",
                &profile.required_scopes,
            ),
            "Bearer resource_metadata=\"https://veoveo.bioma.ai/.well-known/oauth-protected-resource/mcp/operator\", scope=\"operator:use uav-sim:control uav-sim:stream view:read view:write view:capture map:dataset:read map:route time:read\""
        );
        let metadata = catalog
            .protected_resource_metadata(&profile_id)
            .expect("operator protected-resource metadata");
        assert!(
            metadata
                .scopes_supported
                .iter()
                .any(|scope| scope == "uav-sim:read"),
            "operator protected-resource metadata omits UAV domain read authority"
        );
    }

    #[test]
    fn server_capabilities_use_exact_list_change_flags() {
        let control_plane = load("examples/bioma/gateway.json");
        assert!(!control_plane.to_string().contains("\"notifications\""));
        for server in control_plane["servers"].as_array().expect("servers array") {
            let capabilities = server["capabilities"]
                .as_object()
                .expect("capability object");
            if capabilities
                .get("resources_list_changed")
                .and_then(Value::as_bool)
                == Some(true)
            {
                assert_eq!(
                    capabilities.get("resources").and_then(Value::as_bool),
                    Some(true),
                    "{} claims resource list changes without resources",
                    server["slug"]
                );
            }
        }
    }
}
