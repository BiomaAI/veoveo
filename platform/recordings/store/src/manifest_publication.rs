//! Immutable Recording-owned manifest intent, persisted before Artifact transport.
use crate::{
    RecordingBlueprintRecord, RecordingDatasetRecord, RecordingId, RecordingLayerRecord,
    RecordingRecord, RecordingRepository, RecordingState, RecordingStoreError,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use surrealdb::types::{Error, Kind, RecordId, SurrealValue, Value};
use veoveo_artifact_contract::StreamArtifactRequest;
use veoveo_platform_store::{
    InvocationAuthorityRecord, PlatformIdentity, native_json_from_value_strict,
    native_json_into_value,
};
use veoveo_recording_contract::{
    MAX_RECORDING_MANIFEST_BYTES, RecordingArtifactMetadata, RecordingArtifactProvenance,
    RecordingManifest, RecordingPublisherContext,
};
use veoveo_types::Sha256Digest;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManifestPublicationBody(pub RecordingManifest);
impl SurrealValue for ManifestPublicationBody {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        let mut value = native_json_into_value(
            serde_json::to_value(self.0).expect("admitted manifest serialization"),
        );
        // Surreal 3.3 requires optional-object parents for declared children.
        // This native absence represents the owner's public explicit null only.
        if let Value::Object(fields) = &mut value
            && fields.get("blueprint") == Some(&Value::Null)
        {
            fields.insert("blueprint", Value::None);
        }
        value
    }
    fn from_value(mut value: Value) -> Result<Self, Error> {
        if let Value::Object(fields) = &mut value
            && fields
                .get("blueprint")
                .is_none_or(|value| *value == Value::None)
        {
            fields.insert("blueprint", Value::Null);
        }
        serde_json::from_value(native_json_from_value_strict(value)?)
            .map(Self)
            .map_err(|_| Error::internal("invalid current Recording manifest intent body".into()))
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManifestPublicationDescriptor(pub StreamArtifactRequest);
impl SurrealValue for ManifestPublicationDescriptor {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        native_json_into_value(
            serde_json::to_value(self.0).expect("admitted publication descriptor serialization"),
        )
    }
    fn from_value(mut value: Value) -> Result<Self, Error> {
        let invalid =
            || Error::internal("invalid current Recording manifest intent descriptor".into());
        if let Value::Object(fields) = &mut value
            && let Some(Value::Object(artifact)) = fields.get_mut("artifact")
        {
            for key in ["classification", "dataLabels", "retentionExpiresAt"] {
                if artifact.get(key) == Some(&Value::None) {
                    artifact.remove(key);
                }
            }
        }
        let mut descriptor: StreamArtifactRequest =
            serde_json::from_value(native_json_from_value_strict(value)?).map_err(|_| invalid())?;
        // Admit the closed owner metadata and restore its sorted object profile
        // independently of serde_json's preserve_order feature.
        let metadata: RecordingArtifactMetadata =
            serde_json::from_value(descriptor.artifact.metadata).map_err(|_| invalid())?;
        descriptor.artifact.metadata = serde_json::to_value(metadata).map_err(|_| invalid())?;
        descriptor.artifact.metadata.sort_all_objects();
        Ok(Self(descriptor))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ManifestPublicationPublisher(pub RecordingPublisherContext);
impl SurrealValue for ManifestPublicationPublisher {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        native_json_into_value(serde_json::to_value(self.0).expect("validated publisher context"))
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(native_json_from_value_strict(value)?)
            .map(Self)
            .map_err(|_| Error::internal("invalid Recording publication context".into()))
    }
}
/// Native envelope columns keep their database spelling; nested bodies are owner JSON.
#[derive(Clone, Debug, Serialize, Deserialize, SurrealValue)]
pub struct RecordingManifestPublicationRecord {
    pub id: RecordId,
    pub version: u8,
    pub tenant: RecordId,
    pub actor: RecordId,
    pub source_owner: RecordId,
    pub source_work_context: RecordId,
    pub authority: InvocationAuthorityRecord,
    pub publisher: ManifestPublicationPublisher,
    pub recording: RecordId,
    pub dataset: RecordId,
    pub dataset_revision: i64,
    pub recording_revision: i64,
    pub sealed_at: DateTime<Utc>,
    pub descriptor_sha256: String,
    pub body: ManifestPublicationBody,
    pub descriptor: ManifestPublicationDescriptor,
}
pub(crate) fn publication_record_id(recording: RecordingId) -> RecordId {
    RecordId::new(
        "recording_manifest_publication",
        surrealdb::types::Uuid::from(recording.as_uuid()),
    )
}

fn invalid() -> RecordingStoreError {
    disagreement("current immutable intent fields disagree")
}
fn disagreement(reason: &'static str) -> RecordingStoreError {
    RecordingStoreError::InvalidRecordingField {
        field: "manifest_publication",
        reason,
    }
}
impl RecordingManifestPublicationRecord {
    pub fn bytes(&self) -> Result<Vec<u8>, RecordingStoreError> {
        let body = &self.body.0;
        let descriptor = &self.descriptor.0;
        let bytes = serde_json::to_vec_pretty(body).map_err(|_| invalid())?;
        let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        let descriptor_digest = hex::encode(Sha256::digest(
            serde_json::to_vec(descriptor).map_err(|_| invalid())?,
        ));
        let metadata: RecordingArtifactMetadata =
            serde_json::from_value(descriptor.artifact.metadata.clone()).map_err(|_| invalid())?;
        let RecordingArtifactProvenance::RecordingManifest {
            recording_id,
            dataset_id,
            catalog_revision,
            dataset_revision,
            recording_revision,
            sealed_at,
            sha256,
        } = metadata.provenance
        else {
            return Err(invalid());
        };
        if self.version != 1
            || self.dataset_revision < 0
            || self.recording_revision < 0
            || self.id != publication_record_id(RecordingId::from_uuid(recording_id.as_uuid()))
            || self.recording != RecordingId::from_uuid(recording_id.as_uuid()).record_id()
            || self.dataset
                != crate::RecordingDatasetId::from_uuid(dataset_id.as_uuid()).record_id()
            || descriptor.artifact_id.as_uuid() != recording_id.as_uuid()
            || recording_id != body.recording_segment_id
            || dataset_id != body.dataset_id
        {
            return Err(disagreement("current intent identity fields disagree"));
        }
        if dataset_revision != self.dataset_revision as u64
            || recording_revision != self.recording_revision as u64
            || sealed_at != self.sealed_at
            || sealed_at != body.sealed_at
            || catalog_revision != body.catalog_revision
        {
            return Err(disagreement(
                "current intent original snapshot fields disagree",
            ));
        }
        if sha256 != digest
            || descriptor.expected_sha256.as_str() != digest.hex()
            || bytes.is_empty()
            || bytes.len() as u64 > MAX_RECORDING_MANIFEST_BYTES
            || descriptor.expected_byte_len != bytes.len() as u64
        {
            return Err(disagreement(
                "current intent body digest or byte length disagrees",
            ));
        }
        if descriptor_digest != self.descriptor_sha256 {
            return Err(disagreement("current intent descriptor digest disagrees"));
        }
        if descriptor.artifact.mime_type.as_deref()
            != Some("application/vnd.veoveo.recording-manifest+json")
            || descriptor
                .artifact
                .filename
                .as_ref()
                .is_none_or(|name| !name.ends_with(".recording-v10.json"))
        {
            return Err(disagreement(
                "current intent descriptor MIME or filename disagrees",
            ));
        }
        Ok(bytes)
    }
}
#[derive(Clone, Debug, SurrealValue)]
pub(crate) struct LayerRevision {
    pub(crate) layer: RecordId,
    pub(crate) revision: i64,
}
/// Selected source facts checked together when reserving a manifest intent.
pub struct ManifestPublicationSource<'a> {
    pub recording: &'a RecordingRecord,
    pub dataset: &'a RecordingDatasetRecord,
    pub layers: &'a [RecordingLayerRecord],
    pub blueprint: Option<&'a RecordingBlueprintRecord>,
}

impl RecordingRepository {
    /// Select the bounded immutable membership before any catalog pagination.
    pub async fn manifest_publication_layers(
        &self,
        identity: &PlatformIdentity,
        recording: RecordingId,
        intent: &RecordingManifestPublicationRecord,
    ) -> Result<Vec<RecordingLayerRecord>, RecordingStoreError> {
        intent.bytes()?;
        if intent.tenant != identity.tenant_id.record_id()
            || intent.recording != recording.record_id()
        {
            return Err(invalid());
        }
        let ids: Vec<_> = intent
            .body
            .0
            .layers
            .iter()
            .map(|row| crate::RecordingLayerId::from_uuid(row.layer_id.as_uuid()).record_id())
            .collect();
        let mut response = self
            .client()
            .query(include_str!(
                "queries/recordings/manifest_publication_layers.surql"
            ))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("recording", recording.record_id()))
            .bind(("layers", ids.clone()))
            .await?
            .check()?;
        let rows: Vec<RecordingLayerRecord> = response.take(0)?;
        if rows.len() != ids.len() {
            return Err(RecordingStoreError::InvalidRecordingField {
                field: "manifest_publication_layers",
                reason: "selected layers are missing or catalog contains an unselected source layer",
            });
        }
        ids.iter()
            .map(|id| {
                rows.iter()
                    .find(|row| &row.id == id)
                    .cloned()
                    .ok_or_else(invalid)
            })
            .collect()
    }
    /// The service has already admitted current scope and Recording visibility.
    pub async fn manifest_publication(
        &self,
        identity: &PlatformIdentity,
        recording: RecordingId,
    ) -> Result<Option<RecordingManifestPublicationRecord>, RecordingStoreError> {
        let mut result = self
            .client()
            .query(include_str!(
                "queries/recordings/read_manifest_publication.surql"
            ))
            .bind(("publication", publication_record_id(recording)))
            .bind(("tenant", identity.tenant_id.record_id()))
            .bind(("actor", identity.principal_id.record_id()))
            .bind(("recording", recording.record_id()))
            .await?
            .check()?;
        let row: Option<RecordingManifestPublicationRecord> = result.take(0)?;
        if let Some(row) = &row {
            row.bytes()?;
        }
        Ok(row)
    }
    pub async fn reserve_manifest_publication(
        &self,
        identity: &PlatformIdentity,
        source: ManifestPublicationSource<'_>,
        body: RecordingManifest,
        descriptor: StreamArtifactRequest,
        authority: InvocationAuthorityRecord,
        publisher: RecordingPublisherContext,
    ) -> Result<RecordingManifestPublicationRecord, RecordingStoreError> {
        let ManifestPublicationSource {
            recording,
            dataset,
            layers,
            blueprint,
        } = source;
        let metadata: RecordingArtifactMetadata =
            serde_json::from_value(descriptor.artifact.metadata.clone()).map_err(|_| invalid())?;
        let mut canonical_metadata = serde_json::to_value(metadata).map_err(|_| invalid())?;
        canonical_metadata.sort_all_objects();
        if serde_json::to_vec(&descriptor.artifact.metadata).map_err(|_| invalid())?
            != serde_json::to_vec(&canonical_metadata).map_err(|_| invalid())?
        {
            return Err(disagreement(
                "publication metadata must use owner serialization order",
            ));
        }
        let id = RecordingId::from_uuid(body.recording_segment_id.as_uuid());
        let publication = RecordingManifestPublicationRecord {
            id: publication_record_id(id),
            version: 1,
            tenant: identity.tenant_id.record_id(),
            actor: identity.principal_id.record_id(),
            source_owner: recording.owner.clone(),
            source_work_context: recording.work_context.clone(),
            authority,
            publisher: ManifestPublicationPublisher(publisher),
            recording: recording.id.clone(),
            dataset: dataset.id.clone(),
            dataset_revision: dataset.revision,
            recording_revision: recording.revision,
            sealed_at: body.sealed_at,
            descriptor_sha256: hex::encode(Sha256::digest(
                serde_json::to_vec(&descriptor).map_err(|_| invalid())?,
            )),
            body: ManifestPublicationBody(body),
            descriptor: ManifestPublicationDescriptor(descriptor),
        };
        publication.bytes()?;
        if publication.body.0.layers.len() != layers.len() {
            return Err(invalid());
        }
        for (native, body) in layers.iter().zip(&publication.body.0.layers) {
            let kind = match native.kind {
                crate::RecordingLayerKind::Capture => {
                    veoveo_recording_contract::RecordingLayerKind::Capture
                }
                crate::RecordingLayerKind::Properties => {
                    veoveo_recording_contract::RecordingLayerKind::Properties
                }
                crate::RecordingLayerKind::Derived => {
                    veoveo_recording_contract::RecordingLayerKind::Derived
                }
            };
            if native.tenant != publication.tenant
                || native.recording != publication.recording
                || native.state != crate::RecordingLayerState::Committed
                || native.id
                    != crate::RecordingLayerId::from_uuid(body.layer_id.as_uuid()).record_id()
                || native.layer_name != body.layer_name
                || kind != body.kind
                || native.ordinal.map(u64::try_from).transpose().ok() != Some(body.ordinal)
                || u64::try_from(native.byte_len).ok() != Some(body.byte_len.get())
                || native.sha256.as_deref() != Some(body.sha256.hex())
                || native.artifact
                    != Some(
                        veoveo_platform_store::ArtifactId::from_uuid(
                            body.artifact_uri.artifact_id().as_uuid(),
                        )
                        .record_id(),
                    )
                || native.rrd_version.as_deref() != Some(body.rrd_version.as_str())
                || native.schema_digest.as_deref() != Some(body.schema_digest.hex())
            {
                return Err(invalid());
            }
        }
        match (blueprint, publication.body.0.blueprint.as_ref()) {
            (None, None) => {}
            (Some(selected), Some(body))
                if selected.tenant == publication.tenant
                    && selected.recording == publication.recording
                    && selected.blueprint_id == body.blueprint_id
                    && u64::try_from(selected.revision).ok() == Some(body.revision.get())
                    && u64::try_from(selected.byte_len).ok() == Some(body.byte_len.get())
                    && u64::try_from(selected.message_count).ok()
                        == Some(body.message_count.get())
                    && selected.sha256 == body.sha256.hex()
                    && selected.artifact
                        == Some(
                            veoveo_platform_store::ArtifactId::from_uuid(
                                body.artifact_uri.artifact_id().as_uuid(),
                            )
                            .record_id(),
                        ) => {}
            _ => return Err(invalid()),
        }
        if recording.state != RecordingState::Sealing
            || recording.tenant != publication.tenant
            || dataset.tenant != publication.tenant
            || recording.dataset != dataset.id
        {
            return Err(invalid());
        }
        if let Some(existing) = self.manifest_publication(identity, id).await? {
            if existing.authority != publication.authority
                || existing.publisher.0 != publication.publisher.0
                || existing.descriptor.0 != publication.descriptor.0
                || existing.bytes()? != publication.bytes()?
                || existing.dataset_revision != publication.dataset_revision
                || existing.recording_revision != publication.recording_revision
            {
                return Err(invalid());
            }
            return Ok(existing);
        }
        self.client()
            .query(include_str!(
                "queries/recordings/reserve_manifest_publication.surql"
            ))
            .bind(("publication", publication.id.clone()))
            .bind(("content", publication.clone()))
            .bind(("recording", recording.id.clone()))
            .bind(("dataset", dataset.id.clone()))
            .bind(("tenant", publication.tenant.clone()))
            .bind(("actor", publication.actor.clone()))
            .bind(("blueprint", blueprint.cloned()))
            .bind(("revision", recording.revision))
            .bind(("dataset_revision", dataset.revision))
            .bind((
                "layers",
                layers
                    .iter()
                    .map(|layer| LayerRevision {
                        layer: layer.id.clone(),
                        revision: layer.revision,
                    })
                    .collect::<Vec<_>>(),
            ))
            .await
            .and_then(|mut response| {
                match veoveo_platform_store::primary_transaction_error(response.take_errors()) {
                    Some(error) => Err(error),
                    None => Ok(response),
                }
            })?;
        self.manifest_publication(identity, id)
            .await?
            .ok_or(RecordingStoreError::MissingRecord {
                operation: "manifest publication intent readback",
            })
    }
}
