//! Run the installed Python SDK as a real consumer; Rust owns all acceptance assertions.
use super::*;
use veoveo_gateway_contract::ProtectedResourceId;
use veoveo_types::{InvocationAuthority, WorkContextMembershipLevel};
use veoveo_types::{InvocationProvenance, PrincipalId, TenantId};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Observation {
    pub pages: Vec<veoveo_artifact_contract::ArtifactPage>,
    pub foreign_page: veoveo_artifact_contract::ArtifactPage,
    pub metadata: veoveo_artifact_contract::ArtifactMetadata,
    pub resolved_metadata: veoveo_artifact_contract::ArtifactMetadata,
    pub resolved_sha256: String,
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
#[serde(rename_all = "camelCase")]
pub(super) struct CallerInput {
    pub(super) bearer_token: String,
    pub(super) identity: GatewayInternalIdentity,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
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
    let issuer = fixture_issuer(installation).await?;
    let foreign_tenant = TenantId::parse(format!("artifact-consumer-{}", uuid::Uuid::new_v4()))?;
    ensure!(
        foreign_tenant != installation.operator.tenant,
        "foreign fixture tenant collided"
    );
    let input = Input {
        artifact_service_url: consumer.artifact_service_url.as_str(),
        caller: fixture_caller(&issuer, installation, installation.operator.tenant.clone())?,
        foreign: fixture_caller(&issuer, installation, foreign_tenant)?,
        large,
        csv,
    };
    let mut command = tokio::process::Command::new("kubectl");
    command
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
        .kill_on_drop(true);
    let mut child = veoveo_testing_support::spawn_async(command)?;
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

pub(super) async fn recovery_caller(installation: &InstalledTarget) -> Result<CallerInput> {
    let issuer = fixture_issuer(installation).await?;
    fixture_caller(&issuer, installation, installation.operator.tenant.clone())
}

async fn fixture_issuer(installation: &InstalledTarget) -> Result<GatewayInternalTokenIssuer> {
    let target = &installation.target;
    let consumer = target
        .artifact_consumer
        .as_ref()
        .context("missing Artifact consumer")?;
    let context = &target.kubernetes.context;
    let namespace = &target.kubernetes.namespace;
    let mut command = tokio::process::Command::new("kubectl");
    command
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
        .kill_on_drop(true);
    let result = veoveo_testing_support::output_async(command, Duration::from_secs(45))
        .await
        .context("reading installed conformance signing material timed out")?;
    ensure!(
        result.status.success(),
        "could not read installed conformance signing material"
    );
    let secret: Secret = serde_json::from_slice(&result.stdout)?;
    let key_id = String::from_utf8(STANDARD.decode(secret.data.key_id)?)?;
    let key = STANDARD.decode(String::from_utf8(STANDARD.decode(secret.data.key)?)?.trim())?;
    let issuer = GatewayInternalTokenIssuer::new(
        TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
        GatewayInternalSigningKey::new(key_id, key)?,
    );
    Ok(issuer)
}

fn fixture_caller(
    issuer: &GatewayInternalTokenIssuer,
    installation: &InstalledTarget,
    tenant: TenantId,
) -> Result<CallerInput> {
    let actor = Principal {
        id: PrincipalId::parse("https://conformance.veoveo.local#artifact-consumer")?,
        kind: PrincipalKind::Service,
        issuer: TokenIssuer::parse("https://conformance.veoveo.local")?,
        subject: TokenSubject::parse("artifact-consumer")?,
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
        work_context: installation.operator.work_context.id.clone(),
        tenant,
        membership: WorkContextMembershipLevel::Viewer,
        policy_revision: installation.operator.work_context.policy_revision.clone(),
        output_policy: installation.operator.work_context.output_policy.clone(),
        provenance: InvocationProvenance::Automated,
    };
    issue_fixture_caller(
        issuer,
        GatewayProfileId::parse(installation.profile())?,
        installation.operator.resource.clone(),
        actor,
        authority,
    )
}

fn issue_fixture_caller(
    issuer: &GatewayInternalTokenIssuer,
    profile: GatewayProfileId,
    audience: ProtectedResourceId,
    actor: Principal,
    authority: InvocationAuthority,
) -> Result<CallerInput> {
    let now = chrono::Utc::now();
    let expires_at = now + chrono::TimeDelta::minutes(20);
    let request_context = GatewayRequestContext {
        format: GatewayRequestContextFormat::V2,
        audit: veoveo_mcp_contract::audit::AuditRequest::background(),
        principal: actor.clone(),
        access_token: AccessTokenSubject {
            managed_execution: None,
            issuer: actor.issuer.clone(),
            subject: actor.subject.clone(),
            oauth_client_id: OAuthClientId::parse(actor.subject.as_str())?,
            session_family: None,
            audience,
            work_context: authority.work_context.clone(),
            invocation_mode: authority.provenance.mode(),
            initiator: None,
            delegation_id: None,
            scopes: actor.scopes.clone(),
            jwt_id: None,
            issued_at: now,
            not_before: Some(now),
            expires_at,
        },
    };
    let token = issuer.issue(
        profile,
        ServerSlug::parse("datasheet")?,
        actor,
        authority,
        Some(request_context),
        expires_at,
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
from pydantic import BaseModel, ConfigDict, model_validator
from veoveo_mcp.artifacts import HttpArtifactPlane
from veoveo_mcp.contract.identity import PlaneCaller, GatewayInternalIdentity
from veoveo_mcp.contract.artifacts import ArtifactUploadReceipt, ListArtifactsRequest

class CallerInput(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    bearerToken: str
    identity: GatewayInternalIdentity

class ConsumerInput(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)
    artifactServiceUrl: str
    caller: CallerInput
    foreign: CallerInput
    large: ArtifactUploadReceipt
    csv: ArtifactUploadReceipt

    @model_validator(mode="after")
    def selected_uploads(self):
        if self.large.upload_id == self.csv.upload_id or self.large.artifact_id == self.csv.artifact_id:
            raise ValueError("receipts reuse one selected upload or occurrence")
        return self

async def main():
    data = ConsumerInput.model_validate_json(sys.stdin.buffer.read())
    caller = PlaneCaller.from_identity(data.caller.identity, data.caller.bearerToken)
    foreign = PlaneCaller.from_identity(data.foreign.identity, data.foreign.bearerToken)
    plane = HttpArtifactPlane(data.artifactServiceUrl)
    large, csv = data.large, data.csv
    observation = {}
    try:
        pages, cursor = [], None
        async with asyncio.timeout(60):
            for _ in range(256):
                page = await plane.list(caller, ListArtifactsRequest(cursor=cursor, limit=1))
                pages.append(page.model_dump(mode='json'))
                if page.next_cursor is None:
                    break
                cursor = page.next_cursor
            else:
                raise RuntimeError('Artifact consumer catalog exceeds 256-page fixture bound')
            foreign_page = await plane.list(foreign, ListArtifactsRequest(limit=1))
        observation.update(pages=pages, foreignPage=foreign_page.model_dump(mode='json'))
        metadata = await plane.head(caller, csv.artifact_id)
        resolved = await plane.resolve(caller, csv.artifact_uri, max_bytes=1024)
        observation.update(metadata=metadata.model_dump(mode='json'),
                           resolvedMetadata=resolved.metadata.model_dump(mode='json'),
                           resolvedSha256=hashlib.sha256(resolved.bytes_).hexdigest())
        started = time.monotonic()
        count, maximum, digest = 0, 0, hashlib.sha256()
        async with plane.stream(caller, large.artifact_uri, max_bytes=large.byte_len, expected_sha256=large.sha256) as stream:
            async for chunk in stream:
                count += len(chunk)
                maximum = max(maximum, len(chunk))
                digest.update(chunk)
        observation.update(bytes=count, sha256=digest.hexdigest(), maxChunkBytes=maximum, elapsedSeconds=time.monotonic()-started)
        async with plane.stream(caller, large.artifact_uri, max_bytes=large.byte_len) as stream:
            async for chunk in stream:
                observation['earlyBytes'] = len(chunk)
                break
        observation['smallLimitError'] = ''
        try:
            async with plane.stream(caller, large.artifact_uri, max_bytes=1024):
                pass
        except Exception as error:
            observation['smallLimitError'] = type(error).__name__
        async with plane.materialize(caller, csv.artifact_uri, max_bytes=1024, expected_sha256=csv.sha256) as path:
            observation['materializedInside'] = path.is_file()
            observation['csvSha256'] = hashlib.sha256(path.read_bytes()).hexdigest()
            temporary_path = path
        observation['materializedAfter'] = temporary_path.exists()
        observation['foreignTenantError'] = ''
        try:
            async with plane.stream(foreign, large.artifact_uri, max_bytes=large.byte_len) as stream:
                async for chunk in stream:
                    break
        except Exception as error:
            observation['foreignTenantError'] = type(error).__name__
        observation['peakRssKib'] = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
        print(json.dumps(observation))
    finally:
        await plane.close()

if __name__ == "__main__":
    asyncio.run(main())
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_artifact_contract::{ArtifactId, ArtifactUploadId};

    #[test]
    fn direct_consumer_identity_reaches_artifact_audit_admission() -> Result<()> {
        let issuer = GatewayInternalTokenIssuer::new(
            TokenIssuer::parse(GATEWAY_INTERNAL_TOKEN_ISSUER)?,
            GatewayInternalSigningKey::new(
                "artifact-consumer-native",
                STANDARD.decode(INTERNAL_SIGNING_KEY_DER_B64)?,
            )?,
        );
        let actor = Principal {
            id: PrincipalId::parse("https://conformance.veoveo.local#artifact-consumer")?,
            kind: PrincipalKind::Service,
            issuer: TokenIssuer::parse("https://conformance.veoveo.local")?,
            subject: TokenSubject::parse("artifact-consumer")?,
            tenant: Some(TenantId::parse("consumer-native")?),
            groups: BTreeSet::new(),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::new(),
            scopes: BTreeSet::new(),
            data_labels: BTreeSet::new(),
            assurances: BTreeSet::new(),
            authenticated_at: Some(chrono::Utc::now()),
        };
        let authority = InvocationAuthority {
            work_context: veoveo_types::WorkContextId::parse("consumer-native")?,
            tenant: actor.tenant.clone().unwrap(),
            membership: WorkContextMembershipLevel::Viewer,
            policy_revision: veoveo_types::PolicyVersion::parse("consumer-native")?,
            output_policy: veoveo_types::WorkContextOutputPolicy {
                owner: veoveo_types::AccessSubject::Principal(actor.id.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Automated,
        };
        let audience = ProtectedResourceId::parse("https://conformance.veoveo.local/mcp")?;
        let caller = issue_fixture_caller(
            &issuer,
            GatewayProfileId::parse("consumer-native")?,
            audience.clone(),
            actor.clone(),
            authority,
        )?;
        let identity = caller.identity;
        let admitted = identity.audit_context()?;
        assert_eq!(admitted.actor.principal, actor.id);
        let context = identity.request_context.as_ref().unwrap();
        assert_eq!(context.access_token.audience, audience);
        assert_eq!(context.access_token.scopes, actor.scopes);
        assert_eq!(
            context.access_token.oauth_client_id.as_str(),
            actor.subject.as_str()
        );
        assert!(context.access_token.expires_at > chrono::Utc::now());
        assert!(identity.expires_at <= context.access_token.expires_at);
        let mut missing = identity.clone();
        missing.request_context = None;
        assert!(missing.audit_context().is_err());
        let mut foreign = identity.clone();
        foreign
            .request_context
            .as_mut()
            .unwrap()
            .access_token
            .oauth_client_id = OAuthClientId::parse("foreign-consumer")?;
        assert!(foreign.audit_context().is_err());
        let mut scopes = identity;
        scopes
            .request_context
            .as_mut()
            .unwrap()
            .access_token
            .scopes
            .insert(veoveo_types::ScopeName::parse("artifact:read")?);
        assert!(scopes.audit_context().is_err());
        Ok(())
    }

    #[tokio::test]
    async fn actual_upload_receipt_python_peer_refuses_retired_mixed_and_foreign_fields() {
        let id = ArtifactId::new();
        let receipt = ArtifactUploadReceipt {
            upload_id: ArtifactUploadId::new(),
            artifact_id: id,
            artifact_uri: veoveo_artifact_contract::ArtifactUri::plane(id),
            sha256: UploadSha256::parse("a".repeat(64)).unwrap(),
            byte_len: 1,
            mime_type: "application/octet-stream".into(),
            filename: "fixture.bin".into(),
            created_at: chrono::Utc::now(),
        };
        let metadata: veoveo_artifact_contract::ArtifactMetadata =
            serde_json::from_str(include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../../platform/artifacts/contract/tests/fixtures/metadata-output.json"
            )))
            .unwrap();
        // Every timestamp passes the actual Chrono owner and is emitted through the
        // actual Artifact structs. The SDK must retain those complete emitted values.
        let cases = [
            ("whole_seconds", "2026-10-05T12:34:56Z".parse().unwrap()),
            (
                "nanoseconds",
                "2026-10-05T12:34:56.123456789Z".parse().unwrap(),
            ),
            (
                "adjacent_nanosecond_a",
                "2026-10-05T12:34:56.123456001Z".parse().unwrap(),
            ),
            (
                "adjacent_nanosecond_b",
                "2026-10-05T12:34:56.123456002Z".parse().unwrap(),
            ),
            (
                "leap_second",
                "2016-12-31T23:59:60.123456789Z".parse().unwrap(),
            ),
            (
                "year_zero",
                "0000-01-01T00:00:00.000000001Z".parse().unwrap(),
            ),
            (
                "negative_year",
                "-0001-01-01T00:00:00.000000001Z".parse().unwrap(),
            ),
            (
                "extended_positive_year",
                "+10000-01-01T00:00:00.000000001Z".parse().unwrap(),
            ),
            ("minimum_year", chrono::DateTime::<chrono::Utc>::MIN_UTC),
            ("maximum_year", chrono::DateTime::<chrono::Utc>::MAX_UTC),
            (
                "positive_offset",
                "2026-10-05T14:34:56.123456789+02:00".parse().unwrap(),
            ),
            (
                "negative_offset",
                "2026-10-05T07:04:56.123456789-05:30".parse().unwrap(),
            ),
        ]
        .into_iter()
        .map(|(name, timestamp)| {
            let mut receipt = receipt.clone();
            receipt.created_at = timestamp;
            let mut metadata = metadata.clone();
            metadata.created_at = timestamp;
            metadata.compliance.retention_expires_at = Some(timestamp);
            serde_json::json!({"name":name, "receipt":receipt, "metadata":metadata})
        })
        .collect::<Vec<_>>();
        if let Some(path) = std::env::var_os("VEOVEO_ARTIFACT_TIMESTAMP_SCHEMA_CAPTURE") {
            let schemas = serde_json::json!({
                "ArtifactUploadReceipt": schemars::schema_for!(ArtifactUploadReceipt),
                "ArtifactMetadata": schemars::schema_for!(veoveo_artifact_contract::ArtifactMetadata),
            });
            std::fs::write(path, serde_json::to_vec_pretty(&schemas).unwrap()).unwrap();
        }
        if let Some(path) = std::env::var_os("VEOVEO_ARTIFACT_TIMESTAMP_CAPTURE") {
            std::fs::write(path, serde_json::to_vec_pretty(&cases).unwrap()).unwrap();
            return;
        }
        let interpreter = std::env::var_os("VEOVEO_ARTIFACT_CONSUMER_PYTHON")
            .expect("receipt peer control requires the qualified SDK Python interpreter");
        let source =
            format!("__name__ = 'receipt_control'\n{CONSUMER}\n{CONTROL}\n{TIMESTAMP_CONTROL}");
        let mut command = tokio::process::Command::new(interpreter);
        command.args([
            "-c",
            &source,
            &serde_json::to_string(&receipt).unwrap(),
            veoveo_artifact_contract::ArtifactUri::plane(ArtifactId::new()).as_ref(),
            &serde_json::to_string(&cases).unwrap(),
        ]);
        let output = veoveo_testing_support::output_async(command, Duration::from_secs(30))
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "receipt peer refused current owner bytes or admitted a mutation: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout, b"receipt-peer-controls-passed\n");
    }

    const TIMESTAMP_CONTROL: &str = r#"
from veoveo_mcp.contract.artifacts import ArtifactMetadata
observations = []
for case in json.loads(sys.argv[3]):
    for model, key, fields in (
        (ArtifactUploadReceipt, 'receipt', ('createdAt',)),
        (ArtifactMetadata, 'metadata', ('createdAt', 'compliance.retentionExpiresAt')),
    ):
        current = case[key]
        adapter = TypeAdapter(model)
        paths = (
            ('value', lambda: model.model_validate(current)),
            ('json', lambda: model.model_validate_json(json.dumps(current))),
            ('adapter_value', lambda: adapter.validate_python(current)),
            ('adapter_json', lambda: adapter.validate_json(json.dumps(current))),
        )
        for path, decode in paths:
            try:
                output = decode().model_dump(mode='json')
            except (ValidationError, ValueError, TypeError) as error:
                observations.append(dict(case=case['name'], model=key, decoder=path,
                                         errorType=type(error).__name__, exact=False))
                continue
            for field in fields:
                parts = field.split('.')
                expected, actual = current, output
                for part in parts:
                    expected, actual = expected[part], actual[part]
                observations.append(dict(case=case['name'], model=key, decoder=path,
                                         field=field, input=expected, output=actual,
                                         exact=expected == actual))
# Only declared timestamp values and error classes enter this bounded diagnostic.
# It contains no caller, capability, provider payload or credential material.
failed = [row for row in observations if not row['exact']]
if failed:
    diagnostic = json.dumps(dict(timestampObservations=observations))
    assert len(diagnostic.encode()) <= 32 * 1024, 'timestamp diagnostic exceeded 32KiB'
    print(diagnostic, file=sys.stderr)
    raise AssertionError('Artifact SDK changed or refused actual Rust owner timestamps')
"#;

    const CONTROL: &str = r#"
from copy import deepcopy
from pydantic import TypeAdapter, ValidationError
current = json.loads(sys.argv[1])
adapter = TypeAdapter(ArtifactUploadReceipt)
def decoders(value):
    return (
        lambda: ArtifactUploadReceipt.model_validate(value),
        lambda: ArtifactUploadReceipt.model_validate_json(json.dumps(value)),
        lambda: adapter.validate_python(value),
        lambda: adapter.validate_json(json.dumps(value)),
    )
for decode in decoders(current):
    admitted = decode()
    assert str(admitted.artifact_uri) == current['artifactUri']
    assert str(admitted.artifact_id) == current['artifactId']
def refuses(value):
    for decode in decoders(value):
        try:
            decode()
        except (ValidationError, ValueError, TypeError):
            continue
        raise AssertionError('receipt peer admitted a controlled mutation')
for key, retired in [('uploadId','upload_id'),('artifactId','artifact_id'),('artifactUri','artifact_uri'),('byteLen','byte_len'),('mimeType','mime_type'),('createdAt','created_at')]:
    for keep_current in (False, True):
        bad = deepcopy(current)
        bad[retired] = bad[key]
        if not keep_current:
            del bad[key]
        refuses(bad)
for key, value in [('byteLen', True), ('byteLen', -1), ('createdAt','not-a-timestamp'), ('createdAt', True), ('createdAt', 123), ('createdAt', '123'), ('createdAt','2026-01-01T00:00:00'), ('sha256',current['sha256']+'\n'), ('artifactId','not-an-artifact-id'), ('unknown',0)]:
    bad = deepcopy(current)
    bad[key] = value
    refuses(bad)
bad = deepcopy(current)
bad['artifactUri'] = sys.argv[2]
refuses(bad)
print('receipt-peer-controls-passed')
"#;
}
