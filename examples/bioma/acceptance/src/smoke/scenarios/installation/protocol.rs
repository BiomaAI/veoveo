//! Selected installed CPU protocol checks and their caller-owned cleanup.
use super::*;
use serde::{Deserialize, Serialize};
use veoveo_gateway_contract::GatewayToolName;
use veoveo_types::{PromptName, ResourceScheme, ResourceUri, ServerSlug, Sha256Digest};
#[path = "protocol/admin.rs"]
mod admin;
#[path = "protocol/authentication.rs"]
mod authentication;
#[path = "protocol/discovery.rs"]
mod discovery;
#[path = "protocol/evidence.rs"]
mod evidence;
#[path = "protocol/transport.rs"]
mod transport;
use discovery::SelectedOwner;
use evidence::{Evidence, Method, Phase, Target};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkloadIdentity {
    deployment_uid: uuid::Uuid,
    pod_uid: uuid::Uuid,
    image_digest: Sha256Digest,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SelectedServer {
    owner: SelectedOwner,
    slug: ServerSlug,
    scheme: ResourceScheme,
    mount_path: veoveo_mcp_contract::MountPath,
    origin: url::Url,
    allowed_host: String,
    workload: WorkloadIdentity,
    #[serde(skip)]
    expected_tools: BTreeSet<GatewayToolName>,
    #[serde(skip)]
    expected_prompts: BTreeSet<PromptName>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProtocolFixture {
    schema: FixtureSchema,
    context: String,
    namespace: String,
    control_plane_sha256: Sha256Digest,
    gateway: WorkloadIdentity,
    servers: Vec<SelectedServer>,
}
#[derive(Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum FixtureSchema {
    #[vocabulary(rename = "veoveo.ai/installed-protocol-fixture/v1")]
    V1,
}
fn exposed<T: PartialEq>(exposure: &veoveo_mcp_contract::Exposure<T>, value: &T) -> bool {
    match exposure {
        veoveo_mcp_contract::Exposure::All => true,
        veoveo_mcp_contract::Exposure::Listed(values) => values.contains(value),
        veoveo_mcp_contract::Exposure::None => false,
    }
}
fn admit_operator_surface(
    catalog: &veoveo_mcp_gateway::GatewayCatalog,
    installation: &InstalledTarget,
) -> Result<()> {
    let profile = catalog
        .profile(&installation.operator.profile)
        .context("operator profile is not registered")?;
    let client = catalog
        .profile_oauth_clients(profile)
        .into_iter()
        .find(|client| client.id == installation.operator.client_id)
        .context("operator client is not registered for the selected profile")?;
    admit_full_mcp_client(client)
}
fn admit_full_mcp_client(client: &veoveo_mcp_contract::OAuthClientRegistration) -> Result<()> {
    ensure!(
        client.client_surface == veoveo_mcp_contract::OAuthClientSurface::FullMcp,
        "installed protocol requires a FullMcp operator client"
    );
    Ok(())
}
fn admit_origin(origin: &url::Url) -> Result<()> {
    ensure!(
        origin.scheme() == "http"
            && origin.host().is_some_and(|host| match host {
                url::Host::Ipv4(ip) => ip.is_loopback(),
                url::Host::Ipv6(ip) => ip.is_loopback(),
                url::Host::Domain(_) => false,
            })
            && origin.port().is_some_and(|port| port != 0),
        "direct origin must be an explicit loopback HTTP port"
    );
    ensure!(
        origin.username().is_empty()
            && origin.password().is_none()
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none(),
        "direct origin contains unsupported components"
    );
    Ok(())
}
fn fixture(
    path: &Path,
    installation_path: &Path,
    installation: &InstalledTarget,
) -> Result<(ProtocolFixture, veoveo_mcp_gateway::GatewayCatalog)> {
    use sha2::{Digest, Sha256};
    use std::{io::Read, os::unix::fs::MetadataExt};
    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.mode() & 0o077 == 0 && metadata.len() <= 65536,
        "protocol fixture must be a private regular file at most64KiB"
    );
    let mut bytes = Vec::new();
    file.by_ref().take(65537).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 65536, "protocol fixture exceeds64KiB");
    let mut fixture: ProtocolFixture = serde_json::from_slice(&bytes)
        .map_err(|_| anyhow!("protocol fixture typed admission failed"))?;
    ensure!(
        fixture.context == installation.target.kubernetes.context
            && fixture.namespace == installation.target.kubernetes.namespace,
        "fixture selects another installation"
    );
    let control_path = installation.target.control_plane_path(installation_path);
    let control_bytes = fs::read(&control_path)?;
    ensure!(
        fixture.control_plane_sha256
            == Sha256Digest::from_bytes(Sha256::digest(&control_bytes).into()),
        "fixture control-plane identity differs"
    );
    let admission = veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
        .bind(veoveo_gateway_catalog::registry()?)?;
    let catalog = veoveo_mcp_gateway::GatewayCatalog::load_json(control_path, admission)?;
    admit_operator_surface(&catalog, installation)?;
    let mut seen = BTreeSet::new();
    let mut origins = BTreeSet::new();
    for server in &mut fixture.servers {
        ensure!(seen.insert(server.owner), "fixture repeats an owner");
        admit_origin(&server.origin)?;
        ensure!(
            origins.insert(server.origin.clone()),
            "fixture repeats a direct origin"
        );
        ensure!(
            !server.workload.deployment_uid.is_nil() && !server.workload.pod_uid.is_nil(),
            "fixture requires workload UIDs"
        );
        let (_, exposure, manifest) = catalog
            .profile_server(&installation.operator.profile, &server.slug)
            .context("selected server not exposed by operator profile")?;
        ensure!(
            server.slug == server.owner.slug()
                && server.scheme == server.owner.scheme()
                && manifest.uri_scheme == server.scheme
                && manifest.mount_path == server.mount_path,
            "selected server profile does not match owner"
        );
        let expected_host = url::Url::parse(manifest.upstream.url.as_str())?
            .authority()
            .to_owned();
        ensure!(
            server.allowed_host == expected_host,
            "direct Host must match admitted upstream authority"
        );
        server.expected_tools = manifest
            .tools
            .iter()
            .filter(|tool| {
                exposed(&exposure.tools, tool) && !manifest.compatibility_helpers.contains(tool)
            })
            .map(|tool| GatewayToolName::from_parts(&server.slug, tool))
            .collect::<std::result::Result<_, _>>()?;
        ensure!(
            !server.expected_tools.is_empty(),
            "selected owner requires exposed tools"
        );
        server.expected_prompts = manifest
            .prompts
            .iter()
            .filter(|prompt| exposed(&exposure.prompts, prompt))
            .cloned()
            .collect();
    }
    ensure!(
        seen == SelectedOwner::ALL.iter().copied().collect(),
        "fixture must select exactly four CPU owners"
    );
    ensure!(
        !fixture.gateway.deployment_uid.is_nil() && !fixture.gateway.pod_uid.is_nil(),
        "fixture requires gateway UIDs"
    );
    Ok((fixture, catalog))
}

fn admit_identities(installation: &InstalledTarget) -> Result<()> {
    let administrator = installation.administrator()?;
    ensure!(
        administrator.client_id != installation.operator.client_id
            && administrator.principal != installation.operator.principal
            && administrator.profile != installation.operator.profile
            && administrator.resource != installation.operator.resource,
        "protocol requires distinct admitted operator and administrator identities"
    );
    Ok(())
}

pub(crate) async fn run(installation_path: &Path, input_path: &Path, output: &Path) -> Result<()> {
    let installation = InstalledTarget::load(installation_path)?;
    admit_identities(&installation)?;
    let administrator = installation.administrator()?;
    let (fixture, catalog) = fixture(input_path, installation_path, &installation)?;
    let mut evidence = Evidence::create(output)?;
    evidence.admitted(fixture.clone())?;
    let mut operator_client = None;
    let mut admin_client = None;
    let mut curl = transport::Curl::default();
    let mut all_qualified = false;
    let mut admin_receipt = admin::AdminReceipt::admit(&installation)?;
    evidence.administrator(&admin_receipt)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    let operation = tokio::time::timeout_at(deadline, async {
        let operator_token = tokio::time::timeout(Duration::from_secs(15), installation.token())
            .await
            .map_err(|_| anyhow!("operator OAuth deadline"))?
            .map_err(|_| anyhow!("operator OAuth failed"))?;
        let admin_token = tokio::time::timeout(Duration::from_secs(15), administrator.token())
            .await
            .map_err(|_| anyhow!("administrator OAuth deadline"))?
            .map_err(|_| anyhow!("administrator OAuth failed"))?;
        operator_client = Some(
            tokio::time::timeout(
                Duration::from_secs(15),
                connect_mcp_client(installation.operator.resource.as_str(), &operator_token),
            )
            .await
            .map_err(|_| anyhow!("operator MCP deadline"))?
            .map_err(|_| anyhow!("operator MCP connection failed"))?,
        );
        admin_client = Some(
            tokio::time::timeout(
                Duration::from_secs(15),
                connect_mcp_client(administrator.resource.as_str(), &admin_token),
            )
            .await
            .map_err(|_| anyhow!("administrator MCP deadline"))?
            .map_err(|_| anyhow!("administrator MCP connection failed"))?,
        );
        authentication::run(
            &url::Url::parse(installation.operator.resource.as_str())?,
            &mut evidence,
        )
        .await?;
        curl.run(&fixture.servers, &mut evidence).await?;
        discovery::run(
            operator_client.as_ref().expect("owned operator"),
            &fixture.servers,
            &mut evidence,
        )
        .await?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(15))
            .build()
            .map_err(|_| anyhow!("protocol HTTPS client construction failed"))?;
        discovery::admin_documents(
            &http,
            &admin_token,
            &installation,
            &fixture.servers,
            &mut evidence,
        )
        .await?;
        let healthy = fixture
            .servers
            .iter()
            .map(|server| server.slug.clone())
            .collect::<BTreeSet<_>>();
        let expected = catalog
            .control_plane()
            .servers
            .iter()
            .map(|server| server.slug.clone())
            .collect();
        admin::run(
            admin::AdminInput {
                installation: &installation,
                client: &http,
                operator_token: &operator_token,
                admin_token: &admin_token,
                expected_servers: &expected,
                healthy_servers: &healthy,
                deadline,
            },
            &mut admin_receipt,
            |receipt| evidence.administrator(receipt),
        )
        .await?;
        evidence.administrator(&admin_receipt)?;
        ensure!(
            admin_receipt.failure.is_none(),
            "administrator checks failed; see receipt"
        );
        let profile = catalog
            .profile(&administrator.profile)
            .context("administrator profile missing")?;
        let mut prompt_profile = discovery::AdminPromptProfile {
            mode: profile.discovery_failure_mode,
            attempted_servers: BTreeSet::new(),
            healthy_servers: healthy,
            healthy_prompts: BTreeSet::new(),
        };
        for (exposure, server) in catalog.profile_servers(&administrator.profile) {
            if server.capabilities.prompts
                && !matches!(exposure.prompts, veoveo_mcp_contract::Exposure::None)
            {
                prompt_profile.attempted_servers.insert(server.slug.clone());
                if prompt_profile.healthy_servers.contains(&server.slug) {
                    prompt_profile.healthy_prompts.extend(
                        server
                            .prompts
                            .iter()
                            .filter(|prompt| exposed(&exposure.prompts, prompt))
                            .cloned(),
                    );
                }
            }
        }
        let isolation = discovery::prompt_isolation(
            admin_client.as_ref().expect("owned administrator"),
            &prompt_profile,
            &mut evidence,
        )
        .await?;
        all_qualified = matches!(
            isolation.status,
            discovery::PromptIsolationStatus::Qualified
        );
        evidence.isolation(isolation)?;
        Ok::<_, anyhow::Error>(())
    })
    .await;
    let timed_out = operation.is_err();
    let result = operation
        .map_err(|_| anyhow!("installed protocol aggregate deadline"))
        .and_then(std::convert::identity);
    if timed_out {
        admin_receipt.timed_out();
    }
    let admin_snapshot = evidence.administrator(&admin_receipt);
    let process_closed = curl.close().await;
    let operator_closed = close_client(operator_client.take()).await;
    let admin_closed = close_client(admin_client.take()).await;
    let cleanup = process_closed && operator_closed && admin_closed;
    evidence.settled(result.is_ok(), cleanup, all_qualified, timed_out)?;
    admin_snapshot?;
    result?;
    ensure!(cleanup, "installed protocol cleanup unresolved");
    ensure!(
        all_qualified,
        "installed protocol has an unqualified required observation; see receipt"
    );
    Ok(())
}
async fn close_client(client: Option<SmokeMcpClient>) -> bool {
    match client {
        None => true,
        Some(client) => matches!(
            tokio::time::timeout(Duration::from_secs(5), client.cancel()).await,
            Ok(Ok(_))
        ),
    }
}

#[cfg(test)]
mod protocol_fixture_tests {
    use super::*;
    #[test]
    fn direct_origins_refuse_remote_credentials_paths_and_implicit_ports() -> Result<()> {
        for url in ["http://127.0.0.1:18001/", "http://[::1]:18001/"] {
            admit_origin(&url::Url::parse(url)?)?;
        }
        for url in [
            "https://127.0.0.1:18001/",
            "http://example.org:18001/",
            "http://127.0.0.1/",
            "http://user@127.0.0.1:18001/",
            "http://127.0.0.1:18001/escape",
            "http://127.0.0.1:18001/?secret=value",
            "http://127.0.0.1:18001/#fragment",
        ] {
            assert!(admit_origin(&url::Url::parse(url)?).is_err(), "{url}");
        }
        Ok(())
    }
    #[test]
    fn missing_administrator_is_rejected_during_input_admission() -> Result<()> {
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let target = repository.join("examples/bioma/installation-target-initial.json");
        let mut input: serde_json::Value = serde_json::from_slice(&fs::read(&target)?)?;
        input
            .as_object_mut()
            .expect("target object")
            .remove("administrator");
        input["controlPlane"] = serde_json::json!("gateway.json");
        let directory = tempfile::tempdir()?;
        fs::copy(
            repository.join("examples/bioma/gateway.json"),
            directory.path().join("gateway.json"),
        )?;
        let path = directory.path().join("target.json");
        fs::write(&path, serde_json::to_vec(&input)?)?;
        let installed = InstalledTarget::load(&path)?;
        assert!(admit_identities(&installed).is_err());
        Ok(())
    }
    #[test]
    fn exact_fixture_and_profile_bindings_reject_forged_host_owner_and_control_identity()
    -> Result<()> {
        use sha2::{Digest, Sha256};
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        let target = repository.join("examples/bioma/installation-target-initial.json");
        let installation = InstalledTarget::load(&target)?;
        let control_bytes = fs::read(installation.target.control_plane_path(&target))?;
        let admission = veoveo_mcp_gateway::GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry()?)?;
        let catalog = veoveo_mcp_gateway::GatewayCatalog::load_json(
            installation.target.control_plane_path(&target),
            admission,
        )?;
        let identity = || WorkloadIdentity {
            deployment_uid: uuid::Uuid::new_v4(),
            pod_uid: uuid::Uuid::new_v4(),
            image_digest: Sha256Digest::from_bytes([1; 32]),
        };
        let mut input = ProtocolFixture {
            schema: FixtureSchema::V1,
            context: installation.target.kubernetes.context.clone(),
            namespace: installation.target.kubernetes.namespace.clone(),
            control_plane_sha256: Sha256Digest::from_bytes(Sha256::digest(&control_bytes).into()),
            gateway: identity(),
            servers: Vec::new(),
        };
        for (index, owner) in SelectedOwner::ALL.iter().copied().enumerate() {
            let manifest = catalog.server(&owner.slug()).expect("reference owner");
            input.servers.push(SelectedServer {
                owner,
                slug: owner.slug(),
                scheme: owner.scheme(),
                mount_path: manifest.mount_path.clone(),
                origin: url::Url::parse(&format!("http://127.0.0.1:{}/", 18001 + index))?,
                allowed_host: url::Url::parse(manifest.upstream.url.as_str())?
                    .authority()
                    .to_owned(),
                workload: identity(),
                expected_tools: BTreeSet::new(),
                expected_prompts: BTreeSet::new(),
            });
        }
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("fixture.json");
        let save = |input: &ProtocolFixture| -> Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&path)?;
            serde_json::to_writer(&mut file, input)?;
            file.flush()?;
            Ok(())
        };
        save(&input)?;
        let (admitted, _) = fixture(&path, &target, &installation)?;
        assert!(
            admitted
                .servers
                .iter()
                .all(|server| !server.expected_tools.is_empty())
        );
        let media = admitted
            .servers
            .iter()
            .find(|server| server.owner == SelectedOwner::Media)
            .expect("required Media owner");
        assert_eq!(
            media.expected_tools,
            BTreeSet::from([GatewayToolName::parse("media__run")?])
        );
        let manifest = catalog.server(&media.slug).expect("checked Media manifest");
        assert_eq!(manifest.compatibility_helpers.len(), 3);
        for helper in &manifest.compatibility_helpers {
            assert!(
                !media
                    .expected_tools
                    .contains(&GatewayToolName::from_parts(&media.slug, helper)?)
            );
        }
        let profile = catalog
            .profile(&installation.operator.profile)
            .expect("checked profile");
        let mut client = catalog
            .profile_oauth_clients(profile)
            .into_iter()
            .find(|client| client.id == installation.operator.client_id)
            .expect("checked client")
            .clone();
        admit_full_mcp_client(&client)?;
        client.client_surface = veoveo_mcp_contract::OAuthClientSurface::ToolsCompat;
        assert!(admit_full_mcp_client(&client).is_err());
        let mut wrong_identity = InstalledTarget::load(&target)?;
        wrong_identity.operator.client_id = wrong_identity.administrator()?.client_id.clone();
        assert!(admit_operator_surface(&catalog, &wrong_identity).is_err());
        let original = input.servers[0].allowed_host.clone();
        input.servers[0].allowed_host = "outside.invalid".into();
        save(&input)?;
        assert!(fixture(&path, &target, &installation).is_err());
        input.servers[0].allowed_host = original;
        input.servers[0].owner = SelectedOwner::Media;
        save(&input)?;
        assert!(fixture(&path, &target, &installation).is_err());
        input.servers[0].owner = SelectedOwner::DuckDb;
        input.control_plane_sha256 = Sha256Digest::from_bytes([2; 32]);
        save(&input)?;
        assert!(fixture(&path, &target, &installation).is_err());
        Ok(())
    }
}
