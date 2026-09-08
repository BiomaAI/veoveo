//! API-normalized intent after a completed apply; every request is dry-run only.
use std::{
    io::Write,
    process::{Command, Stdio},
};

use anyhow::{Context, Result, ensure};
use serde_json::Value;
use veoveo_deploy_contract::components::AtomicTarget;

pub(super) fn project(context: &str, target: &AtomicTarget, desired: &Value) -> Result<Value> {
    let (mut command, input) = request(context, target, desired)?;
    let input = serde_json::to_vec(&input)?;
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting Kubernetes normalization request")?;
    let mut stdin = child
        .stdin
        .take()
        .context("normalization request has no stdin")?;
    let write_result = stdin.write_all(&input);
    drop(stdin);
    let output = child
        .wait_with_output()
        .context("waiting for Kubernetes normalization")?;
    write_result.context("writing Kubernetes normalization input")?;
    // The body is public configuration, but an admission response may include
    // arbitrary object contents. Keep it out of diagnostics and receipts.
    ensure!(
        output.status.success(),
        "Kubernetes rejected the dry-run normalization request ({})",
        output.status
    );
    serde_json::from_slice(&output.stdout).context("decoding the API-normalized object")
}

fn request(context: &str, target: &AtomicTarget, desired: &Value) -> Result<(Command, Value)> {
    let mut command = Command::new("kubectl");
    command.args([
        "--context",
        context,
        "apply",
        "--dry-run=server",
        "--output=json",
        "--request-timeout=10s",
        "-f",
        "-",
    ]);
    let mut input = desired.clone();
    if let AtomicTarget::HelmRelease { namespace, name } = target {
        crate::helm_bundle::own_object(&mut input, namespace, name)?;
        // Helm 4 applies server-side with its CLI name as field manager. Do not
        // force conflicts or omit ownership fields that Helm actually submitted.
        command.args(["--server-side", "--field-manager=helm"]);
    }
    Ok((command, input))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn projection_is_dry_run_and_uses_the_actual_apply_manager() {
        let desired = json!({"apiVersion":"v1","kind":"ConfigMap","metadata":{"name":"public","namespace":"test"},"data":{}});
        let target = AtomicTarget::HelmRelease {
            namespace: "test".into(),
            name: "platform".into(),
        };
        let (command, input) = request("explicit-context", &target, &desired).unwrap();
        let args = command
            .get_args()
            .map(|arg| arg.to_str().unwrap())
            .collect::<Vec<_>>();
        assert!(args.contains(&"--dry-run=server"));
        assert!(args.contains(&"--server-side"));
        assert!(args.contains(&"--field-manager=helm"));
        assert!(!args.contains(&"--force-conflicts"));
        assert_eq!(
            input["metadata"]["annotations"]["meta.helm.sh/release-name"],
            "platform"
        );
        assert_eq!(
            input["metadata"]["labels"]["app.kubernetes.io/managed-by"],
            "Helm"
        );
        assert!(desired["metadata"].get("annotations").is_none());
        let raw = AtomicTarget::ManifestSet {
            name: "public-resources".into(),
        };
        let (command, input) = request("explicit-context", &raw, &desired).unwrap();
        assert!(command.get_args().any(|arg| arg == "--dry-run=server"));
        assert!(!command.get_args().any(|arg| arg == "--server-side"));
        assert_eq!(input, desired);
        assert!(
            request(
                "explicit-context",
                &target,
                &json!({"metadata":{"annotations":{"meta.helm.sh/release-name":"foreign"}}})
            )
            .is_err()
        );
    }
}
