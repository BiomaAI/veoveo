//! Prearmed local Docker Engine interruption of the exact fixture container.
use super::*;
use serde::Deserialize;

const API_VERSION: &str = "1.44";
const BODY_LIMIT: usize = 65_536;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct EngineVersion {
    #[serde(rename = "ApiVersion")]
    api_version: String,
    #[serde(rename = "MinAPIVersion")]
    min_api_version: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerIdentity {
    id: String,
    state: ContainerState,
}
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ContainerState {
    running: bool,
    paused: bool,
}

pub(super) struct OwnedEngineFreeze {
    client: reqwest::Client,
    stop: reqwest::Request,
}

fn socket_path(endpoint: &str) -> Result<PathBuf> {
    let endpoint = url::Url::parse(endpoint).context("invalid active Docker endpoint")?;
    ensure!(
        endpoint.scheme() == "unix"
            && !endpoint.cannot_be_a_base()
            && endpoint.path().starts_with('/')
            && endpoint.host_str().is_none()
            && endpoint.query().is_none()
            && endpoint.fragment().is_none(),
        "View interruption requires the active Docker context's local unix socket; unsupported endpoint profile"
    );
    let mut file = url::Url::parse("file:///")?;
    // url 2.5.8 preserves existing percent escapes in set_path; to_file_path
    // performs the single URI-to-filesystem decoding step.
    file.set_path(endpoint.path());
    let path = file
        .to_file_path()
        .map_err(|_| anyhow!("Docker unix endpoint has no admitted absolute socket path"))?;
    ensure!(path.is_absolute(), "Docker socket path must be absolute");
    Ok(path)
}

fn api_pair(value: &str) -> Result<(u16, u16)> {
    let (major, minor) = value
        .split_once('.')
        .context("invalid Docker API version")?;
    ensure!(!minor.contains('.'), "invalid Docker API version");
    Ok((major.parse()?, minor.parse()?))
}
fn engine_url(segments: &[&str]) -> Result<url::Url> {
    let mut url = url::Url::parse("http://docker.invalid/")?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("Engine URL cannot carry path segments"))?
        .clear()
        .extend(segments);
    Ok(url)
}

async fn bounded_response(response: reqwest::Response) -> Result<(u16, Vec<u8>)> {
    use futures::StreamExt;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        ensure!(
            body.len() + chunk.len() <= BODY_LIMIT,
            "Docker Engine response exceeds 65536 bytes"
        );
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}

fn endpoint_override(host: Option<String>, context: Option<String>) -> Option<String> {
    if context.is_some_and(|context| !context.is_empty()) {
        None
    } else {
        host.filter(|host| !host.is_empty())
    }
}

impl OwnedEngineFreeze {
    pub(super) async fn prepare(cid: &str, deadline: tokio::time::Instant) -> Result<Self> {
        // DOCKER_CONTEXT overrides DOCKER_HOST, matching the Docker CLI fixture owner.
        let endpoint = endpoint_override(
            std::env::var("DOCKER_HOST").ok(),
            std::env::var("DOCKER_CONTEXT").ok(),
        );
        let endpoint = match endpoint {
            Some(endpoint) => endpoint,
            None => {
                let output = lifecycle_docker(
                    [
                        "context".into(),
                        "inspect".into(),
                        "--format={{json .Endpoints.docker.Host}}".into(),
                    ],
                    deadline,
                )
                .await?;
                serde_json::from_str::<String>(&output)
                    .context("active Docker context omitted its endpoint")?
            }
        };
        let socket = socket_path(&endpoint)?;
        Self::connect(cid, socket, deadline).await
    }

    async fn connect(cid: &str, socket: PathBuf, deadline: tokio::time::Instant) -> Result<Self> {
        ensure!(
            cid.len() == 64
                && cid
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "refuse interruption: owned container ID must be 64 lowercase hex digits"
        );
        let client = reqwest::Client::builder()
            .unix_socket(socket)
            .timeout(Duration::from_secs(2))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        tokio::time::timeout_at(deadline, async {
            let (status, body) =
                bounded_response(client.get(engine_url(&["version"])?).send().await?).await?;
            ensure!(
                status == 200,
                "Docker Engine GET /version status {status}: {}",
                bounded_redacted(&String::from_utf8_lossy(&body), &[])
            );
            let version: EngineVersion = serde_json::from_slice(&body)?;
            let selected = api_pair(API_VERSION)?;
            ensure!(
                api_pair(&version.min_api_version)? <= selected
                    && selected <= api_pair(&version.api_version)?,
                "Docker Engine does not support the interruption API profile v{API_VERSION}"
            );
            let path = format!("v{API_VERSION}");
            let (status, body) = bounded_response(
                client
                    .get(engine_url(&[&path, "containers", cid, "json"])?)
                    .send()
                    .await?,
            )
            .await?;
            ensure!(
                status == 200,
                "owned container inspection status {status}: {}",
                bounded_redacted(&String::from_utf8_lossy(&body), &[])
            );
            let identity: ContainerIdentity = serde_json::from_slice(&body)?;
            ensure!(
                identity.id == cid && identity.state.running && !identity.state.paused,
                "refuse interruption: Engine identity differs or owned process is not running"
            );
            let mut url = engine_url(&[&path, "containers", cid, "kill"])?;
            url.query_pairs_mut().append_pair("signal", "STOP");
            let stop = client.post(url).build()?;
            Ok(Self { client, stop })
        })
        .await
        .context("owned Engine preparation exceeded lifecycle deadline")?
    }

    pub(super) async fn freeze(self, deadline: tokio::time::Instant) -> Result<()> {
        tokio::time::timeout_at(deadline, async {
            let request = self.stop.url().clone();
            let (status, body) = bounded_response(self.client.execute(self.stop).await?).await?;
            ensure!(
                status == 204,
                "POST {request}: Docker Engine status {status}: {}",
                bounded_redacted(&String::from_utf8_lossy(&body), &[])
            );
            Ok(())
        })
        .await
        .context("owned Engine STOP exceeded lifecycle deadline")?
    }
}

/// Selected owner fields from the private runtime envelope; no synthetic Task data.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureInput {
    request: CaptureFrameRequest,
    view_snapshot: CaptureSnapshot,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureSnapshot {
    view: ViewRecord,
    composition: CaptureComposition,
}
#[derive(Deserialize)]
struct CaptureComposition {
    record: SceneComposition,
}

pub(super) struct CaptureExpectation {
    request: CaptureFrameRequest,
    view: ViewRecord,
    composition: SceneComposition,
    principal: PrincipalId,
}
#[derive(Clone)]
struct FrozenClaim<'a> {
    id: &'a surrealdb::types::RecordId,
    server: &'a surrealdb::types::RecordId,
    operation: &'a veoveo_types::TaskTypeName,
    principal: &'a PrincipalId,
    work_context: &'a WorkContextId,
    tenant: &'a TenantId,
    status: veoveo_platform_store::TaskStatus,
    lease_owner: Option<&'a str>,
    lease_expiry: Option<chrono::DateTime<Utc>>,
    settled: bool,
    input: &'a Value,
}
impl CaptureExpectation {
    pub(super) fn new(request: &Value, view: &Value, composition: &Value) -> Result<Self> {
        Ok(Self {
            request: serde_json::from_value(request.clone())?,
            view: serde_json::from_value(view.clone())?,
            composition: serde_json::from_value(composition.clone())?,
            principal: PrincipalId::parse("https://smoke.veoveo.local#view-smoke-a")?,
        })
    }
    pub(super) fn admit(
        &self,
        row: &veoveo_platform_store::TaskRecord,
        task: veoveo_types::TaskId,
    ) -> Result<()> {
        ensure!(
            row.owner
                == veoveo_platform_store::deterministic_principal_id(
                    "local",
                    self.principal.as_str()
                )?
                .record_id()
                && row.tenant
                    == veoveo_platform_store::deterministic_tenant_id("local")?.record_id(),
            "frozen claim durable owner or tenant differs from the actual caller"
        );
        self.admit_claim(
            FrozenClaim {
                id: &row.id,
                server: &row.server,
                operation: &row.task_type,
                principal: &row.owner_context.principal_key,
                work_context: &row.owner_context.authority.work_context,
                tenant: &row.owner_context.authority.tenant,
                status: row.status,
                lease_owner: row.lease_owner.as_deref(),
                lease_expiry: row.lease_expires_at,
                settled: row.result.is_some() || row.completed_at.is_some(),
                input: &row.request.input,
            },
            task,
        )
    }
    fn admit_claim(&self, claim: FrozenClaim<'_>, task: veoveo_types::TaskId) -> Result<()> {
        use veoveo_types::TaskTypeDefinition;
        ensure!(
            *claim.id == veoveo_platform_store::task_record_id(task),
            "frozen claim belongs to another Task"
        );
        ensure!(
            *claim.server == surrealdb::types::RecordId::new("mcp_server", "view")
                && *claim.operation == veoveo_view_mcp::contract::ViewTaskKind::CaptureFrame.name(),
            "frozen claim belongs to another server or operation"
        );
        ensure!(
            *claim.principal == self.principal
                && *claim.work_context == WorkContextId::parse("smoke")?
                && *claim.tenant == TenantId::parse("local")?,
            "frozen claim belongs to another owner or invocation context"
        );
        ensure!(
            matches!(
                claim.status,
                veoveo_platform_store::TaskStatus::Queued
                    | veoveo_platform_store::TaskStatus::Running
                    | veoveo_platform_store::TaskStatus::Waiting
            ) && claim.lease_owner.is_some_and(|owner| !owner.is_empty())
                && claim.lease_expiry.is_some_and(|expiry| expiry > Utc::now())
                && !claim.settled,
            "frozen capture is terminal, unleased or expired"
        );
        let input: CaptureInput = serde_json::from_value(claim.input.clone())
            .context("frozen capture omitted its admitted request and snapshot")?;
        ensure!(
            serde_json::to_value(&input.request)? == serde_json::to_value(&self.request)?
                && serde_json::to_value(&input.view_snapshot.view)?
                    == serde_json::to_value(&self.view)?
                && input.view_snapshot.composition.record == self.composition,
            "frozen capture changed its actual MCP request or owner snapshot"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn engine(
        responses: Vec<(u16, String)>,
    ) -> (
        tempfile::TempDir,
        PathBuf,
        tokio::task::JoinHandle<Vec<String>>,
    ) {
        engine_named(responses, "engine.sock").await
    }
    async fn engine_named(
        responses: Vec<(u16, String)>,
        name: &str,
    ) -> (
        tempfile::TempDir,
        PathBuf,
        tokio::task::JoinHandle<Vec<String>>,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join(name);
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let task = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut requests = Vec::new();
            for (status, body) in responses {
                let mut headers = Vec::new();
                loop {
                    let byte = stream.read_u8().await.unwrap();
                    headers.push(byte);
                    assert!(headers.len() < 4096);
                    if headers.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                requests.push(
                    String::from_utf8(headers)
                        .unwrap()
                        .lines()
                        .next()
                        .unwrap()
                        .to_owned(),
                );
                let response = format!(
                    "HTTP/1.1 {status} Mock\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        (directory, socket, task)
    }
    fn version() -> (u16, String) {
        (
            200,
            json!({"ApiVersion":"1.53","MinAPIVersion":"1.44"}).to_string(),
        )
    }
    fn identity(cid: &str) -> (u16, String) {
        (
            200,
            json!({"Id":cid,"State":{"Running":true,"Paused":false}}).to_string(),
        )
    }
    fn deadline() -> tokio::time::Instant {
        tokio::time::Instant::now() + Duration::from_secs(2)
    }

    #[tokio::test]
    async fn prearmed_engine_stops_only_the_admitted_owned_cid() {
        let cid = "a".repeat(64);
        let (_dir, socket, task) =
            engine(vec![version(), identity(&cid), (204, String::new())]).await;
        let freeze = OwnedEngineFreeze::connect(&cid, socket, deadline())
            .await
            .unwrap();
        freeze.freeze(deadline()).await.unwrap();
        let requests = task.await.unwrap();
        assert_eq!(
            requests,
            [
                "GET /version HTTP/1.1".to_owned(),
                format!("GET /v1.44/containers/{cid}/json HTTP/1.1"),
                format!("POST /v1.44/containers/{cid}/kill?signal=STOP HTTP/1.1")
            ]
        );
    }

    #[tokio::test]
    async fn foreign_identity_and_unsupported_api_never_send_a_signal() {
        let cid = "a".repeat(64);
        let (_dir, socket, task) = engine(vec![version(), identity(&"b".repeat(64))]).await;
        assert!(
            OwnedEngineFreeze::connect(&cid, socket, deadline())
                .await
                .is_err()
        );
        let requests = task.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|request| !request.starts_with("POST")));
        let (_dir, socket, task) = engine(vec![(
            200,
            json!({"ApiVersion":"1.53","MinAPIVersion":"1.45"}).to_string(),
        )])
        .await;
        assert!(
            OwnedEngineFreeze::connect(&cid, socket, deadline())
                .await
                .is_err()
        );
        assert_eq!(task.await.unwrap(), ["GET /version HTTP/1.1"]);
    }

    #[tokio::test]
    async fn engine_refusal_and_deadline_cannot_pass_as_interruption() {
        let cid = "a".repeat(64);
        let (_dir, socket, task) =
            engine(vec![version(), identity(&cid), (409, "not running".into())]).await;
        let freeze = OwnedEngineFreeze::connect(&cid, socket, deadline())
            .await
            .unwrap();
        assert!(
            freeze
                .freeze(deadline())
                .await
                .unwrap_err()
                .to_string()
                .contains("409")
        );
        task.await.unwrap();
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("timeout.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let stalled = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let result = OwnedEngineFreeze::connect(
            &cid,
            socket,
            tokio::time::Instant::now() + Duration::from_millis(30),
        )
        .await;
        assert!(result.is_err());
        stalled.abort();
    }

    #[test]
    fn invalid_cids_and_nonlocal_endpoints_refuse_before_connection() {
        assert_eq!(
            socket_path("unix:///tmp/owned%20engine.sock").unwrap(),
            PathBuf::from("/tmp/owned engine.sock")
        );
        for endpoint in [
            "tcp://localhost:2375",
            "ssh://host",
            "unix://foreign/tmp/engine.sock",
            "unix:///tmp/engine.sock?x=1",
            "unix:///tmp/engine.sock#foreign",
            "unix:relative/engine.sock",
        ] {
            assert!(socket_path(endpoint).is_err());
        }
    }
    #[tokio::test]
    async fn malformed_cid_fails_before_any_engine_connection() {
        let directory = tempfile::tempdir().unwrap();
        let socket = directory.path().join("engine.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        for cid in ["short", &"A".repeat(64), &"a/".repeat(32)] {
            assert!(
                OwnedEngineFreeze::connect(cid, socket.clone(), deadline())
                    .await
                    .is_err()
            );
        }
        assert!(
            tokio::time::timeout(Duration::from_millis(20), listener.accept())
                .await
                .is_err()
        );
    }

    #[test]
    fn frozen_claim_requires_the_actual_owner_operation_input_and_live_lease() {
        use veoveo_types::TaskTypeDefinition;
        use veoveo_view_mcp::contract::{SceneCompositionAuthority, ViewTaskKind};
        let principal = PrincipalId::parse("https://smoke.veoveo.local#view-smoke-a").unwrap();
        let work_context = WorkContextId::parse("smoke").unwrap();
        let tenant = TenantId::parse("local").unwrap();
        let authority = InvocationAuthority {
            work_context: work_context.clone(),
            tenant: tenant.clone(),
            membership: WorkContextMembershipLevel::Owner,
            policy_revision: PolicyVersion::parse("r1").unwrap(),
            output_policy: WorkContextOutputPolicy {
                owner: AccessSubject::Principal(principal.clone()),
                initial_grants: Vec::new(),
                classification: None,
                data_labels: BTreeSet::new(),
            },
            provenance: InvocationProvenance::Automated,
        };
        let now = Utc::now();
        let composition = SceneComposition::new(
            composition_request(LOCAL_LAYER, false).unwrap(),
            SceneCompositionAuthority {
                principal_id: principal.clone(),
                invocation: authority,
            },
            now,
        )
        .unwrap();
        let view = ViewRecord::new(
            ViewId::parse("view-1").unwrap(),
            &composition,
            local_camera(),
            now,
        )
        .unwrap();
        let request = capture_request("view-1", 1, false).unwrap();
        let expected = CaptureExpectation::new(
            &request,
            &serde_json::to_value(&view).unwrap(),
            &serde_json::to_value(&composition).unwrap(),
        )
        .unwrap();
        let input = json!({"request":request,"viewSnapshot":{"view":view,"composition":{"record":composition}}});
        let task = veoveo_types::TaskId::new();
        let id = veoveo_platform_store::task_record_id(task);
        let server = surrealdb::types::RecordId::new("mcp_server", "view");
        let operation = ViewTaskKind::CaptureFrame.name();
        let claim = FrozenClaim {
            id: &id,
            server: &server,
            operation: &operation,
            principal: &principal,
            work_context: &work_context,
            tenant: &tenant,
            status: veoveo_platform_store::TaskStatus::Running,
            lease_owner: Some("actual-process-epoch"),
            lease_expiry: Some(now + TimeDelta::seconds(180)),
            settled: false,
            input: &input,
        };
        expected.admit_claim(claim.clone(), task).unwrap();
        let mut invalid = claim.clone();
        invalid.status = veoveo_platform_store::TaskStatus::Succeeded;
        assert!(expected.admit_claim(invalid, task).is_err());
        let mut invalid = claim.clone();
        invalid.lease_owner = None;
        assert!(expected.admit_claim(invalid, task).is_err());
        let mut invalid = claim.clone();
        invalid.lease_expiry = Some(now - TimeDelta::seconds(1));
        assert!(expected.admit_claim(invalid, task).is_err());
        let mut invalid = claim.clone();
        invalid.settled = true;
        assert!(expected.admit_claim(invalid, task).is_err());
        let foreign = PrincipalId::parse("https://smoke.veoveo.local#view-smoke-b").unwrap();
        let mut invalid = claim.clone();
        invalid.principal = &foreign;
        assert!(expected.admit_claim(invalid, task).is_err());
        let foreign_server = surrealdb::types::RecordId::new("mcp_server", "foreign");
        let mut invalid = claim.clone();
        invalid.server = &foreign_server;
        assert!(expected.admit_claim(invalid, task).is_err());
        let foreign_operation = veoveo_types::TaskTypeName::from_static("foreign_operation");
        let mut invalid = claim.clone();
        invalid.operation = &foreign_operation;
        assert!(expected.admit_claim(invalid, task).is_err());
        assert!(
            expected
                .admit_claim(claim.clone(), veoveo_types::TaskId::new())
                .is_err()
        );
        let mut changed = input.clone();
        changed["request"]["expectedRevision"] = json!(99);
        let mut invalid = claim;
        invalid.input = &changed;
        assert!(expected.admit_claim(invalid, task).is_err());
    }
    #[test]
    fn active_context_overrides_host_and_empty_context_preserves_host() {
        assert_eq!(
            endpoint_override(Some("unix:///host.sock".into()), None),
            Some("unix:///host.sock".into())
        );
        assert_eq!(
            endpoint_override(
                Some("unix:///host.sock".into()),
                Some("named-context".into())
            ),
            None
        );
        assert_eq!(
            endpoint_override(Some("unix:///host.sock".into()), Some(String::new())),
            Some("unix:///host.sock".into())
        );
        assert_eq!(endpoint_override(Some(String::new()), None), None);
    }
    #[test]
    fn unix_uri_decodes_space_percent_and_reserved_path_components_once() {
        for (endpoint, expected) in [
            ("unix:///tmp/a%20b/docker.sock", "/tmp/a b/docker.sock"),
            ("unix:///tmp/a%25b/docker.sock", "/tmp/a%b/docker.sock"),
            ("unix:///tmp/a%2520b/docker.sock", "/tmp/a%20b/docker.sock"),
            ("unix:///tmp/a%23%3Fb/docker.sock", "/tmp/a#?b/docker.sock"),
        ] {
            let actual = socket_path(endpoint).unwrap();
            assert_eq!(actual, PathBuf::from(expected));
            let file = url::Url::from_file_path(&actual).unwrap();
            let mut round_trip = url::Url::parse("unix:///").unwrap();
            round_trip.set_path(file.path());
            assert_eq!(socket_path(round_trip.as_str()).unwrap(), actual);
        }
    }

    #[tokio::test]
    async fn encoded_endpoint_connects_to_the_actual_reserved_name_unix_socket() {
        let cid = "a".repeat(64);
        let (_dir, actual, task) = engine_named(
            vec![version(), identity(&cid), (204, String::new())],
            "a % # ?.sock",
        )
        .await;
        let file = url::Url::from_file_path(&actual).unwrap();
        let mut endpoint = url::Url::parse("unix:///").unwrap();
        endpoint.set_path(file.path());
        let admitted = socket_path(endpoint.as_str()).unwrap();
        assert_eq!(admitted, actual);
        let freeze = OwnedEngineFreeze::connect(&cid, admitted, deadline())
            .await
            .unwrap();
        freeze.freeze(deadline()).await.unwrap();
        assert_eq!(task.await.unwrap().len(), 3);
    }
}
