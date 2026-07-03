use super::*;
use std::fmt::Write as _;

use sha2::{Digest, Sha256};

const VAULT_IMAGE: &str = "hashicorp/vault:2.0.3";
const VAULT_ROOT_TOKEN: &str = "veoveo-smoke-root-token";
const VAULT_SECRET_VALUE: &str = "vault-backed-smoke-secret";

pub(crate) async fn gateway_vault_secrets(gateway: &Path, base_control_plane: &Path) -> Result<()> {
    assert_executable(gateway)?;
    run_checked(Path::new("docker"), ["--version".into()], [])?;

    let tmpdir = smoke_tmpdir()?;
    let mut cleanup = TmpDirGuard::new(tmpdir.clone());
    println!("smoke workspace: {}", tmpdir.display());

    let vault_port = 18820u16;
    let vault_base = format!("http://127.0.0.1:{vault_port}");
    let vault_log = tmpdir.join("vault.log");
    let control_plane = tmpdir.join("gateway.vault-secret.json");
    let container_name = format!("veoveo-vault-smoke-{vault_port}-{}", uuid::Uuid::new_v4());
    let _container = ContainerGuard::new(container_name.clone());
    let mut vault = ChildGuard::spawn(
        Path::new("docker"),
        [
            "run".into(),
            "--rm".into(),
            "--name".into(),
            container_name.into(),
            "--cap-add".into(),
            "IPC_LOCK".into(),
            "-p".into(),
            format!("127.0.0.1:{vault_port}:8200").into(),
            VAULT_IMAGE.into(),
            "server".into(),
            "-dev".into(),
            format!("-dev-root-token-id={VAULT_ROOT_TOKEN}").into(),
            "-dev-listen-address=0.0.0.0:8200".into(),
        ],
        [],
        &vault_log,
    )?;
    wait_for_http_client(
        &reqwest::Client::new(),
        &format!("{vault_base}/v1/sys/health"),
        StatusCode::OK,
    )
    .await?;

    let http = reqwest::Client::new();
    http.post(format!("{vault_base}/v1/secret/data/veoveo/smoke"))
        .header("X-Vault-Token", VAULT_ROOT_TOKEN)
        .json(&serde_json::json!({
            "data": {
                "value": VAULT_SECRET_VALUE
            }
        }))
        .send()
        .await?
        .error_for_status()?;

    write_vault_secret_control_plane(base_control_plane, &control_plane)?;
    let output = run_checked(
        gateway,
        [
            "resolve-secret".into(),
            "--control-plane".into(),
            control_plane.as_os_str().to_os_string(),
            "--secret-id".into(),
            "vault_smoke_secret".into(),
            "--purpose".into(),
            "token_exchange_credential".into(),
        ],
        [
            ("VAULT_ADDR", vault_base.clone().into()),
            ("VAULT_TOKEN", VAULT_ROOT_TOKEN.into()),
        ],
    )?;
    let evidence: Value = serde_json::from_str(&output)?;
    let expected_sha256 = sha256_hex(VAULT_SECRET_VALUE.as_bytes());
    if evidence.get("id").and_then(Value::as_str) != Some("vault_smoke_secret")
        || evidence.get("source").and_then(Value::as_str) != Some("vault")
        || evidence.get("purpose").and_then(Value::as_str) != Some("token_exchange_credential")
        || evidence.get("byte_length").and_then(Value::as_u64)
            != Some(VAULT_SECRET_VALUE.len() as u64)
        || evidence.get("sha256").and_then(Value::as_str) != Some(expected_sha256.as_str())
    {
        bail!("unexpected Vault secret resolution evidence: {evidence}");
    }

    vault.stop();
    cleanup.remove_on_drop();
    println!("gateway Vault secret smoke ok");
    Ok(())
}

fn write_vault_secret_control_plane(base_control_plane: &Path, output: &Path) -> Result<()> {
    let mut control_plane: Value = serde_json::from_str(&fs::read_to_string(base_control_plane)?)?;
    control_plane
        .get_mut("secrets")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow!("control plane has no secrets array"))?
        .push(serde_json::json!({
            "id": "vault_smoke_secret",
            "source": "vault",
            "purpose": "token_exchange_credential",
            "locator": "kv2://secret/veoveo/smoke#value",
            "owner": {
                "kind": "gateway"
            }
        }));
    fs::write(output, serde_json::to_vec_pretty(&control_plane)?)?;
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("write to string");
    }
    output
}
