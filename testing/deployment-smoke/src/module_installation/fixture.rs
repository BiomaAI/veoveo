//! Namespace-owned public plans, disposable credentials and actual chart Jobs.
use super::{Args, PinnedImage, process};
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
        };
        Ok(fixture)
    }
    pub fn initialize(&mut self) -> Result<()> {
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
        self.credentials()?;
        self.control_plane()?;
        self.verify_network_policy()?;
        Ok(())
    }
    pub fn kubectl(&self) -> Command {
        let mut command = Command::new("kubectl");
        command.args([
            "--context",
            &self.context,
            "--namespace",
            &self.namespace,
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
        process::checked(self.kubectl().args(["apply", "--filename"]).arg(path), 30)?;
        Ok(())
    }
    pub fn create_object(&mut self, object: &Value) -> Result<()> {
        let path = self.file(object)?;
        process::checked(self.kubectl().args(["create", "--filename"]).arg(path), 30)?;
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
        self.credentials()
    }
    fn control_plane(&mut self) -> Result<()> {
        let control = json!({"identity_providers":[],"authorization_servers":[],"servers":[],"profiles":[],"tenants":[{"id":"fixture","metadata":{}}],"work_contexts":[{"id":"mission","tenant":"fixture","title":"Mission","policy_revision":"policy-fixture","output_policy":{"owner":{"kind":"group","id":"operations"},"initial_grants":[],"classification":null,"data_labels":[]},"memberships":[{"level":"contributor","groups":["operations"]}]}],"policies":[{"version":"policy-fixture","rules":[],"metadata":{}}],"data_labels":[],"oidc_clients":[]});
        let control: veoveo_mcp_contract::GatewayControlPlane = serde_json::from_value(control)?;
        self.apply(&json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"fixture-control-plane"},"data":{"gateway.json":serde_json::to_string(&control)?}}))
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
        let values = json!({"installationPreset":"custom","components":["gateway","platform-store"],"mcpServers":[],"global":{"production":true},"gateway":{"image":{"repository":self.image.repository,"tag":"fixture","digest":self.image.digest.as_str()},"existingControlPlaneConfigMap":"fixture-control-plane","controlPlaneRevision":"1".repeat(64),"auditRetentionDays":1,"resources":{"requests":{"memory":"128Mi","cpu":"100m"},"limits":{"memory":"512Mi","cpu":"1"}}},"surrealdb":{"namespace":"fixture","database":"installation"},"moduleInstallation":{"planJson":String::from_utf8(raw)?}});
        let path = self.file(&values)?;
        let rendered = process::checked(
            Command::new("helm")
                .args([
                    "template",
                    "module-fixture",
                    "deploy/helm/veoveo",
                    "--namespace",
                    &self.namespace,
                    "--values",
                ])
                .arg(path),
            60,
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
            if object["kind"] == "ServiceAccount"
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
        self.wait_job(name, success)?;
        self.logs(name)
    }
    pub fn failure_diagnostics(&self) -> Result<String> {
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
        let bytes = process::checked(self.kubectl().args(["get", "pods", "--output=json"]), 30)?;
        let pods: Value = serde_json::from_slice(&bytes)?;
        let mut output = String::new();
        for pod in pods["items"]
            .as_array()
            .context("diagnostic pod inventory")?
        {
            let name = pod["metadata"]["name"]
                .as_str()
                .context("diagnostic pod name")?;
            output.push_str(&format!("pod {name}: {}\n", pod["status"]["phase"]));
            for status in pod["status"]["containerStatuses"]
                .as_array()
                .into_iter()
                .flatten()
            {
                let state = &status["state"];
                output.push_str(&format!("container {}: {}\n", status["name"], state));
            }
            if let Ok(logs) = process::checked(
                self.kubectl().args([
                    "logs",
                    name,
                    "--all-containers=true",
                    "--tail=10",
                    "--limit-bytes=4096",
                ]),
                15,
            ) {
                output.push_str(&String::from_utf8_lossy(&logs));
                output.push('\n');
            }
            if output.len() > 16384 {
                break;
            }
        }
        for secret in std::iter::once(&self.root_password)
            .chain(std::iter::once(&self.runtime_password))
            .chain(self.prior_passwords.iter())
        {
            output = output.replace(secret, "[REDACTED]");
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
        Ok(output)
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
        let Some(uid) = self.uid.clone() else {
            return Ok(());
        };
        let options = json!({"apiVersion":"v1","kind":"DeleteOptions","preconditions":{"uid":uid},"propagationPolicy":"Foreground"});
        let path = self.file(&options)?;
        process::checked(
            self.kubectl()
                .arg("delete")
                .arg(format!("--raw=/api/v1/namespaces/{}", self.namespace))
                .arg("--filename")
                .arg(path),
            30,
        )?;
        process::checked(
            self.kubectl().args([
                "wait",
                "--for=delete",
                &format!("namespace/{}", self.namespace),
                "--timeout=60s",
            ]),
            70,
        )?;
        self.uid = None;
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
