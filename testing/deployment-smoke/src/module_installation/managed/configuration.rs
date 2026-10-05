//! Installation-owned model, runtime template and gateway configuration.
use super::super::{Args, fixture::Fixture, process};
use anyhow::{Context, Result, ensure};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt, process::Command};
use veoveo_agent_runtime::contract::authoring::{self as wire, ModelConnection, RuntimeTemplate};
use veoveo_mcp_contract::GatewayControlPlane;

pub(in crate::module_installation) struct Configuration {
    pub namespace: String,
    pub manager_image: super::super::PinnedImage,
    pub model: ModelConnection,
    pub template: RuntimeTemplate,
    pub data: BTreeMap<String, String>,
    pub plane: GatewayControlPlane,
    pub jwks: String,
    pub installation_secrets: BTreeMap<String, String>,
    pub api_egress: Vec<Value>,
}
impl Configuration {
    pub fn create(args: &Args, namespace: &str, directory: &std::path::Path) -> Result<Self> {
        let agent_namespace = format!("{namespace}-agents");
        // No prompt, subscriptions, domain migrations or actionable wake producers.
        let manifest = json!({
            "agent":{"tenant":"${VEOVEO_AGENT_TENANT}","id":"${VEOVEO_AGENT_ID}","display_name":"${VEOVEO_AGENT_NAME}"},
            "model":{"base_url":"${VEOVEO_AGENT_MODEL_URL}","model":"${VEOVEO_AGENT_MODEL_ID}","api_key_env":"VEOVEO_MANAGED_MODEL_KEY"},
            "gateway":{"url":"${VEOVEO_GATEWAY_URL}","transport_url":"${VEOVEO_GATEWAY_TRANSPORT_URL}","profile":"${VEOVEO_AGENT_PROFILE}","client_id":"${VEOVEO_AGENT_CLIENT_ID}","work_context":"${VEOVEO_AGENT_WORK_CONTEXT}","audience":"${VEOVEO_GATEWAY_AUDIENCE}","resource":"${VEOVEO_GATEWAY_RESOURCE}","scopes":["operator:use"],"private_key_env":"VEOVEO_MANAGED_PRIVATE_KEY","private_key_kid":"${VEOVEO_AGENT_KEY_ID}"},
            "episode":{},"schedule":{"heartbeat_interval_s":3600},"resource_subscriptions":[],"preamble":"Credential recovery fixture"
        });
        let data = BTreeMap::from([("manifest.json".into(), serde_json::to_string(&manifest)?)]);
        let model: ModelConnection = serde_json::from_value(
            json!({"id":"approved","name":"Inert recovery model","provider":"fixture","tenant":"fixture","work_contexts":["mission"],"required_scopes":["operator:use"],"base_url":"https://model.invalid/v1","model":"fixture","api_key":"fixture-model-key","limits":{"maxOutputTokens":64,"maxCompletionCalls":1,"maxToolCalls":1,"deadlineSeconds":30}}),
        )?;
        let template: RuntimeTemplate = serde_json::from_value(json!({
            "id":"recovery","name":"Credential recovery","tenant":"fixture","work_contexts":["mission"],"required_deployer_scopes":["operator:use"],"profile":"operator","scopes":["operator:use"],"roles":["fixture-managed"],"membership":"contributor","models":["approved"],"tools":[],"resource_subscriptions":[],"parameters":{},
            "workload":{"namespace":agent_namespace,"config_map":"fixture-runtime-template","config_digest":wire::runtime_config_revision(&data),"image":args.kernel_image.reference(),"database_secret":"veoveo-surreal-runtime","storage_class":"local-path","storage_gib":1,"cpu_millis":500,"memory_mib":1024,"model_secrets":[{"reference":"fixture-model-key","secret":"fixture-model","key":"api-key"}]}
        }))?;
        let private_path = directory.join("authorization.der");
        process::checked(
            Command::new("openssl")
                .args([
                    "genpkey",
                    "-algorithm",
                    "RSA",
                    "-pkeyopt",
                    "rsa_keygen_bits:2048",
                    "-outform",
                    "DER",
                    "-out",
                ])
                .arg(&private_path),
            30,
        )?;
        fs::set_permissions(&private_path, fs::Permissions::from_mode(0o600))?;
        let modulus = process::checked(
            Command::new("openssl")
                .args(["rsa", "-inform", "DER", "-in"])
                .arg(&private_path)
                .args(["-noout", "-modulus"]),
            10,
        )?;
        let modulus = std::str::from_utf8(&modulus)?
            .trim()
            .strip_prefix("Modulus=")
            .context("RSA public modulus")?;
        let jwks = serde_json::to_string(
            &json!({"keys":[{"kty":"RSA","use":"sig","alg":"RS256","kid":"fixture-as","n":URL_SAFE_NO_PAD.encode(hex::decode(modulus)?),"e":"AQAB"}]}),
        )?;
        let ed_path = directory.join("internal.der");
        process::checked(
            Command::new("openssl")
                .args([
                    "genpkey",
                    "-algorithm",
                    "ED25519",
                    "-outform",
                    "DER",
                    "-out",
                ])
                .arg(&ed_path),
            10,
        )?;
        fs::set_permissions(&ed_path, fs::Permissions::from_mode(0o600))?;
        let installation_secrets = BTreeMap::from([
            (
                "authorization-server-private-key-der-b64".into(),
                STANDARD.encode(fs::read(&private_path)?),
            ),
            (
                "internal-signing-key-der-b64".into(),
                STANDARD.encode(fs::read(&ed_path)?),
            ),
            ("internal-signing-key-id".into(), "fixture-internal".into()),
            ("oidc-client-secret".into(), "fixture-unused".into()),
            (
                "refresh-delivery-key-b64".into(),
                STANDARD.encode([7u8; 32]),
            ),
        ]);
        let plane: GatewayControlPlane = serde_json::from_value(json!({
            "identity_providers":[{"id":"fixture-idp","issuer":"https://idp.invalid","jwks":{"source":"file","path":"/etc/veoveo/gateway/jwks.json"},"metadata":{}}],
            "authorization_servers":[{"id":"fixture-as","issuer":"https://gateway.invalid/oauth","jwks":{"source":"file","path":"/etc/veoveo/gateway/jwks.json"},"access_token_key_id":"fixture-as","access_token_signing_key":"fixture-signing","identity_provider":"fixture-idp","authorization_endpoint":"https://gateway.invalid/oauth/authorize","token_endpoint":"https://gateway.invalid/oauth/token","metadata":{}}],
            "servers":[],"profiles":[{"id":"operator","identity_provider":"fixture-idp","authorization_server":"fixture-as","protected_resource":"https://gateway.invalid/mcp/operator","policy_version":"policy-fixture","auth_modes":["oauth_client_credentials"],"required_scopes":["operator:use"],"servers":[],"metadata":{}}],
            "tenants":[{"id":"fixture","metadata":{}}],"work_contexts":[{"id":"mission","tenant":"fixture","title":"Mission","policy_revision":"policy-fixture","output_policy":{"owner":{"kind":"group","id":"operations"},"initial_grants":[],"classification":null,"data_labels":[]},"memberships":[{"level":"contributor","groups":["operations"],"roles":["fixture-managed"]}]}],
            "policies":[{"version":"policy-fixture","rules":[],"metadata":{}}],"data_labels":[],"oidc_clients":[],
            "secrets":[{"id":"fixture-signing","source":"env","purpose":"jwks_private_key","locator":"VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64","owner":{"kind":"gateway"},"metadata":{}},{"id":"fixture-model-key","source":"env","purpose":"provider_api_key","locator":"VEOVEO_AGENT_MODEL_FIXTURE_KEY","owner":{"kind":"gateway"},"metadata":{}}]
        }))?;
        Ok(Self {
            namespace: agent_namespace,
            manager_image: args.manager_image.clone(),
            model,
            template,
            data,
            plane,
            jwks,
            installation_secrets,
            api_egress: Vec::new(),
        })
    }
    pub fn observe_api_egress(fixture: &Fixture) -> Result<Vec<Value>> {
        // Observe both the Service IP and translated API endpoints. CNI profiles
        // differ in whether their egress enforcement precedes service translation.
        let service: Value = serde_json::from_slice(&process::checked(
            fixture.kubectl().args([
                "--namespace=default",
                "get",
                "service",
                "kubernetes",
                "-o=json",
            ]),
            20,
        )?)?;
        let endpoints: Value = serde_json::from_slice(&process::checked(
            fixture.kubectl().args([
                "--namespace=default",
                "get",
                "endpoints",
                "kubernetes",
                "-o=json",
            ]),
            20,
        )?)?;
        let mut addresses = Vec::new();
        for ip in service["spec"]["clusterIPs"]
            .as_array()
            .context("API Service IPs")?
        {
            addresses.push((
                ip.as_str()
                    .context("API Service IP")?
                    .parse::<std::net::IpAddr>()?,
                443,
            ));
        }
        for subset in endpoints["subsets"]
            .as_array()
            .context("API endpoint subsets")?
        {
            for address in subset["addresses"]
                .as_array()
                .context("API endpoint addresses")?
            {
                for port in subset["ports"].as_array().context("API endpoint ports")? {
                    addresses.push((
                        address["ip"].as_str().context("API endpoint IP")?.parse()?,
                        port["port"].as_u64().context("API endpoint port")?,
                    ));
                }
            }
        }
        ensure!(
            !addresses.is_empty(),
            "no Kubernetes API egress targets observed"
        );
        addresses.sort();
        addresses.dedup();
        Ok(addresses.into_iter().map(|(ip, port)| json!({"cidr":format!("{ip}/{}",if ip.is_ipv4(){32}else{128}),"port":port})).collect())
    }
}
