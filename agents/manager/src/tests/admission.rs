//! Native Kubernetes admission qualification. It creates only a temporary
//! namespace, service accounts, RBAC and policies. One zero-replica Deployment
//! qualifies retirement; executable workload requests use dry-run.
use super::*;
use crate::kubernetes::{GENERATION, OWNER_LABEL};
use serde::Deserialize;
use serde_json::Value;
use std::{
    io::Write,
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

fn command(program: &str, args: &[&str], body: Option<&Value>) -> Result<Output> {
    let mut child = Command::new("timeout")
        .arg("30s")
        .arg(program)
        .args(args)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    if let Some(body) = body {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(body)?)?;
    } else {
        drop(child.stdin.take());
    }
    Ok(child.wait_with_output()?)
}

fn success(output: Output) -> Result<Vec<u8>> {
    anyhow::ensure!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(output.stdout)
}

struct Installation {
    directory: PathBuf,
    objects: Vec<Value>,
}
impl Installation {
    fn cleanup(&mut self) -> Result<()> {
        for object in self.objects.iter().rev() {
            success(command(
                "kubectl",
                &["delete", "--ignore-not-found", "--wait=false", "-f", "-"],
                Some(object),
            )?)?;
        }
        self.objects.clear();
        std::fs::remove_dir_all(&self.directory)?;
        Ok(())
    }
}
impl Drop for Installation {
    fn drop(&mut self) {
        if !self.objects.is_empty() {
            let _ = self.cleanup();
        }
    }
}

fn rendered(
    namespace: &str,
    network_policy: bool,
) -> Result<(Installation, Config, ManagedAgentReconciliation, ConfigMap)> {
    let (mut config, mut snapshot, map) = fixture();
    let owner = format!("agent-{}", uuid::Uuid::now_v7().simple());
    config.namespace = namespace.into();
    config.templates[0].workload.namespace = namespace.into();
    config.gateway_url = "https://gateway.test/".into();
    config.gateway_transport_url = "http://mcp-gateway.veoveo.svc:8788/".into();
    config.store_endpoint = "ws://surrealdb.veoveo.svc:8000".into();
    snapshot.instance.resources.namespace = namespace.into();
    snapshot.instance.resources.workload = owner.clone();
    snapshot.instance.resources.credential_secret = format!("{owner}-key");
    snapshot.instance.resources.volume_claim = format!("{owner}-memory");
    let directory = std::env::temp_dir().join(namespace);
    std::fs::create_dir(&directory)?;
    let values = json!({"networkPolicy":{"enabled":network_policy},"global":{"publicBaseUrl":"https://gateway.test"},"gateway":{"controlPlaneRevision":"a".repeat(64),"agents":{"models":config.models,"templates":config.templates}},"agentManager":{"namespace":namespace,"existingControlPlaneConfigMap":"fixture-control","kubernetesApiEgress":[{"cidr":"10.43.0.1/32","port":443}]}});
    let path = directory.join("values.json");
    std::fs::write(&path, serde_json::to_vec(&values)?)?;
    let bytes = success(command(
        "helm",
        &[
            "template",
            namespace,
            "deploy/helm/veoveo",
            "--namespace",
            "veoveo",
            "--values",
            path.to_str().unwrap(),
        ],
        None,
    )?)?;
    let mut objects = Vec::new();
    for document in serde_yaml_ng::Deserializer::from_str(std::str::from_utf8(&bytes)?) {
        let object = Value::deserialize(document)?;
        if object["metadata"]["namespace"] == namespace
            || object["kind"] == "Namespace"
            || object["kind"] == "NetworkPolicy"
            || object["kind"]
                .as_str()
                .is_some_and(|kind| kind.starts_with("ValidatingAdmissionPolicy"))
        {
            objects.push(object);
        }
    }
    Ok((
        Installation {
            directory,
            objects: Vec::new(),
        },
        config,
        snapshot,
        map,
    ))
    .map(|(mut installation, config, snapshot, map)| {
        // Rendered objects are kept separately until the first successful write.
        std::fs::write(
            installation.directory.join("objects.json"),
            serde_json::to_vec(&objects).unwrap(),
        )
        .unwrap();
        installation.objects.clear();
        (installation, config, snapshot, map)
    })
}

#[test]
#[ignore = "creates an isolated admission fixture in the current Kubernetes context"]
fn installed_admission_rejects_workload_and_credential_escalation() -> Result<()> {
    let namespace = format!(
        "agent-admission-{}",
        &uuid::Uuid::now_v7().simple().to_string()[..12]
    );
    let (mut installation, config, snapshot, map) = rendered(&namespace, true)?;
    let objects: Vec<Value> =
        serde_json::from_slice(&std::fs::read(installation.directory.join("objects.json"))?)?;
    for kind in [
        "Namespace",
        "ServiceAccount",
        "Role",
        "RoleBinding",
        "ValidatingAdmissionPolicy",
        "ValidatingAdmissionPolicyBinding",
    ] {
        for object in objects.iter().filter(|object| object["kind"] == kind) {
            success(command("kubectl", &["create", "-f", "-"], Some(object))?)?;
            installation.objects.push(object.clone());
        }
    }
    let deadline = Instant::now() + Duration::from_secs(15);
    for policy in installation
        .objects
        .iter()
        .filter(|object| object["kind"] == "ValidatingAdmissionPolicy")
    {
        loop {
            let bytes = success(command(
                "kubectl",
                &[
                    "get",
                    "validatingadmissionpolicy",
                    policy["metadata"]["name"].as_str().unwrap(),
                    "-o",
                    "json",
                ],
                None,
            )?)?;
            let observed: Value = serde_json::from_slice(&bytes)?;
            if let Some(status) = observed.pointer("/status/typeChecking") {
                anyhow::ensure!(
                    status["expressionWarnings"]
                        .as_array()
                        .is_none_or(Vec::is_empty),
                    "policy type warnings: {status}"
                );
                break;
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "admission type checking did not finish"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    // Allow the API server's admission binding cache to observe the fixture.
    let actor = format!("system:serviceaccount:{namespace}:veoveo-agent-manager");
    let template = &config.templates[0];
    let items = resources::configuration_items(&map, template)?;
    let deployment = serde_json::to_value(resources::deployment(
        &config,
        &snapshot,
        template,
        &config.models[0],
        items,
    )?)?;
    let request = |body: &Value, actor: &str| {
        command(
            "kubectl",
            &[
                "create",
                "--dry-run=server",
                "--as",
                actor,
                "-f",
                "-",
                "-o",
                "json",
            ],
            Some(body),
        )
    };
    let admitted = request(&deployment, &actor)?;
    let admitted: Deployment = serde_json::from_slice(&success(admitted)?)?;
    anyhow::ensure!(!admitted.spec.template.spec.containers[0].volume_mounts[1].read_only);
    let mut rejected = deployment.clone();
    rejected["spec"]["template"]["spec"]["containers"][0]["image"] =
        json!("unapproved.invalid/image:latest");
    loop {
        let output = request(&rejected, &actor)?;
        if !output.status.success() {
            anyhow::ensure!(
                String::from_utf8_lossy(&output.stderr).contains("ValidatingAdmissionPolicy"),
                "unexpected denial: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "admission binding was not enforced"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    for (pointer, value) in [
        (
            "/spec/template/spec/serviceAccountName",
            json!("veoveo-agent-manager"),
        ),
        (
            "/spec/template/spec/automountServiceAccountToken",
            json!(true),
        ),
        ("/spec/template/spec/hostNetwork", json!(true)),
        (
            "/spec/template/spec/containers/0/command",
            json!(["/bin/sh"]),
        ),
        (
            "/spec/template/spec/containers/0/resources/limits/cpu",
            json!("8"),
        ),
        (
            "/spec/template/spec/containers/0/securityContext/allowPrivilegeEscalation",
            json!(true),
        ),
        (
            "/spec/template/metadata/labels/app.kubernetes.io~1component",
            json!("agent-manager"),
        ),
        ("/spec/replicas", json!(2)),
    ] {
        let mut body = deployment.clone();
        let (parent, key) = pointer.rsplit_once('/').unwrap();
        body.pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert(key.replace("~1", "/"), value);
        let output = request(&body, &actor)?;
        anyhow::ensure!(
            !output.status.success(),
            "unsafe mutation was admitted: {pointer}"
        );
    }
    let mut destination = deployment.clone();
    let env = destination
        .pointer_mut("/spec/template/spec/containers/0/env")
        .unwrap()
        .as_array_mut()
        .unwrap();
    env.iter_mut()
        .find(|value| value["name"] == "VEOVEO_AGENT_MODEL_URL")
        .unwrap()["value"] = json!("https://unapproved.invalid/v1");
    anyhow::ensure!(
        !request(&destination, &actor)?.status.success(),
        "unapproved model destination was admitted"
    );
    let mut foreign_memory = deployment.clone();
    foreign_memory["spec"]["template"]["spec"]["volumes"][1]["persistentVolumeClaim"]["claimName"] =
        json!("another-agent-memory");
    anyhow::ensure!(
        !request(&foreign_memory, &actor)?.status.success(),
        "foreign memory was admitted"
    );
    for secret in ["installation-private", "another-agent-key"] {
        let mut body = deployment.clone();
        let env = body
            .pointer_mut("/spec/template/spec/containers/0/env")
            .unwrap()
            .as_array_mut()
            .unwrap();
        let entry = env
            .iter_mut()
            .find(|value| value["name"] == "VEOVEO_MANAGED_PRIVATE_KEY")
            .unwrap();
        entry["valueFrom"]["secretKeyRef"]["name"] = json!(secret);
        anyhow::ensure!(
            !request(&body, &actor)?.status.success(),
            "foreign credential was admitted"
        );
    }
    let mut pod = json!({"apiVersion":"v1","kind":"Pod","metadata":deployment["spec"]["template"]["metadata"],"spec":deployment["spec"]["template"]["spec"]});
    pod["metadata"]["name"] = json!(format!("{}-fixture", snapshot.instance.resources.workload));
    pod["metadata"]["namespace"] = json!(namespace);
    success(command(
        "kubectl",
        &["create", "--dry-run=server", "-f", "-"],
        Some(&pod),
    )?)?;
    anyhow::ensure!(
        !request(&pod, &actor)?.status.success(),
        "manager unexpectedly has direct Pod creation authority"
    );
    let mut hijack = deployment.clone();
    hijack["metadata"]["name"] = json!("veoveo-agent-manager");
    anyhow::ensure!(
        !request(&hijack, &actor)?.status.success(),
        "manager could replace its own privileged workload"
    );
    let pvc = serde_json::to_value(resources::volume_claim(&snapshot.instance, template))?;
    success(request(&pvc, &actor)?)?;
    let mut oversized = pvc.clone();
    oversized["spec"]["resources"]["requests"]["storage"] = json!("999Gi");
    anyhow::ensure!(
        !request(&oversized, &actor)?.status.success(),
        "oversized memory was admitted"
    );
    let secret = json!({"apiVersion":"v1","kind":"Secret","metadata":resources::metadata(&snapshot.instance, &snapshot.instance.resources.credential_secret),"immutable":true,"type":"Opaque","data":{"kid":"Zml4dHVyZQ==","private-key-der-b64":"Zml4dHVyZQ=="}});
    success(request(&secret, &actor)?)?;
    let mut extra_key = secret.clone();
    extra_key["data"]["installation-password"] = json!("Zml4dHVyZQ==");
    anyhow::ensure!(
        !request(&extra_key, &actor)?.status.success(),
        "extra secret material was admitted"
    );
    // A replaced installation template must not prevent retiring its old image.
    // Keep zero replicas throughout: no fixture kernel or memory writer starts.
    let mut stopped = deployment.clone();
    stopped["spec"]["replicas"] = json!(0);
    stopped["metadata"]["finalizers"] = json!(["veoveo.ai/admission-fixture"]);
    let created = success(command(
        "kubectl",
        &["create", "--as", &actor, "-f", "-", "-o", "json"],
        Some(&stopped),
    )?)?;
    let stopped: Value = serde_json::from_slice(&created)?;
    installation.objects.push(stopped.clone());
    let policy_name = format!("{namespace}-managed-deployments");
    let mut retired: Value = serde_json::from_slice(&success(command(
        "kubectl",
        &[
            "get",
            "validatingadmissionpolicy",
            &policy_name,
            "-o",
            "json",
        ],
        None,
    )?)?)?;
    retired["spec"]["validations"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "expression":"false", "message":"Fixture retires all executable images."
        }));
    success(command("kubectl", &["replace", "-f", "-"], Some(&retired))?)?;
    let retired_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let mut attempt: Value = serde_json::from_slice(&success(command(
            "kubectl",
            &[
                "get",
                "deployment",
                &snapshot.instance.resources.workload,
                "--namespace",
                &namespace,
                "-o",
                "json",
            ],
            None,
        )?)?)?;
        attempt["metadata"]["annotations"][GENERATION] = json!("3");
        let output = command(
            "kubectl",
            &["replace", "--dry-run=server", "--as", &actor, "-f", "-"],
            Some(&attempt),
        )?;
        if !output.status.success() {
            anyhow::ensure!(
                String::from_utf8_lossy(&output.stderr)
                    .contains("Fixture retires all executable images"),
                "unexpected retirement denial: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            break;
        }
        anyhow::ensure!(
            Instant::now() < retired_deadline,
            "retired policy did not converge"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    success(command(
        "kubectl",
        &[
            "delete",
            "--as",
            &actor,
            "--cascade=foreground",
            "--wait=false",
            "-f",
            "-",
        ],
        Some(&stopped),
    )?)?;
    let mut finalizing: Value = serde_json::from_slice(&success(command(
        "kubectl",
        &[
            "get",
            "deployment",
            &snapshot.instance.resources.workload,
            "--namespace",
            &namespace,
            "-o",
            "json",
        ],
        None,
    )?)?)?;
    anyhow::ensure!(finalizing["metadata"]["deletionTimestamp"].is_string());
    let mut changed_spec = finalizing.clone();
    changed_spec["spec"]["replicas"] = json!(1);
    let rejected = command(
        "kubectl",
        &["replace", "--dry-run=server", "--as", &actor, "-f", "-"],
        Some(&changed_spec),
    )?;
    anyhow::ensure!(
        !rejected.status.success()
            && String::from_utf8_lossy(&rejected.stderr)
                .contains("Fixture retires all executable images"),
        "retirement did not reject the specification change through admission"
    );
    let mut changed_owner = finalizing.clone();
    changed_owner["metadata"]["labels"][OWNER_LABEL] =
        json!("agent-ffffffffffffffffffffffffffffffff");
    let rejected = command(
        "kubectl",
        &["replace", "--dry-run=server", "--as", &actor, "-f", "-"],
        Some(&changed_owner),
    )?;
    anyhow::ensure!(
        !rejected.status.success()
            && String::from_utf8_lossy(&rejected.stderr).contains("denied request"),
        "retirement did not reject the ownership change through admission"
    );
    finalizing["metadata"]["finalizers"]
        .as_array_mut()
        .unwrap()
        .retain(|value| value != "veoveo.ai/admission-fixture");
    success(command(
        "kubectl",
        &["replace", "--as", &actor, "-f", "-"],
        Some(&finalizing),
    )?)?;
    success(command(
        "kubectl",
        &[
            "wait",
            "--for=delete",
            "deployment",
            &snapshot.instance.resources.workload,
            "--namespace",
            &namespace,
            "--timeout=15s",
        ],
        None,
    )?)?;
    installation.cleanup()?;
    Ok(())
}

#[test]
fn managed_network_isolation_preserves_the_installation_ingress_mode() -> Result<()> {
    for enabled in [false, true] {
        let namespace = format!("agent-network-{}", uuid::Uuid::now_v7().simple());
        let (mut installation, _, _, _) = rendered(&namespace, enabled)?;
        let objects: Vec<Value> =
            serde_json::from_slice(&std::fs::read(installation.directory.join("objects.json"))?)?;
        let policies: Vec<_> = objects
            .iter()
            .filter(|o| o["kind"] == "NetworkPolicy")
            .collect();
        for name in [
            "managed-default-deny",
            "managed-dns",
            "managed-store",
            "managed-controller-api",
            "managed-kernel-gateway",
        ] {
            anyhow::ensure!(
                policies
                    .iter()
                    .any(|o| o["metadata"]["name"] == name
                        && o["metadata"]["namespace"] == namespace),
                "managed policy {name} must always apply"
            );
        }
        for name in ["managed-gateway-ingress", "managed-surrealdb-ingress"] {
            anyhow::ensure!(
                policies.iter().any(|o| o["metadata"]["name"] == name) == enabled,
                "{name} must only extend existing installation isolation"
            );
        }
        installation.cleanup()?;
    }
    Ok(())
}
