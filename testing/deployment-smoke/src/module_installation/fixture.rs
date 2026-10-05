//! Namespace-owned public plans, disposable credentials and actual chart Jobs.
use super::{Args, PinnedImage, managed, process};
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::PermissionsExt,
    process::Command,
    thread,
    time::{Duration, Instant},
};
use veoveo_modules::{
    CredentialRevision, InstallationGeneration, ModuleName, ModulePlanDocument,
    ModuleSelectionDocument,
};

pub(super) struct Render {
    pub plan: ModulePlanDocument,
    pub objects: Vec<Value>,
}
// Policies precede executable workloads; selection is shared with native rendering checks.
fn managed_objects(objects: &[Value]) -> impl Iterator<Item = &Value> {
    [
        "ServiceAccount",
        "Role",
        "RoleBinding",
        "ValidatingAdmissionPolicy",
        "ValidatingAdmissionPolicyBinding",
        "NetworkPolicy",
        "ConfigMap",
        "Service",
        "Deployment",
    ]
    .into_iter()
    .flat_map(move |kind| objects.iter().filter(move |object| object["kind"] == kind))
    .filter(|object| {
        !matches!(
            object["metadata"]["labels"]["app.kubernetes.io/component"].as_str(),
            Some("surrealdb" | "module-plan")
        )
    })
}

pub(super) struct Fixture {
    pub namespace: String,
    context: String,
    image: PinnedImage,
    directory: tempfile::TempDir,
    uid: Option<String>,
    root_password: String,
    runtime_password: String,
    serial: u32,
    prior_passwords: Vec<String>,
    pub(super) managed_config: managed::Configuration,
    pub(super) managed: Option<managed::Managed>,
    agent_uid: Option<String>,
    cluster_owned: Vec<(String, String)>,
    kube_version: Option<String>,
    first_probe_failure: Option<String>,
}
#[derive(Deserialize)]
struct Namespace {
    metadata: Metadata,
}
#[derive(Deserialize)]
struct Metadata {
    uid: String,
}

impl Fixture {
    pub fn create(args: &Args) -> Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix("veoveo-module-")
            .tempdir()?;
        let namespace = directory
            .path()
            .file_name()
            .context("fixture namespace")?
            .to_str()
            .context("fixture namespace UTF-8")?
            .to_ascii_lowercase();
        let managed_config = managed::Configuration::create(args, &namespace, directory.path())?;
        let fixture = Self {
            namespace,
            context: args.context.clone(),
            image: args.gateway_image.clone(),
            directory,
            uid: None,
            root_password: password()?,
            runtime_password: password()?,
            serial: 0,
            prior_passwords: Vec::new(),
            managed_config,
            managed: None,
            agent_uid: None,
            cluster_owned: Vec::new(),
            kube_version: None,
            first_probe_failure: None,
        };
        Ok(fixture)
    }
    pub fn initialize(&mut self) -> Result<()> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Versions {
            server_version: ServerVersion,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct ServerVersion {
            git_version: String,
        }
        let version: Versions = serde_json::from_slice(&process::checked(
            self.kubectl().args(["version", "--output=json"]),
            20,
        )?)?;
        ensure!(
            !version.server_version.git_version.is_empty(),
            "selected server Kubernetes version absent"
        );
        self.kube_version = Some(version.server_version.git_version);
        let created = process::checked(
            self.kubectl()
                .args(["create", "namespace", &self.namespace, "--output=json"]),
            30,
        )?;
        let namespace: Namespace = serde_json::from_slice(&created)?;
        ensure!(
            !namespace.metadata.uid.is_empty(),
            "created namespace did not expose ownership UID"
        );
        self.uid = Some(namespace.metadata.uid);
        // These values never appear in child command arguments or evidence.
        self.prior_passwords
            .extend(self.managed_config.installation_secrets.values().cloned());
        self.managed_config.api_egress = managed::Configuration::observe_api_egress(self)?;
        self.credentials()?;
        self.control_plane()?;
        self.verify_network_policy()?;
        Ok(())
    }
    pub fn kubectl(&self) -> Command {
        self.kubectl_in(&self.namespace)
    }
    pub(super) fn kubectl_in(&self, namespace: &str) -> Command {
        let mut command = Command::new("kubectl");
        command.args([
            "--context",
            &self.context,
            "--namespace",
            namespace,
            "--request-timeout=10s",
        ]);
        command
    }
    fn file(&mut self, value: &Value) -> Result<std::path::PathBuf> {
        self.serial += 1;
        let path = self
            .directory
            .path()
            .join(format!("object-{}.json", self.serial));
        fs::write(&path, serde_json::to_vec(value)?)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        Ok(path)
    }
    pub fn apply(&mut self, object: &Value) -> Result<()> {
        let path = self.file(object)?;
        process::checked(
            self.kubectl_in(
                object["metadata"]["namespace"]
                    .as_str()
                    .unwrap_or(&self.namespace),
            )
            .args(["apply", "--filename"])
            .arg(path),
            30,
        )?;
        Ok(())
    }
    pub fn create_object(&mut self, object: &Value) -> Result<()> {
        let path = self.file(object)?;
        process::checked(
            self.kubectl_in(
                object["metadata"]["namespace"]
                    .as_str()
                    .unwrap_or(&self.namespace),
            )
            .args(["create", "--filename"])
            .arg(path),
            30,
        )?;
        Ok(())
    }
    fn credentials(&mut self) -> Result<()> {
        for (name, user, password) in [
            (
                "veoveo-surreal-admin",
                "fixture-admin",
                self.root_password.clone(),
            ),
            (
                "veoveo-surreal-runtime",
                "fixture-runtime",
                self.runtime_password.clone(),
            ),
        ] {
            self.apply(&json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":name},"type":"Opaque","stringData":{"username":user,"password":password}}))?;
        }
        Ok(())
    }
    pub fn rotate_credentials(&mut self) -> Result<()> {
        self.apply(&json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":"fixture-old-runtime"},"type":"Opaque","stringData":{"username":"fixture-runtime","password":self.runtime_password}}))?;
        self.prior_passwords.push(self.runtime_password.clone());
        self.runtime_password = password()?;
        self.credentials()?;
        if self.agent_uid.is_some() {
            self.agent_runtime_credentials()?;
        }
        Ok(())
    }
    fn control_plane(&mut self) -> Result<()> {
        self.apply(&json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"fixture-control-plane"},"data":{"gateway.json":serde_json::to_string(&self.managed_config.plane)?,"jwks.json":self.managed_config.jwks}}))
    }
    fn verify_network_policy(&mut self) -> Result<()> {
        // Read the chart's qualified database image pin; do not maintain another pin.
        let values: Value =
            serde_yaml_ng::from_slice(&fs::read("deploy/helm/veoveo/values.yaml")?)?;
        let image = &values["surrealdb"]["image"];
        ensure!(
            image["tag"] == "v3.3.0",
            "network probe requires the chart's qualified SurrealDB profile"
        );
        let pinned = format!(
            "{}@{}",
            image["repository"]
                .as_str()
                .context("chart database repository")?,
            image["digest"].as_str().context("chart database digest")?
        );
        let canary = json!({"apiVersion":"v1","kind":"Pod","metadata":{"name":"network-canary","labels":{"veoveo.ai/network-canary":"true"}},"spec":{"automountServiceAccountToken":false,"containers":[{"name":"canary","image":pinned,"args":["start","--bind","0.0.0.0:8000","--unauthenticated","memory"],"resources":{"limits":{"memory":"512Mi","cpu":"1"}},"readinessProbe":{"httpGet":{"path":"/ready","port":8000},"periodSeconds":1}}]}});
        self.create_object(&canary)?;
        process::checked(
            self.kubectl().args([
                "wait",
                "--for=condition=Ready",
                "pod/network-canary",
                "--timeout=120s",
            ]),
            130,
        )?;
        self.create_object(&json!({"apiVersion":"networking.k8s.io/v1","kind":"NetworkPolicy","metadata":{"name":"offline-producer"},"spec":{"podSelector":{"matchLabels":{"veoveo.ai/offline-producer":"true"}},"policyTypes":["Ingress","Egress"],"ingress":[],"egress":[]}}))?;
        let bytes = process::checked(
            self.kubectl()
                .args(["get", "pod", "network-canary", "--output=json"]),
            20,
        )?;
        let canary: Value = serde_json::from_slice(&bytes)?;
        ensure!(
            canary["status"]["conditions"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|c| c["type"] == "Ready" && c["status"] == "True"),
            "network canary lost serving readiness"
        );
        let ip: std::net::IpAddr = canary["status"]["podIP"]
            .as_str()
            .context("Ready network canary PodIP absent")?
            .parse()
            .context("network canary PodIP invalid")?;
        // Policy proof uses a directly observed serving Pod; it does not depend
        // on asynchronous Service/endpoints/kube-proxy publication.
        let endpoint = format!("http://{}", std::net::SocketAddr::new(ip, 8000));
        for (name, denied) in [("network-positive", false), ("network-denied", true)] {
            let labels = if denied {
                json!({"veoveo.ai/offline-producer":"true"})
            } else {
                json!({})
            };
            let probe = json!({"apiVersion":"batch/v1","kind":"Job","metadata":{"name":name},"spec":{"backoffLimit":0,"activeDeadlineSeconds":30,"template":{"metadata":{"labels":labels},"spec":{"restartPolicy":"Never","automountServiceAccountToken":false,"containers":[{"name":"probe","image":pinned,"args":["isready","--endpoint",endpoint]}]}}}});
            self.create_object(&probe)?;
            self.wait_job(name, !denied)?;
            // A scheduling failure or Job deadline is not proof of CNI enforcement.
            if denied {
                let settled = process::checked(
                    self.kubectl().args(["get", "job", name, "--output=json"]),
                    20,
                )?;
                let settled: Value = serde_json::from_slice(&settled)?;
                ensure!(
                    !settled["status"]["conditions"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|c| c["reason"] == "DeadlineExceeded"),
                    "network probe deadline does not prove policy enforcement"
                );
                let bytes = process::checked(
                    self.kubectl().args([
                        "get",
                        "pods",
                        "--selector=job-name=network-denied",
                        "--output=json",
                    ]),
                    20,
                )?;
                let pods: Value = serde_json::from_slice(&bytes)?;
                let status =
                    &pods["items"][0]["status"]["containerStatuses"][0]["state"]["terminated"];
                ensure!(
                    status["exitCode"] == 1
                        && status["signal"].as_i64().unwrap_or(0) == 0
                        && status["reason"] == "Error",
                    "negative network probe did not exit with an observed connection rejection"
                );
            }
        }
        Ok(())
    }
    pub fn generate(&mut self, generation: u64, enabled: &[&str]) -> Result<Render> {
        let selection = ModuleSelectionDocument::new(
            enabled
                .iter()
                .map(|s| ModuleName::new(*s))
                .collect::<Result<Vec<_>, _>>()?,
            InstallationGeneration::new(generation)?,
            CredentialRevision::new(format!("fixture-runtime-{generation}"))?,
        )?;
        let selection_name = format!("selection-{generation}");
        self.apply(&json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":selection_name},"immutable":true,"data":{"selection.json":serde_json::to_string(&selection)?}}))?;
        let job = format!("producer-{generation}");
        let producer = json!({"apiVersion":"batch/v1","kind":"Job","metadata":{"name":job},"spec":{"backoffLimit":0,"activeDeadlineSeconds":120,"template":{"metadata":{"labels":{"veoveo.ai/offline-producer":"true"}},"spec":{"restartPolicy":"Never","automountServiceAccountToken":false,"securityContext":{"runAsNonRoot":true,"runAsUser":10001,"seccompProfile":{"type":"RuntimeDefault"}},"containers":[{"name":"producer","image":self.image.reference(),"command":["/usr/local/bin/gateway"],"args":["module-plan","--modules","/selection/selection.json","--composition",self.image.digest.as_str()],"securityContext":{"readOnlyRootFilesystem":true,"allowPrivilegeEscalation":false,"capabilities":{"drop":["ALL"]}},"resources":{"limits":{"memory":"512Mi","cpu":"1"},"requests":{"memory":"128Mi","cpu":"100m"}},"volumeMounts":[{"name":"selection","mountPath":"/selection","readOnly":true}]}],"volumes":[{"name":"selection","configMap":{"name":selection_name}}]}}}});
        self.create_object(&producer)?;
        self.wait_job(&job, true)?;
        let raw = self.logs(&job)?;
        let plan: ModulePlanDocument = serde_json::from_slice(&raw)?;
        ensure!(
            plan.composition().as_str() == self.image.digest.as_str(),
            "image producer returned another composition binding"
        );
        ensure!(
            plan.selection()? == selection,
            "image producer changed installation selection"
        );
        self.render(plan, &raw)
    }
    fn chart_values(&self, raw: &[u8]) -> Result<Value> {
        let config = &self.managed_config;
        Ok(
            json!({"installationPreset":"custom","components":["gateway","platform-store","agent-runtime-support"],"mcpServers":[],"global":{"production":true,"installationId":self.namespace,"publicBaseUrl":"https://gateway.invalid"},"gateway":{"image":{"repository":self.image.repository,"tag":"fixture","digest":self.image.digest.as_str()},"existingControlPlaneConfigMap":"fixture-control-plane","controlPlaneRevision":"1".repeat(64),"auditRetentionDays":1,"resources":{"requests":{"memory":"128Mi","cpu":"100m"},"limits":{"memory":"512Mi","cpu":"1"}},"agents":{"models":[config.model],"templates":[config.template],"modelSecrets":{"VEOVEO_AGENT_MODEL_FIXTURE_KEY":{"existingSecret":"fixture-model","key":"api-key"}}}},"agentManager":{"namespace":config.namespace,"image":{"repository":config.manager_image.repository,"tag":"fixture","digest":config.manager_image.digest.as_str()},"existingControlPlaneConfigMap":"fixture-control-plane","kubernetesApiEgress":config.api_egress,"modelEgress":[]},"surrealdb":{"namespace":"fixture","database":"installation"},"moduleInstallation":{"planJson":std::str::from_utf8(raw)?}}),
        )
    }
    fn render(&mut self, plan: ModulePlanDocument, raw: &[u8]) -> Result<Render> {
        let values = self.chart_values(raw)?;
        let path = self.file(&values)?;
        let rendered = process::checked_redacted(
            Command::new("helm")
                .args([
                    "template",
                    "--kube-version",
                    self.kube_version
                        .as_deref()
                        .context("selected Kubernetes server version not observed")?,
                    "module-fixture",
                    "deploy/helm/veoveo",
                    "--namespace",
                    &self.namespace,
                    "--values",
                ])
                .arg(path)
                .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")),
            60,
            "module installation chart render",
            |stderr| self.redact_diagnostics(String::from_utf8_lossy(stderr).into_owned()),
        )?;
        let objects = serde_yaml_ng::Deserializer::from_slice(&rendered)
            .map(Value::deserialize)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Render { plan, objects })
    }
    pub fn install(&mut self, render: &Render) -> Result<()> {
        for object in &render.objects {
            let component = object["metadata"]["labels"]["app.kubernetes.io/component"]
                .as_str()
                .unwrap_or("");
            if (object["kind"] == "ServiceAccount"
                && object["metadata"]["namespace"] != self.managed_config.namespace)
                || component == "surrealdb"
                || component == "module-plan"
                || (object["kind"] == "Job"
                    && matches!(
                        component,
                        "installation-prepare" | "module-migration" | "control-plane-publication"
                    ))
            {
                self.apply(object)?;
            }
        }
        for job in render.objects.iter().filter(|o| o["kind"] == "Job") {
            self.wait_job(
                job["metadata"]["name"]
                    .as_str()
                    .context("rendered Job name")?,
                true,
            )?;
        }
        self.install_managed_objects(render)?;
        Ok(())
    }
    pub fn wait_job(&self, name: &str, success: bool) -> Result<()> {
        let start = Instant::now();
        loop {
            let bytes = process::checked(
                self.kubectl().args(["get", "job", name, "--output=json"]),
                20,
            )?;
            let job: Value = serde_json::from_slice(&bytes)?;
            let conditions = job["status"]["conditions"].as_array();
            if conditions.is_some_and(|c| {
                c.iter()
                    .any(|v| v["type"] == "Complete" && v["status"] == "True")
            }) {
                ensure!(success, "rejected fixture Job unexpectedly succeeded");
                return Ok(());
            }
            if conditions.is_some_and(|c| {
                c.iter()
                    .any(|v| v["type"] == "Failed" && v["status"] == "True")
            }) {
                ensure!(
                    !success,
                    "installation Job failed; bounded redacted pod diagnostics will accompany the failure"
                );
                return Ok(());
            }
            ensure!(
                start.elapsed() < Duration::from_secs(360),
                "fixture Job did not settle within 360 seconds"
            );
            thread::sleep(Duration::from_millis(250));
        }
    }
    fn install_managed_objects(&mut self, render: &Render) -> Result<()> {
        for object in render.objects.iter().filter(|o| o["kind"] == "Namespace") {
            if self.agent_uid.is_none() {
                let path = self.file(object)?;
                let bytes = process::checked(
                    self.kubectl()
                        .args(["create", "--filename"])
                        .arg(path)
                        .arg("--output=json"),
                    30,
                )?;
                let owned: Namespace = serde_json::from_slice(&bytes)?;
                ensure!(!owned.metadata.uid.is_empty(), "agent namespace UID absent");
                self.agent_uid = Some(owned.metadata.uid);
            }
        }
        self.agent_runtime_credentials()?;
        let namespace = self.managed_config.namespace.clone();
        for target in [&self.namespace.clone(), &namespace] {
            self.apply(&json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":"fixture-model","namespace":target},"type":"Opaque","stringData":{"api-key":"fixture-only-unused"}}))?;
        }
        self.apply(&json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":"veoveo-installation-secrets"},"type":"Opaque","stringData":self.managed_config.installation_secrets}))?;
        self.apply(&json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":"veoveo-audit-signing-key"},"type":"Opaque","stringData":{"seed-b64":"BwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwcHBwc="}}))?;
        self.apply(&json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"fixture-control-plane","namespace":namespace},"data":{"gateway.json":serde_json::to_string(&self.managed_config.plane)?,"jwks.json":self.managed_config.jwks}}))?;
        self.apply(&json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":self.managed_config.template.workload.config_map,"namespace":namespace},"immutable":true,"data":self.managed_config.data}))?;
        for object in managed_objects(&render.objects) {
            let kind = object["kind"].as_str().context("managed object kind")?;
            if kind.starts_with("ValidatingAdmissionPolicy") {
                let name = object["metadata"]["name"]
                    .as_str()
                    .context("cluster policy name")?;
                let path = format!(
                    "/apis/admissionregistration.k8s.io/v1/{}/{name}",
                    if kind == "ValidatingAdmissionPolicy" {
                        "validatingadmissionpolicies"
                    } else {
                        "validatingadmissionpolicybindings"
                    }
                );
                if !self.cluster_owned.iter().any(|(p, _)| p == &path) {
                    let file = self.file(object)?;
                    let bytes = process::checked(
                        self.kubectl()
                            .args(["create", "--filename"])
                            .arg(file)
                            .arg("--output=json"),
                        30,
                    )?;
                    let created: Namespace = serde_json::from_slice(&bytes)?;
                    ensure!(
                        !created.metadata.uid.is_empty(),
                        "cluster policy UID absent"
                    );
                    self.cluster_owned.push((path, created.metadata.uid));
                }
            } else {
                self.apply(object)?;
            }
        }
        Ok(())
    }
    fn agent_runtime_credentials(&mut self) -> Result<()> {
        self.apply(&json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":"veoveo-surreal-runtime","namespace":self.managed_config.namespace},"type":"Opaque","stringData":{"username":"fixture-runtime","password":self.runtime_password}}))
    }
    pub(super) fn store_config(
        &self,
        endpoint: &str,
    ) -> Result<veoveo_platform_store::StoreConfig> {
        veoveo_platform_store::StoreConfig::builder(
            endpoint,
            "fixture",
            "installation",
            veoveo_platform_store::StoreCredentials::root(
                "fixture-admin",
                self.root_password.clone(),
            ),
        )
        .build()
        .map_err(Into::into)
    }
    pub fn logs(&self, job: &str) -> Result<Vec<u8>> {
        process::checked(
            self.kubectl()
                .args(["logs", &format!("job/{job}"), "--limit-bytes=2097152"]),
            30,
        )
    }
    pub fn snapshot(&self) -> Result<BTreeMap<String, String>> {
        let bytes = process::checked(
            self.kubectl()
                .args(["get", "jobs,configmaps,statefulsets", "--output=json"]),
            30,
        )?;
        let objects: Value = serde_json::from_slice(&bytes)?;
        Ok(objects["items"]
            .as_array()
            .context("snapshot items")?
            .iter()
            .map(|o| {
                (
                    format!(
                        "{}/{}",
                        o["kind"].as_str().unwrap_or(""),
                        o["metadata"]["name"].as_str().unwrap_or("")
                    ),
                    o["metadata"]["uid"].as_str().unwrap_or("").into(),
                )
            })
            .collect())
    }
    pub fn probe(
        &mut self,
        template: &Value,
        name: &str,
        args: &[&str],
        success: bool,
    ) -> Result<Vec<u8>> {
        let mut job = template.clone();
        job["metadata"] = json!({"name":name});
        job["spec"]
            .as_object_mut()
            .context("probe Job spec")?
            .remove("ttlSecondsAfterFinished");
        job["spec"]["template"]["spec"]["containers"][0]["command"] =
            json!(["/usr/local/bin/gateway"]);
        job["spec"]["template"]["spec"]["containers"][0]["args"] = json!(args);
        self.create_object(&job)?;
        let settled = self.wait_job(name, success);
        let logs = self.logs(name);
        if let Err(error) = settled {
            if let Ok(ref bytes) = logs {
                self.remember_probe_failure(name, bytes);
            }
            return Err(error).with_context(|| format!("probe {name}"));
        }
        logs
    }
    fn remember_probe_failure(&mut self, name: &str, bytes: &[u8]) {
        if self.first_probe_failure.is_none() {
            self.first_probe_failure = Some(self.redact_diagnostics(format!(
                "failed probe {name}:\n{}\n",
                String::from_utf8_lossy(bytes)
            )));
        }
    }
    pub fn require_probe_result(
        &mut self,
        name: &str,
        bytes: &[u8],
        accepted: bool,
        reason: &str,
    ) -> Result<()> {
        if !accepted {
            self.remember_probe_failure(name, bytes);
            anyhow::bail!("{reason}; probe {name}");
        }
        Ok(())
    }
    pub fn failure_diagnostics(&self) -> Result<String> {
        let mut output = self.first_probe_failure.clone().unwrap_or_default();
        match self.inventory_diagnostics() {
            Ok(inventory) => output.push_str(&inventory),
            Err(_) => output.push_str("owned inventory diagnostics unavailable\n"),
        }
        Ok(self.redact_diagnostics(output))
    }
    fn inventory_diagnostics(&self) -> Result<String> {
        let uid = self
            .uid
            .as_ref()
            .context("no owned namespace was created")?;
        let namespace = process::checked(
            self.kubectl()
                .args(["get", "namespace", &self.namespace, "--output=json"]),
            20,
        )?;
        let namespace: Namespace = serde_json::from_slice(&namespace)?;
        ensure!(
            &namespace.metadata.uid == uid,
            "diagnostic namespace ownership changed"
        );
        let mut output = String::new();
        let mut diagnostic_namespaces = Vec::new();
        if let Some(uid) = &self.agent_uid {
            let namespace: Namespace = serde_json::from_slice(&process::checked(
                self.kubectl().args([
                    "get",
                    "namespace",
                    &self.managed_config.namespace,
                    "--output=json",
                ]),
                20,
            )?)?;
            ensure!(
                &namespace.metadata.uid == uid,
                "diagnostic agent namespace ownership changed"
            );
            diagnostic_namespaces.push(&self.managed_config.namespace);
        }
        diagnostic_namespaces.push(&self.namespace);
        let mut inventories = Vec::new();
        // Describe both owned namespaces before logs can consume the output budget.
        for namespace in diagnostic_namespaces {
            output.push_str(&format!("namespace {namespace}: owned\n"));
            let controllers: Value = serde_json::from_slice(&process::checked(
                self.kubectl_in(namespace)
                    .args(["get", "deployments", "--output=json"]),
                30,
            )?)?;
            let controllers = controllers["items"]
                .as_array()
                .context("diagnostic controller inventory")?;
            output.push_str(&format!("deployments: {}\n", controllers.len()));
            for controller in controllers {
                output.push_str(&format!(
                    "deployment {namespace}/{}: replicas={} ready={} available={}\n",
                    controller["metadata"]["name"]
                        .as_str()
                        .context("diagnostic controller name")?,
                    controller["spec"]["replicas"],
                    controller["status"]["readyReplicas"],
                    controller["status"]["availableReplicas"]
                ));
            }
            let pods: Value = serde_json::from_slice(&process::checked(
                self.kubectl_in(namespace)
                    .args(["get", "pods", "--output=json"]),
                30,
            )?)?;
            let pods = pods["items"]
                .as_array()
                .context("diagnostic pod inventory")?;
            output.push_str(&format!("pods: {}\n", pods.len()));
            let mut names = Vec::new();
            for pod in pods {
                let name = pod["metadata"]["name"]
                    .as_str()
                    .context("diagnostic pod name")?;
                names.push(name.to_owned());
                output.push_str(&format!(
                    "pod {namespace}/{name}: {}\n",
                    pod["status"]["phase"]
                ));
                for status in pod["status"]["containerStatuses"]
                    .as_array()
                    .into_iter()
                    .flatten()
                {
                    output.push_str(&format!(
                        "container {}: {}\n",
                        status["name"], status["state"]
                    ));
                }
            }
            inventories.push((namespace, names));
        }
        for (namespace, names) in inventories {
            for name in names {
                if output.len() >= 16384 {
                    break;
                }
                output.push_str(&format!("logs {namespace}/{name}:\n"));
                if let Ok(logs) = process::checked(
                    self.kubectl_in(namespace).args([
                        "logs",
                        &name,
                        "--all-containers=true",
                        "--tail=10",
                        "--limit-bytes=4096",
                    ]),
                    15,
                ) {
                    output.push_str(&String::from_utf8_lossy(&logs));
                    output.push('\n');
                }
            }
        }
        Ok(self.redact_diagnostics(output))
    }
    fn redact_diagnostics(&self, mut output: String) -> String {
        for secret in std::iter::once(&self.root_password)
            .chain(std::iter::once(&self.runtime_password))
            .chain(self.prior_passwords.iter())
            .chain(self.managed_config.installation_secrets.values())
        {
            if !secret.is_empty() {
                output = output.replace(secret, "[REDACTED]");
            }
        }
        let end = output
            .char_indices()
            .map(|(i, _)| i)
            .take_while(|i| *i <= 16384)
            .last()
            .unwrap_or(0);
        if output.len() > 16384 {
            output.truncate(end);
        }
        output
    }
    pub fn image_ids(&self) -> Result<Vec<String>> {
        let bytes = process::checked(self.kubectl().args(["get", "pods", "--output=json"]), 30)?;
        let pods: Value = serde_json::from_slice(&bytes)?;
        let mut ids = Vec::new();
        for pod in pods["items"].as_array().context("image observation pods")? {
            let statuses = pod["status"]["containerStatuses"].as_array();
            for container in pod["spec"]["containers"].as_array().into_iter().flatten() {
                if container["image"] != self.image.reference() {
                    continue;
                }
                let status = statuses
                    .and_then(|s| s.iter().find(|v| v["name"] == container["name"]))
                    .context("bound image container status absent")?;
                let id = status["imageID"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .context("bound image runtime identity absent")?;
                ids.push(id.into());
            }
        }
        ids.sort();
        ids.dedup();
        ensure!(!ids.is_empty(), "no actual bound image identities observed");
        Ok(ids)
    }
    pub fn cleanup(&mut self) -> Result<()> {
        let mut failures = Vec::new();
        let mut kernels_gone = self.agent_uid.is_none();
        if let Some(uid) = self.agent_uid.clone() {
            let name = self.managed_config.namespace.clone();
            match self.delete_owned_namespace(&name, &uid) {
                Ok(()) => {
                    self.agent_uid = None;
                    kernels_gone = true;
                }
                Err(_) => failures.push("agent namespace deletion/observation"),
            }
        }
        // The DB remains available until every kernel has exited. Sequences and
        // episodes are monotonic in this qualified kernel path.
        if let Some(mut managed) = self.managed.take() {
            if kernels_gone {
                if managed.final_zero_episodes().is_err() {
                    failures.push("final episode observation");
                }
            } else {
                failures.push("final drain/zero-inference proof unavailable");
            }
            if managed.close_live().is_err() {
                failures.push("owned LIVE cleanup");
            }
        }
        for index in (0..self.cluster_owned.len()).rev() {
            let (path, uid) = self.cluster_owned[index].clone();
            let result = (|| {
                let file = self.file(&json!({"apiVersion":"v1","kind":"DeleteOptions","preconditions":{"uid":uid},"propagationPolicy":"Foreground"}))?;
                process::checked(
                    self.kubectl()
                        .arg("delete")
                        .arg(format!("--raw={path}"))
                        .arg("--filename")
                        .arg(file),
                    30,
                )?;
                Ok::<_, anyhow::Error>(())
            })();
            if result.is_ok() {
                self.cluster_owned.remove(index);
            } else {
                failures.push("UID-owned cluster policy deletion");
            }
        }
        if let Some(uid) = self.uid.clone() {
            let name = self.namespace.clone();
            if self.delete_owned_namespace(&name, &uid).is_ok() {
                self.uid = None;
            } else {
                failures.push("installation namespace deletion/observation");
            }
        }
        ensure!(
            failures.is_empty(),
            "fixture cleanup failed: {}",
            failures.join(", ")
        );
        Ok(())
    }
    fn delete_owned_namespace(&mut self, name: &str, uid: &str) -> Result<()> {
        let existing = process::checked(
            self.kubectl().args([
                "get",
                "namespace",
                name,
                "--ignore-not-found",
                "--output=json",
            ]),
            20,
        )?;
        if existing.is_empty() {
            return Ok(());
        }
        let existing: Namespace = serde_json::from_slice(&existing)?;
        ensure!(
            existing.metadata.uid == uid,
            "owned namespace UID changed before deletion"
        );
        let options = json!({"apiVersion":"v1","kind":"DeleteOptions","preconditions":{"uid":uid},"propagationPolicy":"Foreground"});
        let path = self.file(&options)?;
        process::checked(
            self.kubectl()
                .arg("delete")
                .arg(format!("--raw=/api/v1/namespaces/{name}"))
                .arg("--filename")
                .arg(path),
            30,
        )?;
        process::checked(
            self.kubectl().args([
                "wait",
                "--for=delete",
                &format!("namespace/{name}"),
                "--timeout=60s",
            ]),
            70,
        )?;
        Ok(())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
fn password() -> Result<String> {
    let mut bytes = [0_u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_modules::{
        CompositionIdentity, ExecutionCommand, ExecutionImage, LaneExecution, ModuleRegistry,
        ModuleRuntimeBinding, RuntimeBindingKey,
    };

    #[test]
    fn fixture_diagnostics_redact_every_owned_secret_before_capping() -> Result<()> {
        let image = "registry.invalid/fixture@sha256:".to_owned() + &"a".repeat(64);
        let args = Args {
            context: "native-unused".into(),
            gateway_image: image.parse().unwrap(),
            manager_image: image.parse().unwrap(),
            kernel_image: image.parse().unwrap(),
            evidence_output: std::path::PathBuf::from("unused"),
        };
        let mut fixture = Fixture::create(&args)?;
        fixture.prior_passwords.push("prior-test-password".into());
        let secrets: Vec<_> = std::iter::once(&fixture.root_password)
            .chain(std::iter::once(&fixture.runtime_password))
            .chain(fixture.prior_passwords.iter())
            .chain(fixture.managed_config.installation_secrets.values())
            .cloned()
            .collect();
        let input = secrets
            .iter()
            .map(|secret| secret.as_str())
            .collect::<Vec<_>>()
            .join("\n")
            + &"é".repeat(20000);
        fixture
            .require_probe_result("first", input.as_bytes(), false, "wrong admission")
            .unwrap_err();
        fixture
            .require_probe_result("later", b"later failure", false, "wrong admission")
            .unwrap_err();
        let remembered = fixture.first_probe_failure.as_ref().unwrap();
        assert!(remembered.starts_with("failed probe first:"));
        assert!(!remembered.contains("later failure"));
        assert!(remembered.len() <= 16384);
        // Even an unavailable namespace inventory cannot erase the primary probe.
        let failure = fixture.failure_diagnostics()?;
        assert!(failure.starts_with("failed probe first:"));
        assert!(failure.len() <= 16384);
        for secret in &secrets {
            assert!(!remembered.contains(secret));
        }
        let diagnostic = fixture.redact_diagnostics(input);
        assert!(diagnostic.len() <= 16384);
        assert!(diagnostic.contains("[REDACTED]"));
        for secret in secrets {
            assert!(!diagnostic.contains(&secret));
        }
        Ok(())
    }
    #[test]
    fn generated_fixture_plans_render_with_agents_selected_through_rotation() -> Result<()> {
        let image = |name: &str, digest: char| {
            format!(
                "registry.invalid/{name}@sha256:{}",
                digest.to_string().repeat(64)
            )
            .parse()
            .unwrap()
        };
        let args = Args {
            context: "native-unused".into(),
            gateway_image: image("gateway", 'a'),
            manager_image: image("manager", 'b'),
            kernel_image: image("kernel", 'c'),
            evidence_output: std::path::PathBuf::from("unused-evidence.json"),
        };
        let mut fixture = Fixture::create(&args)?;
        fixture.kube_version = Some("v1.37.0".into());
        // Installed setup observes this address from the selected Kubernetes
        // Service. Native chart admission needs only its admitted network shape.
        fixture.managed_config.api_egress = vec![json!({"cidr":"192.0.2.1/32","port":443})];
        fn execution(name: &str) -> Result<LaneExecution> {
            Ok(LaneExecution::new(
                ExecutionImage::new("gateway")?,
                ExecutionCommand::new(vec![
                    "/usr/local/bin/gateway".into(),
                    "module-migrate".into(),
                    "--module".into(),
                    name.into(),
                ])?,
            )?)
        }
        // Real owner declarations plus the public plan producer. Unselected
        // composition modules remain the installed gateway producer's concern.
        let registry = ModuleRegistry::new(vec![
            veoveo_platform_store::schema::store::module_setup(execution("store")?)?,
            veoveo_platform_store::schema::identity::module_setup(execution("identity")?)?,
            veoveo_platform_store::schema::gateway::module_setup(execution("gateway")?)?,
            veoveo_platform_store::schema::artifacts::module_setup(execution("artifacts")?)?,
            veoveo_platform_store::schema::tasks::module_setup(execution("tasks")?)?,
            veoveo_platform_store::schema::audit::module_setup(execution("audit")?)?,
            veoveo_platform_store::schema::knowledge::module_setup(execution("knowledge")?)?,
            veoveo_agent_runtime::schema::module_setup(execution("agents")?)?,
            veoveo_time_mcp::schema::module_setup(execution("time")?)?,
            veoveo_media_mcp::schema::module_setup(execution("media")?)?,
        ])?;
        let composition = CompositionIdentity::new(fixture.image.digest.as_str())?;
        let produce = |generation, enabled: &[&str]| -> Result<ModulePlanDocument> {
            let selection = ModuleSelectionDocument::new(
                enabled
                    .iter()
                    .map(|name| ModuleName::new(*name))
                    .collect::<Result<Vec<_>, _>>()?,
                InstallationGeneration::new(generation)?,
                CredentialRevision::new(format!("fixture-runtime-{generation}"))?,
            )?;
            Ok(ModulePlanDocument::generate(
                &registry,
                &selection,
                composition.clone(),
                vec![ModuleRuntimeBinding {
                    module: ModuleName::new("agents")?,
                    component: Some(RuntimeBindingKey::new("agent-runtime-support")?),
                    mcp_server: None,
                }],
            )?)
        };
        let plans = super::super::assertions::GENERATIONS
            .into_iter()
            .map(|(generation, enabled, _)| produce(generation, enabled))
            .collect::<Result<Vec<_>>>()?;
        for (index, plan) in plans.into_iter().enumerate() {
            let selection = plan.selection()?;
            assert!(
                selection
                    .enabled()
                    .iter()
                    .any(|name| name.as_str() == "agents")
            );
            assert_eq!(
                selection
                    .enabled()
                    .iter()
                    .any(|name| name.as_str() == "time"),
                index != 1
            );
            assert_eq!(
                selection
                    .enabled()
                    .iter()
                    .any(|name| name.as_str() == "media"),
                index == 2
            );
            let raw = serde_json::to_vec(&plan)?;
            let rendered = fixture
                .render(plan, &raw)
                .with_context(|| format!("native generation {} chart", index + 1))?;
            let applied = managed_objects(&rendered.objects).collect::<Vec<_>>();
            let agent_namespace = fixture.managed_config.namespace.as_str();
            for (kind, name) in [
                ("ServiceAccount", "veoveo-agent-manager"),
                ("ServiceAccount", "veoveo-agent-kernel"),
                ("Role", "veoveo-agent-manager"),
                ("RoleBinding", "veoveo-agent-manager"),
                ("ConfigMap", "veoveo-agent-manager"),
                ("NetworkPolicy", "managed-default-deny"),
                ("NetworkPolicy", "managed-dns"),
                ("NetworkPolicy", "managed-store"),
                ("NetworkPolicy", "managed-controller-api"),
                ("NetworkPolicy", "managed-kernel-gateway"),
                ("Deployment", "veoveo-agent-manager"),
            ] {
                assert_eq!(
                    applied
                        .iter()
                        .filter(|object| object["kind"] == kind
                            && object["metadata"]["name"] == name
                            && object["metadata"]["namespace"] == agent_namespace)
                        .count(),
                    1,
                    "generation {} must apply {kind} {name}",
                    index + 1
                );
            }
            for kind in [
                "ValidatingAdmissionPolicy",
                "ValidatingAdmissionPolicyBinding",
                "Service",
            ] {
                let expected = rendered
                    .objects
                    .iter()
                    .filter(|object| {
                        object["kind"] == kind
                            && !matches!(
                                object["metadata"]["labels"]["app.kubernetes.io/component"]
                                    .as_str(),
                                Some("surrealdb" | "module-plan")
                            )
                    })
                    .count();
                assert!(expected > 0);
                assert_eq!(
                    applied
                        .iter()
                        .filter(|object| object["kind"] == kind)
                        .count(),
                    expected
                );
            }
            let first_deployment = applied
                .iter()
                .position(|object| object["kind"] == "Deployment")
                .context("managed applied deployment")?;
            assert!(
                applied[first_deployment..]
                    .iter()
                    .all(|object| object["kind"] == "Deployment")
            );
            assert!(applied.iter().all(|object| object["kind"] != "Job"
                && !matches!(
                    object["metadata"]["labels"]["app.kubernetes.io/component"].as_str(),
                    Some("surrealdb" | "module-plan")
                )));
            assert!(
                rendered
                    .objects
                    .iter()
                    .any(|object| object["kind"] == "Deployment"
                        && object["spec"]["template"]["metadata"]["labels"]["app.kubernetes.io/component"]
                            == "agent-manager")
            );
            assert!(
                !rendered
                    .objects
                    .iter()
                    .any(|object| object["kind"] == "Deployment"
                        && matches!(
                            object["spec"]["template"]["metadata"]["labels"]["app.kubernetes.io/component"].as_str(),
                            Some("time" | "media")
                        ))
            );
        }
        let invalid = produce(1, &["time"])?;
        let error = fixture
            .render(invalid.clone(), &serde_json::to_vec(&invalid)?)
            .err()
            .context("missing Agents must fail chart admission")?;
        let diagnostic = format!("{error:#}");
        assert!(
            diagnostic.contains("helm failed (exit status: 1)"),
            "{diagnostic}"
        );
        assert!(
            diagnostic.contains("enabled runtime requires selected module agents"),
            "{diagnostic}"
        );
        Ok(())
    }
}
