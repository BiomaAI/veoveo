//! Store-to-domain admission for public catalog, layer and seal metadata.
use super::{RecordingService, record_uuid};
use crate::contract::{
    LayerView, LayerViewBuilder, ManifestBlueprint, ManifestLayer, ManifestLayerBuilder,
    RecordingView, RecordingViewBuilder,
};
use anyhow::{Context as _, Result, ensure};
use veoveo_artifact_contract::{ArtifactId, ArtifactUri};
use veoveo_platform_store::{ArtifactId as PlatformArtifactId, RecordId};
use veoveo_recording_store::{
    RecordingBlueprintRecord, RecordingDatasetId, RecordingId, RecordingLayerRecord,
    RecordingLayerState, RecordingRecord,
};
use veoveo_types::Sha256Digest;

pub(super) fn artifact_uri(id: PlatformArtifactId) -> Result<ArtifactUri> {
    Ok(ArtifactId::try_from(id.as_uuid())?.plane_uri())
}

pub(super) fn artifact_reference(record: &RecordId) -> Result<ArtifactUri> {
    Ok(ArtifactId::try_from(record_uuid(record, "artifact_occurrence")?)?.plane_uri())
}

pub(super) fn layer_view(layer: &RecordingLayerRecord) -> Result<LayerView> {
    use crate::contract::RecordingLayerState as Public;
    use veoveo_recording_store::RecordingLayerKind;
    match (&layer.kind, &layer.properties_preparation) {
        (RecordingLayerKind::Properties, Some(preparation)) => {
            ensure!(
                preparation.body.recording_id.as_uuid()
                    == record_uuid(&layer.recording, "recording")?,
                "properties preparation identifies another recording"
            );
        }
        (RecordingLayerKind::Properties, None) => {
            anyhow::bail!("properties layer has no preparation snapshot");
        }
        (_, Some(_)) => {
            anyhow::bail!("source layer unexpectedly carries properties preparation");
        }
        (_, None) => {}
    }
    Ok(LayerViewBuilder {
        layer_id: crate::contract::RecordingLayerId::try_from(record_uuid(
            &layer.id,
            "recording_layer",
        )?)?,
        layer_name: layer.layer_name.clone(),
        kind: super::layer_kind(layer.kind),
        ordinal: layer.ordinal.map(u64::try_from).transpose()?,
        state: match layer.state {
            RecordingLayerState::Writing => Public::Writing,
            RecordingLayerState::Staged => Public::Staged,
            RecordingLayerState::Committed => Public::Committed,
            RecordingLayerState::Failed => Public::Failed,
        },
        byte_len: layer.byte_len.try_into()?,
        message_count: layer.message_count.try_into()?,
        sha256: layer
            .sha256
            .as_ref()
            .map(Sha256Digest::from_hex)
            .transpose()?,
        artifact_uri: layer
            .artifact
            .as_ref()
            .map(artifact_reference)
            .transpose()?,
        rrd_version: layer.rrd_version.clone(),
        schema_digest: layer
            .schema_digest
            .as_ref()
            .map(Sha256Digest::from_hex)
            .transpose()?,
        created_at: layer.created_at,
        updated_at: layer.updated_at,
    }
    .build()?)
}

pub(super) fn manifest_layer(layer: &RecordingLayerRecord) -> Result<ManifestLayer> {
    ensure!(
        layer.state == RecordingLayerState::Committed,
        "manifest layer is not committed"
    );
    let view = layer_view(layer)?;
    Ok(ManifestLayerBuilder {
        layer_id: view.layer_id,
        layer_name: view.layer_name.clone(),
        kind: view.kind,
        ordinal: view.ordinal,
        byte_len: view.byte_len.try_into()?,
        sha256: view
            .sha256
            .clone()
            .context("committed layer has no digest")?,
        artifact_uri: view
            .artifact_uri
            .clone()
            .context("committed layer has no Artifact occurrence")?,
        rrd_version: view
            .rrd_version
            .clone()
            .context("committed layer has no RRD version")?,
        schema_digest: view
            .schema_digest
            .clone()
            .context("committed layer has no schema digest")?,
    }
    .build()?)
}

pub(super) fn manifest_blueprint(blueprint: RecordingBlueprintRecord) -> Result<ManifestBlueprint> {
    Ok(ManifestBlueprint {
        blueprint_id: blueprint.blueprint_id,
        revision: u64::try_from(blueprint.revision)?.try_into()?,
        byte_len: u64::try_from(blueprint.byte_len)?.try_into()?,
        message_count: u64::try_from(blueprint.message_count)?.try_into()?,
        sha256: Sha256Digest::from_hex(blueprint.sha256)?,
        artifact_uri: artifact_reference(
            blueprint
                .artifact
                .as_ref()
                .context("recording Blueprint has not been published")?,
        )?,
    })
}

impl RecordingService {
    pub(super) async fn view(
        &self,
        tenant_id: veoveo_platform_store::TenantId,
        recording: RecordingRecord,
    ) -> Result<RecordingView> {
        let recording_id = RecordingId::from_uuid(record_uuid(&recording.id, "recording")?);
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&recording.dataset, "recording_dataset")?);
        let dataset = self
            .recordings
            .recording_dataset(tenant_id, dataset_id)
            .await?
            .context("recording dataset is missing")?;
        // These aggregates describe the full current catalog. Sealed manifest
        // receivers separately admit the immutable intent-selected membership.
        let catalog_counts = self
            .recordings
            .recording_layer_counts(tenant_id, recording_id)
            .await?;
        Ok(RecordingViewBuilder {
            recording_id: crate::contract::RecordingId::try_from(recording_id.as_uuid())?,
            dataset_id: crate::contract::RecordingDatasetId::try_from(dataset_id.as_uuid())?,
            dataset_key: dataset.dataset_key,
            application_id: recording.application_id,
            recording_key: recording.recording_key,
            state: super::recording_state(recording.state),
            classification: recording.classification,
            labels: recording.labels,
            started_at: recording.started_at,
            last_data_at: recording.last_data_at,
            ended_at: recording.ended_at,
            sealed_at: recording.sealed_at,
            manifest_artifact_uri: recording
                .manifest_artifact
                .as_ref()
                .map(artifact_reference)
                .transpose()?,
            layer_count: catalog_counts.total,
            committed_layer_count: catalog_counts.committed,
        }
        .build()?)
    }
}
