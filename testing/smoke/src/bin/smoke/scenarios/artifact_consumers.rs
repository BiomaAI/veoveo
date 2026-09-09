//! Installed HTTP ingestion and Python consumption, separate from visual acceptance.
use super::*;
use anyhow::ensure;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, num::NonZeroU32};
use tokio::io::AsyncWriteExt;
use veoveo_mcp_contract::*;

#[path = "artifact_consumers/python.rs"]
mod python;

#[derive(Deserialize, Serialize)]
struct BrowserReceipt {
    artifact_id: ArtifactId,
    artifact_uri: String,
    upload_id: ArtifactUploadId,
    byte_len: u64,
    sha256: UploadSha256,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserEvidence {
    large_receipt: BrowserReceipt,
    csv_receipt: BrowserReceipt,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Evidence {
    schema: &'static str,
    source_revision: String,
    public_base_url: String,
    large_receipt: BrowserReceipt,
    python: python::Observation,
    parquet_receipt: ArtifactUploadReceipt,
    unknown_length_receipt: ArtifactUploadReceipt,
    checks: Vec<&'static str>,
}

pub(crate) async fn artifact_upload_consumers(
    conformance: &Path,
    context: &str,
    public_base: &str,
    browser_evidence: &Path,
    evidence_output: &Path,
) -> Result<()> {
    ensure!(
        url::Url::parse(public_base)?.scheme() == "https",
        "public HTTPS is required"
    );
    ensure!(
        !evidence_output.exists(),
        "consumer evidence already exists"
    );
    let browser: BrowserEvidence = serde_json::from_slice(&fs::read(browser_evidence)?)?;
    ensure!(
        browser.large_receipt.byte_len > u64::from(u32::MAX),
        "large fixture must exceed 4 GiB"
    );
    let base = public_base.trim_end_matches('/');
    let token = gateway_token_for_context(
        conformance,
        base,
        "operator-service",
        "operator",
        &["operator:use", "artifact:upload"],
        "operations",
    )
    .await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(Policy::none())
        .build()?;
    let upload_base = format!("{base}/artifacts/operator/uploads");

    let other_actor = client
        .get(format!("{upload_base}/{}", browser.large_receipt.upload_id))
        .bearer_auth(&token)
        .send()
        .await?;
    ensure!(
        other_actor.status() == StatusCode::FORBIDDEN,
        "a different actor could read the browser upload session"
    );
    let independent = gateway_token_for_context(
        conformance,
        base,
        "operator-service",
        "operator",
        &["operator:use"],
        "independent-review",
    )
    .await?;
    let denied = client
        .get(format!(
            "{base}/artifacts/operator/{}/download",
            browser.large_receipt.artifact_id
        ))
        .bearer_auth(independent)
        .send()
        .await?;
    ensure!(
        denied.status() == StatusCode::FORBIDDEN,
        "independent context received {}",
        denied.status()
    );

    // A real Parquet producer supplies bytes; protocol assertions remain here in Rust.
    let temporary = tempfile::tempdir()?;
    let parquet = temporary.path().join("upload-consumer.parquet");
    let db = ::duckdb::Connection::open_in_memory()?;
    db.execute_batch(&format!("COPY (SELECT * FROM (VALUES ('alpha', 1), ('beta', 2)) AS t(name, value)) TO '{}' (FORMAT PARQUET)", parquet.display().to_string().replace('\'', "''")))?;
    let parquet_bytes = fs::read(&parquet)?;
    let parquet_receipt = upload_small(
        &client,
        &token,
        &upload_base,
        "upload-consumer.parquet",
        "application/vnd.apache.parquet",
        parquet_bytes,
        true,
    )
    .await?;
    let unknown_length_receipt = upload_small(
        &client,
        &token,
        &upload_base,
        "upload-consumer.csv",
        "text/csv",
        b"name,value\nalpha,1\nbeta,2\n".to_vec(),
        false,
    )
    .await?;
    for uri in [
        &browser.csv_receipt.artifact_uri,
        &parquet_receipt.artifact_uri,
        &unknown_length_receipt.artifact_uri,
    ] {
        let arguments = serde_json::to_string(&serde_json::json!({"dataset_uri":uri,"rows":2}))?;
        let result = bioma::run_public_conformance(
            conformance,
            base,
            &token,
            &[
                "call",
                "--tool-name",
                "datasheet__preview_dataset",
                "--arguments",
                &arguments,
            ],
            Duration::from_secs(60),
        )
        .await?;
        let result = bioma::structured_output(&result)?;
        ensure!(
            result.get("row_count").and_then(Value::as_u64) == Some(2),
            "Datasheet did not consume both rows: {result}"
        );
        let rows = result
            .get("rows")
            .and_then(Value::as_array)
            .context("missing preview rows")?;
        ensure!(
            rows.len() == 2
                && rows[0].get("name").and_then(Value::as_str) == Some("alpha")
                && rows[1].get("value").and_then(Value::as_i64) == Some(2),
            "Datasheet changed uploaded values"
        );
    }
    println!(
        "Public known/unknown-length upload, immutable part retry, context denial, and Python CSV/Parquet MCP consumption passed"
    );

    let python = python::consume(context, &browser.large_receipt, &browser.csv_receipt).await?;
    ensure!(
        python.bytes == browser.large_receipt.byte_len
            && python.sha256 == browser.large_receipt.sha256.as_str(),
        "Python large-file byte or hash mismatch"
    );
    ensure!(
        python.peak_rss_kib < 256 * 1024 && python.max_chunk_bytes <= 1024 * 1024,
        "Python large download exceeded its bounded working set: {python:?}"
    );
    ensure!(
        python.early_bytes > 0
            && python.early_bytes <= 1024 * 1024
            && python.small_limit_error == "ArtifactTooLarge",
        "Python early exit or byte ceiling failed"
    );
    ensure!(
        python.materialized_inside
            && !python.materialized_after
            && python.csv_sha256 == browser.csv_receipt.sha256.as_str(),
        "Python materialization or cleanup failed"
    );
    ensure!(
        matches!(
            python.foreign_tenant_error.as_str(),
            "ArtifactDenied" | "ArtifactNotFound"
        ),
        "Python foreign tenant could consume the artifact"
    );
    let revision = run_checked(Path::new("git"), ["rev-parse".into(), "HEAD".into()], [])?
        .trim()
        .to_owned();
    let evidence = Evidence {
        schema: "veoveo.io/artifact-upload-consumer-acceptance/v1",
        source_revision: revision,
        public_base_url: base.into(),
        large_receipt: browser.large_receipt,
        python,
        parquet_receipt,
        unknown_length_receipt,
        checks: vec![
            "Public machine OAuth and actor/Work Context denial",
            "Known and unknown length uploads with immutable retry and completion replay",
            "Real CSV and Parquet consumed through public Datasheet MCP",
            "Installed Python SDK streamed the entire large object with exact SHA-256",
            "Bounded Python memory, early exit, byte ceiling, temporary-file cleanup, and tenant denial",
        ],
    };
    if let Some(parent) = evidence_output.parent() {
        fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(evidence_output)?;
    output.write_all(&serde_json::to_vec_pretty(&evidence)?)?;
    println!(
        "Installed Artifact consumers passed. Evidence: {}",
        evidence_output.display()
    );
    Ok(())
}

async fn upload_small(
    client: &reqwest::Client,
    token: &str,
    base: &str,
    filename: &str,
    mime: &str,
    bytes: Vec<u8>,
    known: bool,
) -> Result<ArtifactUploadReceipt> {
    let sha = UploadSha256::parse(hex::encode(Sha256::digest(&bytes)))?;
    let descriptor = CreateArtifactUpload {
        filename: filename.into(),
        mime_type: mime.into(),
        byte_len: known.then_some(bytes.len() as u64),
        sha256: Some(sha.clone()),
    };
    let response = client
        .post(base)
        .bearer_auth(token)
        .header("Idempotency-Key", uuid::Uuid::now_v7().to_string())
        .json(&descriptor)
        .send()
        .await?;
    ensure!(
        response.status() == StatusCode::CREATED,
        "public admission returned {}",
        response.status()
    );
    let session: ArtifactUploadSession = response.json().await?;
    let url = format!("{base}/{}", session.upload_id);
    let result = async {
        ensure!(
            bytes.len() as u64 <= session.layout.part_bytes.get(),
            "small fixture requires multiple parts"
        );
        let part_url = format!("{url}/parts/1");
        let send_part = || {
            client
                .put(&part_url)
                .bearer_auth(token)
                .header(UPLOAD_PART_BYTE_LEN_HEADER, bytes.len())
                .header(UPLOAD_PART_SHA256_HEADER, sha.as_str())
                .body(bytes.clone())
        };
        let first: UploadPartReceipt = send_part().send().await?.error_for_status()?.json().await?;
        let repeat: UploadPartReceipt =
            send_part().send().await?.error_for_status()?.json().await?;
        ensure!(
            first == repeat && first.sha256 == sha && first.byte_len == bytes.len() as u64,
            "accepted part replay differs"
        );
        let mut changed = bytes.clone();
        changed[0] ^= 1;
        let conflict = client
            .put(&part_url)
            .bearer_auth(token)
            .header(UPLOAD_PART_BYTE_LEN_HEADER, changed.len())
            .header(
                UPLOAD_PART_SHA256_HEADER,
                hex::encode(Sha256::digest(&changed)),
            )
            .body(changed)
            .send()
            .await?;
        ensure!(
            conflict.status() == StatusCode::CONFLICT,
            "changed accepted part returned {}",
            conflict.status()
        );
        let manifest = CompleteArtifactUpload {
            byte_len: bytes.len() as u64,
            part_count: NonZeroU32::new(1).unwrap(),
            sha256: Some(sha.clone()),
        };
        let response = client
            .post(format!("{url}/complete"))
            .bearer_auth(token)
            .json(&manifest)
            .send()
            .await?
            .error_for_status()?;
        let mut status: ArtifactUploadSession = response.json().await?;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(90);
        while status.receipt.is_none() {
            ensure!(
                !status.state.is_terminal() && tokio::time::Instant::now() < deadline,
                "upload did not publish a receipt: {:?}",
                status.state
            );
            tokio::time::sleep(Duration::from_millis(500)).await;
            status = client
                .get(&url)
                .bearer_auth(token)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
        }
        let receipt = status.receipt.unwrap();
        ensure!(
            receipt.sha256 == sha && receipt.byte_len == bytes.len() as u64,
            "uploaded object changed bytes"
        );
        let replay: ArtifactUploadSession = client
            .post(format!("{url}/complete"))
            .bearer_auth(token)
            .json(&manifest)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        ensure!(
            replay.receipt.as_ref() == Some(&receipt),
            "completion replay changed occurrence"
        );
        ensure!(
            client
                .delete(&url)
                .bearer_auth(token)
                .send()
                .await?
                .status()
                == StatusCode::CONFLICT,
            "completed upload was cancelled"
        );
        Ok::<_, anyhow::Error>(receipt)
    }
    .await;
    if result.is_err() {
        let _ = client.delete(&url).bearer_auth(token).send().await;
    }
    result
}
