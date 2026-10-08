//! Diagnostic-only admission of the generation-one chart policies; no installation.
use super::{Args, assertions, fixture::Fixture, process};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};
use veoveo_modules::{
    CredentialRevision, InstallationGeneration, ModuleName, ModulePlanDocument,
    ModuleSelectionDocument,
};

const MAX_PLAN_BYTES: u64 = 2 * 1024 * 1024;
const MAX_POLICIES: usize = 16;

pub(super) struct AdmittedPolicy {
    object: Value,
    resource: process::KubernetesResource,
    name: String,
}
impl AdmittedPolicy {
    fn admit(object: &Value) -> Result<Self> {
        ensure!(
            object["kind"] == "ValidatingAdmissionPolicy"
                && object["apiVersion"] == "admissionregistration.k8s.io/v1",
            "policy dry-run accepts only admissionregistration v1 ValidatingAdmissionPolicy"
        );
        ensure!(
            object["metadata"].get("namespace").is_none(),
            "policy dry-run requires a cluster-scoped policy"
        );
        let name = object["metadata"]["name"]
            .as_str()
            .context("policy name absent")?;
        let resource = process::KubernetesResource::new("ValidatingAdmissionPolicy", None, name)?;
        ensure!(
            object["spec"]
                .as_object()
                .is_some_and(|spec| !spec.is_empty()),
            "policy body is empty or not an object"
        );
        Ok(Self {
            object: object.clone(),
            resource,
            name: name.into(),
        })
    }
    pub(super) fn object(&self) -> &Value {
        &self.object
    }
    pub(super) fn resource(&self) -> &process::KubernetesResource {
        &self.resource
    }
    pub(super) fn admit_response(&self, bytes: &[u8]) -> Result<()> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Response {
            api_version: String,
            kind: String,
            metadata: Metadata,
        }
        #[derive(Deserialize)]
        struct Metadata {
            name: String,
            namespace: Option<String>,
        }
        let response: Response = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("policy dry-run response is not an admitted object"))?;
        ensure!(
            response.api_version == "admissionregistration.k8s.io/v1"
                && response.kind == "ValidatingAdmissionPolicy"
                && response.metadata.name == self.name
                && response.metadata.namespace.is_none(),
            "policy dry-run response identity differs from selected policy"
        );
        Ok(())
    }
}
pub(super) fn select_policies(objects: &[Value]) -> Result<Vec<AdmittedPolicy>> {
    let policies = objects
        .iter()
        .filter(|object| object["kind"] == "ValidatingAdmissionPolicy")
        .map(AdmittedPolicy::admit)
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        !policies.is_empty() && policies.len() <= MAX_POLICIES,
        "policy diagnostic requires between one and sixteen rendered policies"
    );
    let mut names = BTreeSet::new();
    ensure!(
        policies.iter().all(|policy| names.insert(&policy.name)),
        "rendered policy names repeat"
    );
    Ok(policies)
}
pub(super) fn admit_plan(plan: &ModulePlanDocument, args: &Args) -> Result<()> {
    let (generation, enabled, _) = assertions::GENERATIONS[0];
    let expected = ModuleSelectionDocument::new(
        enabled
            .iter()
            .map(|name| ModuleName::new(*name))
            .collect::<Result<Vec<_>, _>>()?,
        InstallationGeneration::new(generation)?,
        CredentialRevision::new("fixture-runtime-1")?,
    )?;
    ensure!(
        plan.selection()? == expected,
        "policy plan requires the fixture's exact generation-one selection and credential revision"
    );
    ensure!(
        plan.composition().as_str() == args.gateway_image.digest.as_str(),
        "policy plan composition differs from selected gateway digest"
    );
    Ok(())
}
pub(super) fn read_plan(path: &Path, args: &Args) -> Result<(ModulePlanDocument, Vec<u8>)> {
    let metadata = fs::symlink_metadata(path).context("policy plan file unavailable")?;
    ensure!(
        metadata.is_file() && metadata.permissions().mode() & 0o077 == 0,
        "policy plan must be a private regular file"
    );
    ensure!(
        metadata.len() > 0 && metadata.len() <= MAX_PLAN_BYTES,
        "policy plan is empty or exceeds 2 MiB"
    );
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_PLAN_BYTES + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        !bytes.is_empty() && bytes.len() as u64 <= MAX_PLAN_BYTES,
        "policy plan is empty or exceeds 2 MiB"
    );
    let plan: ModulePlanDocument = serde_json::from_slice(&bytes).map_err(|_| {
        anyhow::anyhow!("policy plan does not admit through the owner ModulePlan document")
    })?;
    admit_plan(&plan, args)?;
    Ok((plan, bytes))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PolicyResult {
    name: String,
    rendered_sha256: String,
    server_accepted: bool,
    failure: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema_version: &'static str,
    scope: &'static str,
    installation_qualified: bool,
    context: String,
    gateway_image: String,
    manager_image: String,
    kernel_image: String,
    server_version: Option<String>,
    plan_sha256: Option<String>,
    provenance_limit: &'static str,
    policies: Vec<PolicyResult>,
    failure: Option<String>,
}
impl Evidence {
    fn new(args: &Args) -> Self {
        Self {
            schema_version: "veoveo.ai/module-policy-dry-run-evidence/v1",
            scope: "policy-server-dry-run-only",
            installation_qualified: false,
            context: args.context.clone(),
            gateway_image: args.gateway_image.reference(),
            manager_image: args.manager_image.reference(),
            kernel_image: args.kernel_image.reference(),
            server_version: None,
            plan_sha256: None,
            provenance_limit: "Supplied plan admission binds selection and composition; image producer provenance requires the separate operational receipt.",
            policies: Vec::new(),
            failure: None,
        }
    }
}
pub(super) fn verify(args: &Args) -> Result<()> {
    ensure!(
        args.policy_server_dry_run && args.policy_plan.is_some(),
        "policy diagnostic requires explicit mode and plan"
    );
    ensure!(
        !args.evidence_output.exists(),
        "policy diagnostic output already exists"
    );
    let parent = args
        .evidence_output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .context("policy output requires an explicit private parent directory")?;
    let metadata = fs::symlink_metadata(parent)?;
    ensure!(
        metadata.is_dir() && metadata.permissions().mode() & 0o077 == 0,
        "policy output directory must be private"
    );
    ensure!(
        !args.context.is_empty()
            && !args.context.starts_with('-')
            && args.context.len() <= 256
            && !args.context.chars().any(char::is_control),
        "policy diagnostic context is invalid"
    );
    let mut evidence = Evidence::new(args);
    // This creates local trust/config files only. Never initialize namespaces or
    // run the image producer, Store, Jobs, controllers or installation lifecycle.
    let mut fixture = None;
    let result: Result<()> = (|| {
        let (plan, bytes) = read_plan(args.policy_plan.as_ref().unwrap(), args)?;
        evidence.plan_sha256 = Some(hex::encode(Sha256::digest(&bytes)));
        fixture = Some(Fixture::create(args)?);
        let fixture = fixture.as_mut().expect("local fixture initialized");
        evidence.server_version = Some(fixture.observe_policy_render_context()?);
        let render = fixture.render(plan, &bytes)?;
        // Admit the entire nonempty set before any server dry-run request.
        let policies = select_policies(&render.objects)?;
        for policy in policies {
            let mut receipt = PolicyResult {
                name: policy.name.clone(),
                rendered_sha256: hex::encode(Sha256::digest(serde_json::to_vec(policy.object())?)),
                server_accepted: false,
                failure: None,
            };
            let result = fixture.dry_run_policy(&policy);
            match &result {
                Ok(()) => receipt.server_accepted = true,
                Err(error) => {
                    receipt.failure = Some(fixture.redact_diagnostics(format!("{error:#}")))
                }
            }
            evidence.policies.push(receipt);
            result?; // Unknown and failed observations stop; never repeat a request.
        }
        Ok(())
    })();
    if let Err(error) = &result {
        evidence.failure = Some(
            if evidence
                .policies
                .last()
                .is_some_and(|policy| policy.failure.is_some())
            {
                "Policy dry-run failed; the first failed policy result contains the private cause."
                    .into()
            } else {
                fixture
                    .as_ref()
                    .map(|fixture| fixture.redact_diagnostics(format!("{error:#}")))
                    .unwrap_or_else(|| {
                        "Policy setup failed before the local secret registry was available.".into()
                    })
            },
        );
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&args.evidence_output)?;
    serde_json::to_writer_pretty(&mut file, &evidence)?;
    file.sync_all()?;
    result?;
    println!(
        "Policy server dry-run diagnostics completed: {}",
        args.evidence_output.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn args() -> Args {
        let image = format!("registry.invalid/fixture@sha256:{}", "a".repeat(64));
        Args {
            context: "unused".into(),
            gateway_image: image.parse().unwrap(),
            manager_image: image.parse().unwrap(),
            kernel_image: image.parse().unwrap(),
            evidence_output: "/tmp/unused-evidence".into(),
            policy_server_dry_run: true,
            policy_plan: Some("/tmp/unused-plan".into()),
        }
    }
    fn policy() -> Value {
        json!({"apiVersion":"admissionregistration.k8s.io/v1","kind":"ValidatingAdmissionPolicy","metadata":{"name":"owned-policy"},"spec":{"validations":[{"expression":"true"}]}})
    }
    #[test]
    fn policy_set_admits_only_nonempty_cluster_policies_and_exact_response() -> Result<()> {
        let good = policy();
        let admitted = select_policies(&[
            json!({"kind":"Secret","metadata":{"name":"private"}}),
            good.clone(),
        ])?;
        assert_eq!(admitted.len(), 1);
        admitted[0].admit_response(&serde_json::to_vec(&good)?)?;
        assert!(select_policies(&[]).is_err());
        assert!(select_policies(&[json!({"kind":"Deployment"})]).is_err());
        assert!(select_policies(&[good.clone(), good.clone()]).is_err());
        for bad in [
            json!({"apiVersion":"v1","kind":"Secret","metadata":{"name":"owned-policy"},"spec":{"a":1}}),
            json!({"apiVersion":"admissionregistration.k8s.io/v1","kind":"ValidatingAdmissionPolicy","metadata":{"name":"owned-policy","namespace":"foreign"},"spec":{"a":1}}),
            json!({"apiVersion":"admissionregistration.k8s.io/v1","kind":"ValidatingAdmissionPolicy","metadata":{"name":"owned-policy"},"spec":{}}),
            json!({"apiVersion":"admissionregistration.k8s.io/v1beta1","kind":"ValidatingAdmissionPolicy","metadata":{"name":"owned-policy"},"spec":{"a":1}}),
        ] {
            assert!(AdmittedPolicy::admit(&bad).is_err());
        }
        let mut foreign = good;
        foreign["metadata"]["name"] = json!("foreign");
        assert!(
            admitted[0]
                .admit_response(&serde_json::to_vec(&foreign)?)
                .is_err()
        );
        assert!(admitted[0].admit_response(b"unknown-success").is_err());
        Ok(())
    }
    #[test]
    fn supplied_plan_requires_private_bounded_file_before_server_observation() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("plan.json");
        fs::write(&path, b"secret-invalid-json")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644))?;
        assert!(read_plan(&path, &args()).is_err());
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let error = read_plan(&path, &args()).err().unwrap();
        assert!(!format!("{error:#}").contains("secret-invalid-json"));
        fs::write(&path, b"")?;
        assert!(read_plan(&path, &args()).is_err());
        fs::File::create(&path)?.set_len(MAX_PLAN_BYTES + 1)?;
        assert!(read_plan(&path, &args()).is_err());
        let link = dir.path().join("link");
        std::os::unix::fs::symlink(&path, &link)?;
        assert!(read_plan(&link, &args()).is_err());
        Ok(())
    }
    #[test]
    fn invalid_plan_emits_private_diagnostic_report_before_any_server_setup() -> Result<()> {
        let directory = tempfile::tempdir()?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o700))?;
        let mut args = args();
        let plan = directory.path().join("plan.json");
        fs::write(&plan, "private-invalid-input-sentinel")?;
        fs::set_permissions(&plan, fs::Permissions::from_mode(0o600))?;
        args.policy_plan = Some(plan);
        args.evidence_output = directory.path().join("result.json");
        assert!(verify(&args).is_err());
        assert_eq!(
            fs::metadata(&args.evidence_output)?.permissions().mode() & 0o777,
            0o600
        );
        let bytes = fs::read(&args.evidence_output)?;
        assert!(!String::from_utf8_lossy(&bytes).contains("private-invalid-input-sentinel"));
        let report: Value = serde_json::from_slice(&bytes)?;
        assert_eq!(report["installationQualified"], false);
        assert_eq!(report["policies"], json!([]));
        assert!(report["failure"].is_string());
        assert!(
            verify(&args).is_err(),
            "diagnostics must never overwrite a prior receipt"
        );
        assert_eq!(fs::read(&args.evidence_output)?, bytes);
        fs::remove_file(&args.evidence_output)?;
        fs::set_permissions(directory.path(), fs::Permissions::from_mode(0o755))?;
        assert!(verify(&args).is_err());
        assert!(!args.evidence_output.exists());
        Ok(())
    }
    #[test]
    fn diagnostic_cli_and_report_cannot_claim_installation_or_omit_plan() {
        use clap::Parser;
        let image = args().gateway_image.reference();
        let base = [
            "deployment-smoke",
            "module-installation-verify",
            "--context",
            "unused",
            "--gateway-image",
            &image,
            "--manager-image",
            &image,
            "--kernel-image",
            &image,
            "--evidence-output",
            "/tmp/evidence",
        ];
        let mut argv = base.to_vec();
        argv.push("--policy-server-dry-run");
        assert!(crate::Args::try_parse_from(&argv).is_err());
        argv.extend(["--policy-plan", "/tmp/plan"]);
        assert!(crate::Args::try_parse_from(&argv).is_ok());
        let mut argv = base.to_vec();
        argv.extend(["--policy-plan", "/tmp/plan"]);
        assert!(crate::Args::try_parse_from(&argv).is_err());
        let report = serde_json::to_value(Evidence::new(&args())).unwrap();
        assert_eq!(report["installationQualified"], false);
        assert_eq!(report["scope"], "policy-server-dry-run-only");
        assert_eq!(
            report["schemaVersion"],
            "veoveo.ai/module-policy-dry-run-evidence/v1"
        );
        for installed in ["cases", "managedRecovery", "namespace", "agentNamespace"] {
            assert!(report.get(installed).is_none());
        }
    }
}
