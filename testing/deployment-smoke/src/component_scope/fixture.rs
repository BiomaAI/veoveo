use super::Args;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use veoveo_deploy_contract::{
    DeploymentLock, LoadedProfile, LockedImage, LockedSource, RegistryTransport,
    components::ComponentId,
};
use veoveo_deploy_runtime::{
    ComponentUpdates, compile_component_lock, lock_source_charts, update_components,
};
use veoveo_extension_contract::{ArtifactDigest, SourceRevision};

pub(super) struct Fixture {
    pub profile: LoadedProfile,
    pub lock: DeploymentLock,
    pub directory: PathBuf,
    sources: BTreeMap<String, PathBuf>,
    repository: PathBuf,
    namespace: String,
    push_registry: String,
    _builder: veoveo_image_build_control::BuilderLease,
}

pub(super) fn output(command: &mut Command) -> Result<Vec<u8>> {
    let output = command.output()?;
    ensure!(
        output.status.success(),
        "fixture command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}
fn git(root: &Path, args: &[&str]) -> Result<String> {
    Ok(
        String::from_utf8(output(Command::new("git").args(args).current_dir(root))?)?
            .trim()
            .into(),
    )
}
fn initialize(root: &Path) -> Result<()> {
    fs::create_dir_all(root)?;
    git(root, &["init", "--quiet"])?;
    git(
        root,
        &["config", "user.name", "Veoveo deployment scope fixture"],
    )?;
    git(root, &["config", "user.email", "scope@example.invalid"])?;
    git(
        root,
        &[
            "config",
            "remote.origin.url",
            &format!("file://{}", root.display()),
        ],
    )?;
    Ok(())
}
fn commit(root: &Path, message: &str) -> Result<String> {
    git(root, &["add", "."])?;
    git(
        root,
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    )?;
    git(root, &["rev-parse", "HEAD"])
}
fn target(source: &str) -> &str {
    if source == "platform" {
        "artifact-service"
    } else {
        "extension"
    }
}

impl Fixture {
    pub fn create(args: &Args, namespace: &str, context: &str, directory: &Path) -> Result<Self> {
        ensure!(
            !args.base_image.chars().any(char::is_whitespace),
            "fixture base image contains whitespace"
        );
        ArtifactDigest::new(
            args.base_image
                .split_once('@')
                .context("fixture base image must be digest-pinned")?
                .1,
        )?;
        fs::create_dir_all(directory)?;
        let repository = fs::canonicalize(&args.repository)?;
        let builder = veoveo_image_build_control::ensure_for_registry(
            &repository,
            &args.push_registry,
            RegistryTransport::InsecureHttp,
        )?;
        let mut roots = BTreeMap::new();
        for name in ["platform", "extension"] {
            let root = directory.join(name);
            initialize(&root)?;
            fs::create_dir_all(root.join("chart/templates"))?;
            fs::write(
                root.join("Dockerfile"),
                format!(
                    "FROM {}\nCOPY revision.txt /fixture-revision.txt\nCMD [\"/bin/sleep\",\"600\"]\n",
                    args.base_image
                ),
            )?;
            fs::write(root.join(".dockerignore"), ".git\nchart\n")?;
            fs::write(root.join("revision.txt"), format!("{name}-1\n"))?;
            fs::write(
                root.join("chart/Chart.yaml"),
                format!("apiVersion: v2\nname: {name}\nversion: 1.0.0\n"),
            )?;
            let (registry, images) = if name == "platform" {
                (
                    ".Values.global.veoveoRegistry",
                    ".Values.global.imageDigests",
                )
            } else {
                (".Values.veoveo.registry", ".Values.veoveo.imageDigests")
            };
            fs::write(
                root.join("chart/templates/deployment.yaml"),
                format!(
                    r#"apiVersion: apps/v1
kind: Deployment
metadata:
  name: {name}
spec:
  replicas: 1
  selector:
    matchLabels: {{app: {name}}}
  template:
    metadata:
      labels: {{app: {name}}}
    spec:
      terminationGracePeriodSeconds: 2
      containers:
        - name: witness
          image: "{{{{ {registry} }}}}/{namespace}/{}@{{{{ index {images} "{namespace}/{}" }}}}"
          command: ["/bin/sleep", "600"]
"#,
                    target(name),
                    target(name)
                ),
            )?;
            commit(&root, "initial independent fixture source")?;
            roots.insert(name.into(), root);
        }
        let installation = directory.join("installation");
        initialize(&installation)?;
        let profile_path = installation.join("deployment.json");
        let definition = json!({
            "schemaVersion":"veoveo.io/deployment/v7", "name":"scope-fixture",
            "registry":{"pushAddress":args.push_registry,"pullAddress":args.pull_registry,"transport":"insecure-http"},
            "namespace":namespace,"kubernetes":{"context":context,"localCluster":null},
            "sources":[
                {"name":"platform","role":"platform","repository":{"kind":"local","path":"../platform"},"revision":"HEAD","imageGroups":[],
                    "releases":[{"name":"platform","chart":"chart","sourceValues":[],"installationValues":[],"valuesContract":"platform","timeoutSeconds":60}]},
                {"name":"extension","role":"extension","repository":{"kind":"local","path":"../extension"},"revision":"HEAD","imageGroups":["extension"],
                    "releases":[{"name":"extension","chart":"chart","sourceValues":[],"installationValues":[],"valuesContract":"extension","timeoutSeconds":60}]}
            ],
            "components":[
                {"id":"installation","owner":{"kind":"installation"},"role":"installation","dependencies":[],"namespaces":[namespace],
                    "clusterObjects":[{"group":"","kind":"Namespace","namespace":null,"name":namespace}],"releases":[],"installationInputs":["namespace"],"extensionRelease":null},
                {"id":"platform","owner":{"kind":"source","name":"platform"},"role":"platform","dependencies":["installation"],"namespaces":[namespace],
                    "clusterObjects":[],"releases":["platform"],"installationInputs":[],"extensionRelease":null},
                {"id":"extension","owner":{"kind":"source","name":"extension"},"role":"extension","dependencies":["installation"],"namespaces":[namespace],
                    "clusterObjects":[],"releases":["extension"],"installationInputs":[],"extensionRelease":{"extension":"scope.example","version":"1.0.0","manifestDigest":format!("sha256:{}", "a".repeat(64))}}
            ],
            "platform":{"installationPreset":"custom","components":["platform-store","object-store","artifact-service"],"mcpServers":[],"artifactAudiences":[],"externalWorkloads":[]},
            "resources":{"manifests":[],"configMaps":[]},"gatewayRequirements":[],"waitForDeployments":["platform","extension"]
        });
        fs::write(&profile_path, serde_json::to_vec_pretty(&definition)?)?;
        let revision = commit(&installation, "installation ownership and configuration")?;
        let profile = LoadedProfile::load(&profile_path, &installation)?;
        let mut sources = Vec::new();
        for source in &profile.definition.sources {
            let root = &roots[&source.name];
            sources.push(LockedSource {
                name: source.name.clone(),
                role: source.role,
                repository: format!("file://{}", root.display()),
                revision: git(root, &["rev-parse", "HEAD"])?,
                images: Vec::new(),
                charts: lock_source_charts(source, root)?,
            });
        }
        let lock = DeploymentLock {
            schema_version: "veoveo.io/deployment-lock/v7".into(),
            profile: profile.definition.name.clone(),
            profile_revision: revision,
            registry: profile.definition.registry.locked(),
            sources,
            components: Vec::new(),
            platform: profile.resolved_platform()?,
        };
        let mut fixture = Self {
            profile,
            lock,
            directory: directory.into(),
            sources: roots,
            repository,
            namespace: namespace.into(),
            push_registry: args.push_registry.clone(),
            _builder: builder,
        };
        for name in ["platform", "extension"] {
            let targets = if name == "platform" {
                fixture.profile.required_platform_images()?
            } else {
                BTreeSet::from([target(name).into()])
            };
            let images = fixture.build(name, &targets, 1)?;
            fixture
                .lock
                .sources
                .iter_mut()
                .find(|source| source.name == name)
                .unwrap()
                .images = images;
        }
        fixture.lock.components = compile_component_lock(
            &fixture.profile,
            &fixture.lock.profile_revision,
            &fixture.lock.sources,
            &fixture.sources,
        )?;
        fixture.lock.validate()?;
        Ok(fixture)
    }

    fn build(
        &self,
        source: &str,
        targets: &BTreeSet<String>,
        version: u8,
    ) -> Result<Vec<LockedImage>> {
        let root = &self.sources[source];
        let revision = git(root, &["rev-parse", "HEAD"])?;
        let metadata = self
            .directory
            .join(format!("{source}-{version}-image.json"));
        let mut command = veoveo_image_build_control::buildx_command(&self.repository)?;
        command
            .args([
                "build",
                "--builder",
                veoveo_image_build_control::BUILDER_NAME,
                "--platform=linux/amd64",
                "--provenance=mode=max",
                "--output=type=image,push=true,registry.insecure=true",
                "--metadata-file",
            ])
            .arg(&metadata);
        for target in targets {
            command.args([
                "--tag",
                &format!(
                    "{}/{}/{target}:v{version}",
                    self.push_registry, self.namespace
                ),
            ]);
        }
        let status = command.arg(root).status()?;
        ensure!(status.success(), "building immutable scope fixture failed");
        #[derive(Deserialize)]
        struct Metadata {
            #[serde(rename = "containerimage.digest")]
            digest: String,
        }
        let metadata: Metadata = serde_json::from_slice(&fs::read(metadata)?)?;
        ArtifactDigest::new(&metadata.digest)?;
        let first = targets.first().context("image build has no targets")?;
        let published = format!(
            "{}/{}/{first}@{}",
            self.push_registry, self.namespace, metadata.digest
        );
        let raw = output(
            veoveo_image_build_control::buildx_command(&self.repository)?.args([
                "imagetools",
                "inspect",
                "--raw",
                &published,
            ]),
        )?;
        #[derive(Deserialize)]
        struct Index {
            manifests: Vec<Descriptor>,
        }
        #[derive(Deserialize)]
        struct Descriptor {
            digest: String,
            platform: Option<Platform>,
        }
        #[derive(Deserialize)]
        struct Platform {
            os: String,
            architecture: String,
        }
        let index: Index = serde_json::from_slice(&raw)?;
        let runnable = index
            .manifests
            .iter()
            .filter(|descriptor| {
                descriptor.platform.as_ref().is_some_and(|platform| {
                    platform.os == "linux" && platform.architecture == "amd64"
                })
            })
            .collect::<Vec<_>>();
        ensure!(
            runnable.len() == 1,
            "publication has no unique runnable linux/amd64 manifest"
        );
        ArtifactDigest::new(&runnable[0].digest)?;
        targets
            .iter()
            .map(|target| {
                Ok(LockedImage {
                    name: target.clone(),
                    repository: format!(
                        "{}/{}/{target}",
                        self.profile.definition.registry.pull_address, self.namespace
                    ),
                    source_revision: SourceRevision::new(&revision)?,
                    digest: runnable[0].digest.clone(),
                    publication_digest: metadata.digest.clone(),
                })
            })
            .collect()
    }

    pub fn advance(&mut self, name: &str) -> Result<()> {
        let source = &self.sources[name];
        fs::write(source.join("revision.txt"), format!("{name}-2\n"))?;
        commit(source, "one independent image edit")?;
        let images = self.build(name, &BTreeSet::from([target(name).into()]), 2)?;
        let previous = self
            .lock
            .components
            .iter()
            .find(|component| component.declaration.id.as_str() == name)
            .unwrap()
            .clone();
        self.lock = update_components(
            &self.profile,
            &self.lock,
            &BTreeSet::from([ComponentId::try_from(name.to_owned())?]),
            &ComponentUpdates {
                images: BTreeMap::from([(name.into(), images)]),
                ..Default::default()
            },
        )?;
        let updated = self
            .lock
            .components
            .iter()
            .find(|component| component.declaration.id.as_str() == name)
            .unwrap();
        ensure!(
            updated.units[0].content_digest != previous.units[0].content_digest,
            "fixture image update did not change deployable content"
        );
        Ok(())
    }

    pub fn lock_file(&self, name: &str) -> Result<PathBuf> {
        let path = self.directory.join(format!("{name}.lock.json"));
        fs::write(&path, serde_json::to_vec_pretty(&self.lock)?)?;
        Ok(path)
    }

    pub fn hide(&self, name: &str) -> Result<HiddenSource> {
        let original = self.sources[name].clone();
        let hidden = original.with_extension("unavailable");
        fs::rename(&original, &hidden)?;
        Ok(HiddenSource { original, hidden })
    }
}

pub(super) struct HiddenSource {
    original: PathBuf,
    hidden: PathBuf,
}
impl Drop for HiddenSource {
    fn drop(&mut self) {
        if let Err(error) = fs::rename(&self.hidden, &self.original) {
            eprintln!("restoring fixture source: {error}");
        }
    }
}
