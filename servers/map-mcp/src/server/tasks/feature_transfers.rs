use std::path::Path;

use anyhow::{Context, Result, bail};
use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;
use veoveo_mcp_contract::GatewayInternalIdentity;
use veoveo_task_runtime::TaskId;

use crate::{
    authoring::GeneratedLayerProduct,
    contract::{
        FeatureExportFormat, FeatureImportSource, ImportFeatureLayerRequest,
        InspectGeoPackageOutput, InspectGeoPackageRequest, LayerProductFormat,
    },
    feature_packages::GeoPackageDecode,
    state::MapApplication,
};

use super::{AuthenticatedCaller, cleanup_task_directory, task_directory, tool_result_with_links};

const STAGED_GEOPACKAGE_FILENAME: &str = "source.gpkg";
const STAGED_GENERIC_FEATURE_FILENAME: &str = "source";

pub(super) fn staged_import_filename(source: &FeatureImportSource) -> &'static str {
    match source {
        FeatureImportSource::GeoPackage { .. } => STAGED_GEOPACKAGE_FILENAME,
        FeatureImportSource::GeoJsonFeatureCollection { .. }
        | FeatureImportSource::GeoJsonTextSequence { .. } => STAGED_GENERIC_FEATURE_FILENAME,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct DurableGeoPackageInspectionRequest {
    pub(super) input: InspectGeoPackageRequest,
    pub(super) identity: GatewayInternalIdentity,
    pub(super) source_digest_sha256: String,
}

pub(super) async fn prepare_inspection(
    state: &MapApplication,
    caller: &AuthenticatedCaller,
    task_id: &TaskId,
    input: InspectGeoPackageRequest,
) -> Result<DurableGeoPackageInspectionRequest> {
    let digest = stage_authorized_artifact(
        state,
        caller,
        task_id,
        &input.source_artifact_id,
        "GeoPackage inspection",
    )
    .await?;
    Ok(DurableGeoPackageInspectionRequest {
        input,
        identity: caller.identity.clone(),
        source_digest_sha256: digest,
    })
}

pub(super) async fn run_inspection(
    state: &MapApplication,
    task_id: &str,
    request: DurableGeoPackageInspectionRequest,
    cancellation: CancellationToken,
) -> Result<CallToolResult> {
    let source_path = task_directory(state, task_id)?.join(STAGED_GEOPACKAGE_FILENAME);
    verify_staged_source(state, &source_path, &request.source_digest_sha256).await?;
    let manifest = state
        .feature_packages
        .inspect(&source_path, cancellation)
        .await?;
    let table_count = manifest.feature_tables.len();
    let output = InspectGeoPackageOutput { manifest };
    tool_result_with_links(
        format!("inspected GeoPackage with {table_count} feature tables"),
        &output,
        [(
            request.input.source_artifact_id.plane_uri(),
            "Inspected GeoPackage artifact",
        )],
    )
}

pub(super) async fn transcode_import(
    state: &MapApplication,
    task_id: &str,
    request: &mut ImportFeatureLayerRequest,
    original: Vec<u8>,
    cancellation: CancellationToken,
) -> Result<Vec<u8>> {
    let FeatureImportSource::GeoPackage {
        table,
        identity_column,
        semantic_type_column,
        default_semantic_type,
        title_column,
        valid_from_column,
        valid_until_column,
    } = &request.source
    else {
        return Ok(original);
    };
    let directory = task_directory(state, task_id)?;
    let output_dir = directory.join("decoded");
    tokio::fs::create_dir(&output_dir).await?;
    let decoded = state
        .feature_packages
        .decode(
            &directory.join(STAGED_GEOPACKAGE_FILENAME),
            &output_dir,
            GeoPackageDecode {
                table,
                identity_column: identity_column.as_ref(),
                semantic_type_column: semantic_type_column.as_ref(),
                default_semantic_type,
                title_column: title_column.as_ref(),
                valid_from_column: valid_from_column.as_ref(),
                valid_until_column: valid_until_column.as_ref(),
            },
            cancellation,
        )
        .await?;
    let bytes = tokio::fs::read(&decoded.path).await?;
    if bytes.len() as u64 != decoded.byte_count
        || hex::encode(Sha256::digest(&bytes)) != decoded.digest_sha256
    {
        bail!("decoded GeoPackage output failed its byte-count or digest check");
    }
    request.source = FeatureImportSource::GeoJsonTextSequence {
        default_semantic_type: default_semantic_type.clone(),
    };
    Ok(bytes)
}

pub(super) async fn generate_export(
    state: &MapApplication,
    identity: &GatewayInternalIdentity,
    scope: &crate::catalog::MapScope,
    request: &crate::contract::ExportFeatureLayerRequest,
    directory: &Path,
    cancellation: CancellationToken,
) -> Result<GeneratedLayerProduct> {
    let FeatureExportFormat::GeoPackage { table } = &request.format else {
        return state
            .authoring
            .generate_export(
                identity,
                scope,
                request,
                directory,
                state.max_artifact_bytes,
            )
            .await;
    };
    let sequence = state
        .authoring
        .generate_export(
            identity,
            scope,
            &crate::contract::ExportFeatureLayerRequest {
                layer_id: request.layer_id.clone(),
                publication_id: request.publication_id.clone(),
                format: FeatureExportFormat::GeoJsonSeq,
            },
            directory,
            state.max_artifact_bytes,
        )
        .await?;
    let sequence_path = directory.join("publication.geojsons");
    let mut source = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&sequence_path)
        .await?;
    source.write_all(&sequence.bytes).await?;
    source.sync_all().await?;
    let output_dir = directory.join("geopackage");
    tokio::fs::create_dir(&output_dir).await?;
    let encoded = state
        .feature_packages
        .encode(&sequence_path, &output_dir, table, cancellation)
        .await?;
    let bytes = tokio::fs::read(&encoded.path).await?;
    if bytes.len() as u64 != encoded.byte_count
        || hex::encode(Sha256::digest(&bytes)) != encoded.digest_sha256
        || encoded.feature_count != sequence.feature_count
    {
        bail!("encoded GeoPackage output failed its count, size, or digest check");
    }
    Ok(GeneratedLayerProduct {
        bytes,
        format: LayerProductFormat::GeoPackage,
        mime_type: "application/geopackage+sqlite3",
        filename: format!("{}.gpkg", request.publication_id),
        feature_count: encoded.feature_count,
        digest_sha256: encoded.digest_sha256,
        tile_count: None,
    })
}

async fn stage_authorized_artifact(
    state: &MapApplication,
    caller: &AuthenticatedCaller,
    task_id: &TaskId,
    artifact_id: &veoveo_mcp_contract::ArtifactId,
    operation: &str,
) -> Result<String> {
    let artifact = state
        .artifacts
        .get(&caller.caller, artifact_id)
        .await?
        .context("unknown or unauthorized source artifact")?;
    if artifact.metadata.byte_len > state.max_artifact_bytes
        || artifact.bytes.len() as u64 > state.max_artifact_bytes
        || artifact.metadata.byte_len != artifact.bytes.len() as u64
    {
        bail!("source artifact exceeds the configured byte limit or has inconsistent metadata");
    }
    let directory = task_directory(state, &task_id.to_string())?;
    tokio::fs::create_dir(&directory).await.with_context(|| {
        format!(
            "creating {operation} task directory {}",
            directory.display()
        )
    })?;
    let digest = hex::encode(Sha256::digest(&artifact.bytes));
    let staging = async {
        let mut file = tokio::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(directory.join(STAGED_GEOPACKAGE_FILENAME))
            .await?;
        file.write_all(&artifact.bytes).await?;
        file.sync_all().await
    }
    .await;
    if let Err(error) = staging {
        cleanup_task_directory(state, &task_id.to_string()).await;
        return Err(error).with_context(|| format!("staging authorized artifact for {operation}"));
    }
    Ok(digest)
}

async fn verify_staged_source(
    state: &MapApplication,
    source_path: &Path,
    expected_digest: &str,
) -> Result<()> {
    let bytes = tokio::fs::read(source_path).await?;
    if bytes.len() as u64 > state.max_artifact_bytes
        || hex::encode(Sha256::digest(&bytes)) != expected_digest
    {
        bail!("staged GeoPackage failed its bound size or digest check");
    }
    Ok(())
}
