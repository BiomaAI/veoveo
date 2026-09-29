//! Run the installed Python SDK as a real consumer; Rust owns all acceptance assertions.
use super::*;
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel};
use veoveo_types::{InvocationProvenance, PrincipalId, TenantId};

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct Observation {
    pub bytes: u64,
    pub sha256: String,
    pub max_chunk_bytes: usize,
    pub peak_rss_kib: u64,
    pub elapsed_seconds: f64,
    pub early_bytes: usize,
    pub small_limit_error: String,
    pub csv_sha256: String,
    pub materialized_inside: bool,
    pub materialized_after: bool,
    pub foreign_tenant_error: String,
}

#[derive(Deserialize)]
struct Secret {
    data: SigningData,
}

#[derive(Deserialize)]
struct SigningData {
    #[serde(rename = "internal-signing-key-der-b64")]
    key: String,
    #[serde(rename = "internal-signing-key-id")]
    key_id: String,
}

#[derive(Serialize)]
struct CallerInput {
    bearer_token: String,
    identity: GatewayInternalIdentity,
}

#[derive(Serialize)]
struct Input<'a> {
    artifact_service_url: &'a str,
    caller: CallerInput,
    foreign: CallerInput,
    large: &'a BrowserReceipt,
    csv: &'a BrowserReceipt,
}

pub(super) async fn consume(
    installation: &InstalledTarget,
    large: &BrowserReceipt,
    csv: &BrowserReceipt,
) -> Result<Observation> {
    // This is a direct installed-plane conformance fixture, not evidence of public
    // OAuth issuance. The preceding HTTP/MCP checks use registered machine OAuth.
    // Only the local Rust harness sees the signer; the Python child receives two
    // short-lived read identities over stdin and never receives the signing key.
    let target = &installation.target;
    let consumer = target.artifact_consumer.as_ref().context(
        "artifact-upload-consumers requires artifactConsumer in the installation target",
    )?;
    let context = &target.kubernetes.context;
    let namespace = &target.kubernetes.namespace;
    let result = tokio::time::timeout(
        Duration::from_secs(45),
        tokio::process::Command::new("kubectl")
            .args([
                "--request-timeout=30s",
                "--context",
                context,
                "-n",
                namespace,
                "get",
                "secret",
                &consumer.internal_signing_secret,
                "-o",
                "json",
            ])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .context("reading installed conformance signing material timed out")??;
    ensure!(
        result.status.success(),
        "could not read installed conformance signing material"
    );
    let secret: Secret = serde_json::from_slice(&result.stdout)?;
    let key_id = String::from_utf8(STANDARD.decode(secret.data.key_id)?)?;
    let key = STANDARD.decode(String::from_utf8(STANDARD.decode(secret.data.key)?)?.trim())?;
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::new(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(key_id, key)?,
    );
    let foreign_tenant = TenantId::new(format!("artifact-consumer-{}", uuid::Uuid::new_v4()))?;
    ensure!(
        foreign_tenant != installation.tenant,
        "foreign fixture tenant collided"
    );
    let input = Input {
        artifact_service_url: consumer.artifact_service_url.as_str(),
        caller: fixture_caller(&issuer, installation, installation.tenant.clone())?,
        foreign: fixture_caller(&issuer, installation, foreign_tenant)?,
        large,
        csv,
    };
    let mut child = tokio::process::Command::new("kubectl")
        .args([
            "--request-timeout=30s",
            "--context",
            context,
            "-n",
            namespace,
            "exec",
            "-i",
            &format!("deployment/{}", consumer.python_deployment),
            "--",
            "python",
            "-c",
            CONSUMER,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child.stdin.take().context("consumer stdin unavailable")?;
    stdin.write_all(&serde_json::to_vec(&input)?).await?;
    drop(stdin);
    let output = tokio::time::timeout(Duration::from_secs(900), child.wait_with_output())
        .await
        .context("installed Python consumer timed out")??;
    ensure!(
        output.status.success(),
        "installed Python consumer failed: {}",
        String::from_utf8_lossy(&output.stderr)
            .replace(&input.caller.bearer_token, "<redacted>")
            .replace(&input.foreign.bearer_token, "<redacted>")
    );
    serde_json::from_slice(&output.stdout).context("invalid Python consumption observations")
}

fn fixture_caller(
    issuer: &GatewayInternalTokenIssuer,
    installation: &InstalledTarget,
    tenant: TenantId,
) -> Result<CallerInput> {
    let actor = Principal {
        id: PrincipalId::new("https://conformance.veoveo.local#artifact-consumer")?,
        kind: PrincipalKind::Service,
        issuer: TokenIssuer::new("https://conformance.veoveo.local")?,
        subject: TokenSubject::new("artifact-consumer")?,
        tenant: Some(tenant.clone()),
        groups: BTreeSet::new(),
        group_roles: BTreeSet::new(),
        roles: BTreeSet::new(),
        scopes: BTreeSet::new(),
        data_labels: BTreeSet::new(),
        assurances: BTreeSet::new(),
        authenticated_at: Some(chrono::Utc::now()),
    };
    let authority = InvocationAuthority {
        work_context: installation.work_context.id.clone(),
        tenant,
        membership: WorkContextMembershipLevel::Viewer,
        policy_revision: installation.work_context.policy_revision.clone(),
        output_policy: installation.work_context.output_policy.clone(),
        provenance: InvocationProvenance::Automated,
    };
    let token = issuer.issue(
        GatewayProfileId::new(installation.profile())?,
        ServerSlug::new("datasheet")?,
        actor,
        authority,
        None,
        chrono::Utc::now() + chrono::TimeDelta::minutes(20),
    )?;
    Ok(CallerInput {
        bearer_token: token.bearer_token,
        identity: token.identity,
    })
}

// This program performs SDK operations and reports observations. It has no smoke
// assertions, retries, service lifecycle, or independently implemented downloader.
const CONSUMER: &str = r#"
import asyncio, hashlib, json, resource, sys, time
from pathlib import Path
from veoveo_mcp.artifacts import HttpArtifactPlane
from veoveo_mcp.contract.identity import PlaneCaller, GatewayInternalIdentity

async def main():
    data = json.load(sys.stdin)
    caller = PlaneCaller.from_identity(GatewayInternalIdentity.model_validate(data['caller']['identity']), data['caller']['bearer_token'])
    foreign = PlaneCaller.from_identity(GatewayInternalIdentity.model_validate(data['foreign']['identity']), data['foreign']['bearer_token'])
    plane = HttpArtifactPlane(data['artifact_service_url'])
    large, csv = data['large'], data['csv']
    observation = {}
    try:
        started = time.monotonic()
        count, maximum, digest = 0, 0, hashlib.sha256()
        async with plane.stream(caller, large['artifact_uri'], max_bytes=large['byte_len'], expected_sha256=large['sha256']) as stream:
            async for chunk in stream:
                count += len(chunk)
                maximum = max(maximum, len(chunk))
                digest.update(chunk)
        observation.update(bytes=count, sha256=digest.hexdigest(), max_chunk_bytes=maximum, elapsed_seconds=time.monotonic()-started)
        async with plane.stream(caller, large['artifact_uri'], max_bytes=large['byte_len']) as stream:
            async for chunk in stream:
                observation['early_bytes'] = len(chunk)
                break
        observation['small_limit_error'] = ''
        try:
            async with plane.stream(caller, large['artifact_uri'], max_bytes=1024):
                pass
        except Exception as error:
            observation['small_limit_error'] = type(error).__name__
        async with plane.materialize(caller, csv['artifact_uri'], max_bytes=1024, expected_sha256=csv['sha256']) as path:
            observation['materialized_inside'] = path.is_file()
            observation['csv_sha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
            temporary_path = path
        observation['materialized_after'] = temporary_path.exists()
        observation['foreign_tenant_error'] = ''
        try:
            async with plane.stream(foreign, large['artifact_uri'], max_bytes=large['byte_len']) as stream:
                async for chunk in stream:
                    break
        except Exception as error:
            observation['foreign_tenant_error'] = type(error).__name__
        observation['peak_rss_kib'] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        print(json.dumps(observation))
    finally:
        await plane.close()

asyncio.run(main())
"#;
