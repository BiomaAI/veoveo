//! Credentials remain in process memory; command output never prints their values.

use anyhow::{Context, Result, bail, ensure};
use base64::Engine;
use serde::Deserialize;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use veoveo_artifact_service::ObjectStoreConfig;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Env {
    name: String,
    value: Option<String>,
    value_from: Option<ValueFrom>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ValueFrom {
    secret_key_ref: Option<SecretRef>,
}
#[derive(Deserialize)]
struct SecretRef {
    name: String,
    key: String,
}

pub struct Fixture {
    pub config: ObjectStoreConfig,
    child: Child,
    _logs: tempfile::TempDir,
}

fn kubectl(context: &str, namespace: &str) -> Command {
    let mut command = Command::new("kubectl");
    command
        .args(["--context", context, "--namespace", namespace])
        .stderr(Stdio::null());
    command
}

fn output(command: &mut Command) -> Result<Vec<u8>> {
    let result = command
        .output()
        .context("running the Kubernetes fixture command")?;
    ensure!(
        result.status.success(),
        "Kubernetes fixture command failed with {}",
        result.status
    );
    ensure!(
        result.stdout.len() <= 1024 * 1024,
        "Kubernetes fixture response exceeded its bound"
    );
    Ok(result.stdout)
}

impl Fixture {
    pub async fn start() -> Result<Self> {
        let context = std::env::var("VEOVEO_UPLOAD_S3_CONTEXT")
            .context("VEOVEO_UPLOAD_S3_CONTEXT must identify the installation under test")?;
        let namespace =
            std::env::var("VEOVEO_UPLOAD_S3_NAMESPACE").unwrap_or_else(|_| "veoveo".into());
        let vars: Vec<Env> =
            serde_json::from_slice(&output(kubectl(&context, &namespace).args([
                "get",
                "deployment",
                "artifact-service",
                "-o",
                "jsonpath={.spec.template.spec.containers[?(@.name==\"artifact-service\")].env}",
            ]))?)?;
        let var = |key: &str| {
            vars.iter()
                .find(|value| value.name == key)
                .context("required artifact-service environment entry is missing")
        };
        let value = |key: &str| {
            var(key)?
                .value
                .clone()
                .context("artifact-service value is not a literal")
        };
        ensure!(
            value("ARTIFACT_S3_ENDPOINT")? == "http://rustfs:9000",
            "this fixture requires the installation's RustFS service"
        );
        let secret = |key: &str| -> Result<String> {
            let reference = &var(key)?
                .value_from
                .as_ref()
                .context("missing secret reference")?
                .secret_key_ref
                .as_ref()
                .context("missing S3 secret reference")?;
            ensure!(
                !reference.key.is_empty()
                    && reference
                        .key
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)),
                "invalid Kubernetes secret key"
            );
            let selector = format!("jsonpath={{.data.{}}}", reference.key.replace('.', "\\."));
            let encoded = output(kubectl(&context, &namespace).args([
                "get",
                "secret",
                &reference.name,
                "-o",
                &selector,
            ]))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .context("decoding S3 fixture credential")?;
            String::from_utf8(bytes).context("S3 fixture credential is not UTF-8")
        };
        let access_key_id = secret("ARTIFACT_S3_ACCESS_KEY_ID")?;
        let secret_access_key =
            secrecy::SecretString::from(secret("ARTIFACT_S3_SECRET_ACCESS_KEY")?);
        let bucket = value("ARTIFACT_S3_BUCKET")?;
        let region = value("ARTIFACT_S3_REGION")?;
        let socket = TcpListener::bind("127.0.0.1:0")?;
        let port = socket.local_addr()?.port();
        drop(socket);
        let logs = tempfile::tempdir()?;
        let log = std::fs::File::create(logs.path().join("port-forward.log"))?;
        let child = kubectl(&context, &namespace)
            .args([
                "port-forward",
                "--address",
                "127.0.0.1",
                "service/rustfs",
                &format!("{port}:9000"),
            ])
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()
            .context("starting S3 fixture port-forward")?;
        let mut fixture = Self {
            config: ObjectStoreConfig::S3 {
                endpoint: Some(format!("http://127.0.0.1:{port}")),
                bucket,
                region,
                access_key_id,
                secret_access_key,
                allow_http: true,
            },
            child,
            _logs: logs,
        };
        let started = Instant::now();
        loop {
            ensure!(
                fixture.child.try_wait()?.is_none(),
                "S3 fixture port-forward exited"
            );
            if tokio::net::TcpStream::connect(("127.0.0.1", port))
                .await
                .is_ok()
            {
                break;
            }
            if started.elapsed() > Duration::from_secs(30) {
                bail!("S3 fixture port-forward did not become ready");
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Ok(fixture)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
