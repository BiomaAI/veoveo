//! Current retained manifest admission under a fresh, call-scoped Artifact reader.
use super::*;
use crate::contract::{
    MAX_RECORDING_MANIFEST_BYTES, ManifestBlueprint, ManifestLayer, RecordingArtifactMetadata,
    RecordingArtifactProvenance, RecordingManifestSchema,
};

pub(super) fn manifest_bytes(manifest: &RecordingManifest) -> Result<Vec<u8>> {
    let bytes = serde_json::to_vec_pretty(manifest)?;
    ensure!(
        bytes.len() as u64 <= MAX_RECORDING_MANIFEST_BYTES,
        "Recording manifest exceeds its byte limit"
    );
    Ok(bytes)
}

fn manifest_publication_descriptor(
    manifest: &RecordingManifest,
    recording_key: &str,
    classification: Option<DataLabelId>,
    data_labels: BTreeSet<DataLabelId>,
    dataset_revision: u64,
    recording_revision: u64,
) -> Result<veoveo_artifact_contract::StreamArtifactRequest> {
    let bytes = manifest_bytes(manifest)?;
    let sha256 = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
    let mut metadata = serde_json::to_value(RecordingArtifactMetadata {
        provenance: RecordingArtifactProvenance::RecordingManifest {
            recording_id: manifest.recording_segment_id,
            dataset_id: manifest.dataset_id,
            catalog_revision: manifest.catalog_revision.clone(),
            dataset_revision,
            recording_revision,
            sealed_at: manifest.sealed_at,
            sha256: sha256.clone(),
        },
    })?;
    metadata.sort_all_objects();
    Ok(veoveo_artifact_contract::StreamArtifactRequest {
        artifact_id: veoveo_artifact_contract::ArtifactId::try_from(
            manifest.recording_segment_id.as_uuid(),
        )?,
        artifact: PutArtifactRequest {
            mime_type: Some(MANIFEST_MIME.to_owned()),
            filename: Some(format!("{recording_key}.recording-v10.json")),
            classification,
            data_labels,
            retention_expires_at: None,
            metadata,
        },
        expected_byte_len: bytes.len() as u64,
        expected_sha256: veoveo_artifact_contract::UploadSha256::parse(sha256.hex())?,
    })
}

fn admit_manifest_outcome(
    intent: &veoveo_recording_store::RecordingManifestPublicationRecord,
    aggregate: &veoveo_platform_store::ArtifactAggregate,
    metadata: &veoveo_artifact_contract::ArtifactMetadata,
) -> Result<()> {
    use veoveo_platform_store::ArtifactGrantSubjectKind;
    use veoveo_types::{AccessSubject, InvocationProvenance};
    let request = &intent.descriptor.0;
    let occurrence = &aggregate.occurrence;
    let compliance = &metadata.compliance;
    let observed = PutArtifactRequest {
        classification: compliance.classification.clone(),
        data_labels: compliance.data_labels.clone(),
        ..PutArtifactRequest::default()
    };
    let effective = observed.effective_labels();
    ensure!(
        request
            .artifact
            .classification
            .as_ref()
            .is_none_or(|value| compliance.classification.as_ref() == Some(value))
            && request.artifact.effective_labels().is_subset(&effective)
            && metadata.filename == request.artifact.filename
            && metadata.mime_type == request.artifact.mime_type
            && compliance.retention_expires_at == request.artifact.retention_expires_at,
        "Recording manifest outcome differs from its explicit publication descriptor"
    );
    let native_classification =
        (!occurrence.classification.is_empty()).then_some(occurrence.classification.as_str());
    let provenance = compliance
        .provenance
        .as_ref()
        .context("Recording manifest outcome has no Artifact publisher provenance")?;
    let (mode, initiator, delegation) = match &provenance.invocation {
        InvocationProvenance::Direct { initiator } => (
            veoveo_platform_store::InvocationMode::Direct,
            Some(initiator.as_str()),
            None,
        ),
        InvocationProvenance::Delegated {
            initiator,
            delegation_id,
        } => (
            veoveo_platform_store::InvocationMode::Delegated,
            Some(initiator.as_str()),
            Some(delegation_id.as_str()),
        ),
        InvocationProvenance::Automated => {
            (veoveo_platform_store::InvocationMode::Automated, None, None)
        }
    };
    let (owner_kind, owner_key) = match compliance
        .owner
        .as_ref()
        .context("Recording manifest outcome has no Artifact owner")?
    {
        AccessSubject::Principal(value) => (ArtifactGrantSubjectKind::Principal, value.as_str()),
        AccessSubject::Group(value) => (ArtifactGrantSubjectKind::Group, value.as_str()),
    };
    ensure!(
        metadata.artifact_uri == request.artifact_id.plane_uri()
            && metadata.byte_len == request.expected_byte_len
            && metadata.metadata == request.artifact.metadata
            && aggregate.blob.sha256 == request.expected_sha256.as_str()
            && aggregate.blob.byte_len == i64::try_from(request.expected_byte_len)?
            && occurrence.tenant == intent.tenant
            && aggregate.tenant.id == occurrence.tenant
            && aggregate.blob.tenant == occurrence.tenant
            && occurrence.blob == aggregate.blob.id
            && occurrence.id
                == PlatformArtifactId::from_uuid(request.artifact_id.as_uuid()).record_id()
            && occurrence.filename == metadata.filename
            && Some(occurrence.media_type.as_str()) == metadata.mime_type.as_deref()
            && aggregate.blob.content_type == occurrence.media_type
            && compliance
                .classification
                .as_ref()
                .map(|value| value.as_str())
                == native_classification
            && effective == labels(&occurrence.labels)?
            && compliance.retention_expires_at == occurrence.retention_expires_at
            && compliance.tenant_id.as_ref().map(|value| value.as_str())
                == Some(aggregate.tenant.slug.as_str())
            && compliance.work_context.as_ref().map(|value| value.as_str())
                == Some(occurrence.authority.context_key.as_str())
            && owner_kind == occurrence.owner_kind
            && owner_key == occurrence.owner_key
            && owner_kind == occurrence.authority.owner_kind
            && owner_key == occurrence.authority.owner_key
            && provenance.producer.as_str() == occurrence.producer_key
            && provenance.policy_revision.as_str() == occurrence.authority.policy_revision
            && mode == occurrence.invocation_mode
            && mode == occurrence.authority.invocation_mode
            && initiator == occurrence.initiator_key.as_deref()
            && initiator == occurrence.authority.initiator_key.as_deref()
            && delegation == occurrence.delegation_id.as_deref()
            && delegation == occurrence.authority.delegation_id.as_deref(),
        "Recording manifest outcome compliance differs from its selected native Artifact occurrence"
    );
    Ok(())
}

impl RecordingService {
    pub(super) async fn read_current_manifest(
        &self,
        identity: &PlatformIdentity,
        caller: &PlaneCaller,
        recording: &RecordingRecord,
        catalog_layers: &[RecordingLayerRecord],
        layers: &[ManifestLayer],
        blueprint: Option<&ManifestBlueprint>,
    ) -> Result<RecordingManifest> {
        tokio::time::timeout(
            std::time::Duration::from_secs(30),
            self.read_manifest_body(
                identity,
                caller,
                recording,
                catalog_layers,
                layers,
                blueprint,
            ),
        )
        .await
        .context("Recording manifest read deadline expired; seal outcome remains unresolved")?
    }

    pub(super) fn manifest_publication_request(
        &self,
        recording: &RecordingRecord,
        dataset_id: RecordingDatasetId,
        recording_id: RecordingId,
        manifest: &RecordingManifest,
        dataset_revision: i64,
        recording_revision: i64,
    ) -> Result<veoveo_artifact_contract::StreamArtifactRequest> {
        ensure!(
            manifest.dataset_id.as_uuid() == dataset_id.as_uuid()
                && manifest.recording_segment_id.as_uuid() == recording_id.as_uuid(),
            "Recording manifest producer selection differs from its body"
        );
        manifest_publication_descriptor(
            manifest,
            &recording.recording_key,
            artifact_classification(&recording.classification)?,
            labels(&recording.labels)?,
            u64::try_from(dataset_revision)?,
            u64::try_from(recording_revision)?,
        )
    }

    pub(super) async fn publish_manifest(
        &self,
        intent: &veoveo_recording_store::RecordingManifestPublicationRecord,
    ) -> Result<veoveo_artifact_contract::ArtifactMetadata> {
        let publisher = self
            .layer_publisher
            .as_ref()
            .context("Recording manifest publisher is not configured")?;
        let cache_root = self
            .catalog_cache_root
            .as_ref()
            .context("Recording catalog cache is not configured")?;
        let directory = cache_root.join("manifests");
        std::fs::create_dir_all(&directory)?;
        let descriptor = &intent.descriptor.0;
        let path = directory.join(format!("{}.v10.json", intent.body.0.recording_segment_id));
        let bytes = intent.bytes()?;
        if path.exists() {
            ensure!(
                std::fs::read(&path)? == bytes,
                "staged Recording manifest differs from durable intent"
            );
        } else {
            let prefix = format!("{}.v10.", intent.body.0.recording_segment_id);
            let mut temporary = tempfile::Builder::new()
                .prefix(&prefix)
                .suffix(".partial")
                .tempfile_in(&directory)?;
            use std::io::Write as _;
            temporary.write_all(&bytes)?;
            temporary.as_file().sync_all()?;
            match temporary.persist_noclobber(&path) {
                Ok(_) => {}
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    ensure!(
                        std::fs::read(&path)? == bytes,
                        "staged Recording manifest differs from durable intent"
                    );
                }
                Err(error) => return Err(error.error.into()),
            }
            File::open(&directory)?.sync_all()?;
        }
        let metadata = publisher
            .publish_artifact(
                descriptor.artifact_id,
                descriptor.artifact.clone(),
                &path,
                descriptor.expected_byte_len,
                &Sha256Digest::from_hex(descriptor.expected_sha256.as_str())?,
            )
            .await?;
        let aggregate = self
            .store
            .artifact_aggregate(PlatformArtifactId::from_uuid(
                descriptor.artifact_id.as_uuid(),
            ))
            .await?
            .context("Recording manifest publication has no confirmed native occurrence")?;
        admit_manifest_outcome(intent, &aggregate, &metadata)?;
        Ok(metadata)
    }

    pub(super) fn remove_manifest_staging(&self, recording_id: RecordingId) -> Result<()> {
        if let Some(root) = &self.catalog_cache_root {
            let directory = root.join("manifests");
            match std::fs::remove_file(directory.join(format!("{recording_id}.v10.json"))) {
                Ok(()) => File::open(directory)?.sync_all()?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub(super) async fn sealed_output(
        &self,
        identity: &PlatformIdentity,
        artifact_caller: &PlaneCaller,
        recording: RecordingRecord,
    ) -> Result<SealRecordingOutput> {
        let recording_id = RecordingId::from_uuid(record_uuid(&recording.id, "recording")?);
        let manifest = recording
            .manifest_artifact
            .as_ref()
            .context("sealed recording has no manifest artifact")?;
        let intent = self
            .recordings
            .manifest_publication(identity, recording_id)
            .await?
            .context("sealed Recording has no current publication intent")?;
        self.admit_manifest_publication(&recording, &intent)?;
        let layers = self
            .recordings
            .manifest_publication_layers(identity, recording_id, &intent)
            .await?;
        let selected_layers = layers
            .iter()
            .map(manifest_layer)
            .collect::<Result<Vec<_>>>()?;
        let layer_artifact_uris = selected_layers
            .iter()
            .map(|layer| layer.artifact_uri.clone())
            .collect();
        let selected_blueprint = self
            .recordings
            .current_recording_blueprint(identity.tenant_id, recording_id)
            .await?
            .map(manifest_blueprint)
            .transpose()?;
        self.read_current_manifest(
            identity,
            artifact_caller,
            &recording,
            &layers,
            &selected_layers,
            selected_blueprint.as_ref(),
        )
        .await?;
        let blueprint_artifact_uri = selected_blueprint.map(|blueprint| blueprint.artifact_uri);
        Ok(SealRecordingOutputBuilder {
            recording_id: crate::contract::RecordingId::try_from(recording_id.as_uuid())?,
            manifest_artifact_uri: artifact_reference(manifest)?,
            layer_artifact_uris,
            blueprint_artifact_uri,
        }
        .build()?)
    }

    pub(super) async fn admit_retained_properties(
        &self,
        identity: &PlatformIdentity,
        recording: &RecordingRecord,
        layers: &[RecordingLayerRecord],
        sealed_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<()> {
        let properties: Vec<_> = layers
            .iter()
            .filter(|layer| layer.kind == RecordingLayerKind::Properties)
            .collect();
        ensure!(
            properties.len() == 1,
            "Recording manifest selection requires one properties preparation"
        );
        let body = &properties[0]
            .properties_preparation
            .as_ref()
            .context("retained properties layer has no current original preparation")?
            .body;
        let dataset_id =
            RecordingDatasetId::from_uuid(record_uuid(&recording.dataset, "recording_dataset")?);
        let dataset = self
            .recordings
            .recording_dataset(identity.tenant_id, dataset_id)
            .await?
            .context("Recording properties dataset is missing")?;
        super::admit_properties_source(body, recording, dataset_id, &dataset.dataset_key, layers)?;
        ensure!(
            chrono::DateTime::parse_from_rfc3339(&body.sealed_at)?.with_timezone(&chrono::Utc)
                == sealed_at,
            "Recording properties preparation seal time differs from its publication intent"
        );
        Ok(())
    }

    pub(super) fn admit_manifest_publication(
        &self,
        recording: &RecordingRecord,
        intent: &veoveo_recording_store::RecordingManifestPublicationRecord,
    ) -> Result<()> {
        intent.bytes()?;
        let publisher = self
            .layer_publisher
            .as_ref()
            .context("Recording manifest publisher is not configured")?;
        ensure!(
            intent.tenant == recording.tenant
                && intent.recording == recording.id
                && intent.dataset == recording.dataset
                && intent.source_owner == recording.owner
                && intent.source_work_context == recording.work_context
                && intent.publisher.0 == *publisher.publication_context(),
            "Recording manifest intent differs from its selected source or publisher context"
        );
        Ok(())
    }

    async fn read_manifest_body(
        &self,
        identity: &PlatformIdentity,
        caller: &PlaneCaller,
        recording: &RecordingRecord,
        catalog_layers: &[RecordingLayerRecord],
        layers: &[ManifestLayer],
        blueprint: Option<&ManifestBlueprint>,
    ) -> Result<RecordingManifest> {
        let recording_id = record_uuid(&recording.id, "recording")?;
        let artifact_record = recording
            .manifest_artifact
            .as_ref()
            .context("Recording manifest occurrence is missing")?;
        let artifact_id = record_uuid(artifact_record, "artifact_occurrence")?;
        ensure!(
            artifact_id == recording_id,
            "Recording manifest occurrence differs from its reserved identity"
        );
        let aggregate = self
            .store
            .artifact_aggregate(PlatformArtifactId::from_uuid(artifact_id))
            .await?
            .context("Recording manifest occurrence is missing")?;
        ensure!(
            aggregate.occurrence.id == *artifact_record
                && aggregate.occurrence.tenant == recording.tenant,
            "Recording manifest occurrence differs from its selected source"
        );
        let intent = self
            .recordings
            .manifest_publication(identity, RecordingId::from_uuid(recording_id))
            .await?
            .context("retained Recording manifest has no current durable publication intent")?;
        self.admit_manifest_publication(recording, &intent)?;
        self.admit_retained_properties(identity, recording, catalog_layers, intent.sealed_at)
            .await?;
        let byte_len = u64::try_from(aggregate.blob.byte_len)?;
        ensure!(
            byte_len > 0 && byte_len <= MAX_RECORDING_MANIFEST_BYTES,
            "Recording manifest exceeds its byte limit"
        );
        let sha256 = Sha256Digest::from_hex(&aggregate.blob.sha256)?;
        let filename = format!("{}.recording-v10.json", recording.recording_key);
        ensure!(
            aggregate.occurrence.filename.as_deref() == Some(&filename)
                && aggregate.occurrence.media_type == MANIFEST_MIME
                && aggregate.blob.content_type == MANIFEST_MIME,
            "Recording manifest occurrence does not declare the current format"
        );
        let metadata = serde_json::to_value(&aggregate.occurrence.metadata)?;
        ensure!(
            metadata == intent.descriptor.0.artifact.metadata
                && byte_len == intent.descriptor.0.expected_byte_len
                && sha256.hex() == intent.descriptor.0.expected_sha256.as_str(),
            "Recording manifest occurrence differs from its immutable publication intent"
        );
        let owned: RecordingArtifactMetadata = serde_json::from_value(metadata.clone())
            .context("Recording manifest provenance is not current owner metadata")?;
        let uri = veoveo_artifact_contract::ArtifactId::try_from(artifact_id)?.plane_uri();
        let mut download = self.artifacts.download(caller, &uri).await.map_err(|_| {
            anyhow::anyhow!("Recording manifest Artifact read was denied or unavailable")
        })?;
        ensure!(
            download.metadata.artifact_uri == uri
                && download.metadata.byte_len == byte_len
                && download.metadata.filename.as_deref() == Some(&filename)
                && download.metadata.mime_type.as_deref() == Some(MANIFEST_MIME)
                && download.metadata.download_url.is_none()
                && download.metadata.metadata == metadata,
            "Recording manifest read metadata differs from its selected occurrence"
        );
        admit_manifest_outcome(&intent, &aggregate, &download.metadata)?;
        ensure!(
            download
                .response
                .content_length()
                .is_none_or(|size| size == byte_len),
            "Recording manifest read length differs from its occurrence"
        );
        let mut bytes = Vec::with_capacity(usize::try_from(byte_len)?);
        while let Some(chunk) = download
            .response
            .chunk()
            .await
            .map_err(|_| anyhow::anyhow!("Recording manifest Artifact body read failed"))?
        {
            ensure!(
                (bytes.len() as u64)
                    .checked_add(chunk.len() as u64)
                    .is_some_and(|size| size <= byte_len),
                "Recording manifest body exceeds its selected length"
            );
            bytes.extend_from_slice(&chunk);
        }
        ensure!(
            bytes.len() as u64 == byte_len
                && Sha256Digest::from_bytes(Sha256::digest(&bytes).into()) == sha256,
            "Recording manifest body differs from its immutable digest"
        );
        let manifest: RecordingManifest = serde_json::from_slice(&bytes)
            .context("Recording manifest body is not current v10 JSON")?;
        manifest
            .validate_selection(
                crate::contract::RecordingDatasetId::try_from(record_uuid(
                    &recording.dataset,
                    "recording_dataset",
                )?)?,
                crate::contract::RecordingId::try_from(recording_id)?,
                layers,
                blueprint,
            )
            .context("Recording manifest differs from selected layer and Blueprint facts")?;
        let RecordingArtifactProvenance::RecordingManifest {
            recording_id: provenance_recording,
            dataset_id,
            catalog_revision: original_catalog_revision,
            dataset_revision,
            recording_revision,
            sealed_at,
            sha256: provenance_sha256,
        } = owned.provenance
        else {
            anyhow::bail!("Recording manifest provenance has the wrong product kind");
        };
        ensure!(
            provenance_recording == manifest.recording_segment_id
                && dataset_id == manifest.dataset_id
                && original_catalog_revision == manifest.catalog_revision
                && sealed_at == manifest.sealed_at
                && original_catalog_revision
                    == catalog_revision(
                        i64::try_from(dataset_revision)?,
                        i64::try_from(recording_revision)?,
                        catalog_layers
                    )
                && provenance_sha256 == sha256,
            "Recording manifest body differs from its owned provenance"
        );
        // Rebuild from the captured original source epochs and currently selected
        // immutable layer/Blueprint facts. Mutable catalog rows supply no inferred epoch.
        let expected = RecordingManifestBuilder {
            schema: RecordingManifestSchema::V10,
            dataset_id: manifest.dataset_id,
            recording_segment_id: manifest.recording_segment_id,
            catalog_revision: original_catalog_revision,
            layers: layers.to_vec(),
            blueprint: blueprint.cloned(),
            sealed_at,
        }
        .build()?;
        ensure!(
            bytes == manifest_bytes(&expected)? && bytes == intent.bytes()?,
            "retained Recording manifest differs from the current publication preimage"
        );
        Ok(manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{
        ManifestLayerBuilder, RecordingLayerId, RecordingLayerKind, RecordingManifestSchema,
    };
    #[test]
    fn current_manifest_publication_preimage_and_selection() {
        let dataset =
            crate::contract::RecordingDatasetId::parse("019fa000-0000-7000-8000-000000000002")
                .unwrap();
        let recording =
            crate::contract::RecordingId::parse("019fa000-0000-7000-8000-000000000001").unwrap();
        let layer = ManifestLayerBuilder {
            layer_id: RecordingLayerId::parse("019fa000-0000-7000-8000-000000000003").unwrap(),
            layer_name: "capture-00000000000000000000".into(),
            kind: RecordingLayerKind::Capture,
            ordinal: Some(0),
            byte_len: 128.try_into().unwrap(),
            sha256: Sha256Digest::from_bytes([1; 32]),
            artifact_uri: veoveo_artifact_contract::ArtifactId::parse(
                "019fa000-0000-7000-8000-000000000003",
            )
            .unwrap()
            .plane_uri(),
            rrd_version: "0.38.1".into(),
            schema_digest: Sha256Digest::from_bytes([2; 32]),
        }
        .build()
        .unwrap();
        let manifest = RecordingManifestBuilder {
            schema: RecordingManifestSchema::V10,
            dataset_id: dataset,
            recording_segment_id: recording,
            catalog_revision: "r1".into(),
            layers: vec![layer.clone()],
            blueprint: None,
            sealed_at: "2026-10-03T00:01:00Z".parse().unwrap(),
        }
        .build()
        .unwrap();
        manifest
            .validate_selection(dataset, recording, &[layer.clone()], None)
            .unwrap();
        let bytes = manifest_bytes(&manifest).unwrap();
        let roundtrip: RecordingManifest = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(bytes, manifest_bytes(&roundtrip).unwrap());
        for pointer in [
            "/datasetId",
            "/recordingSegmentId",
            "/layers/0/layerId",
            "/layers/0/artifactUri",
        ] {
            let mut value = serde_json::to_value(&manifest).unwrap();
            *value.pointer_mut(pointer).unwrap() = if pointer.ends_with("artifactUri") {
                serde_json::json!(veoveo_artifact_contract::ArtifactId::new().plane_uri())
            } else {
                serde_json::json!(crate::contract::RecordingId::new().to_string())
            };
            let admitted: RecordingManifest = serde_json::from_value(value).unwrap();
            assert!(
                admitted
                    .validate_selection(dataset, recording, &[layer.clone()], None)
                    .is_err(),
                "{pointer}"
            );
        }
        let descriptor = manifest_publication_descriptor(
            &manifest,
            "rollover",
            Some(DataLabelId::parse("restricted").unwrap()),
            BTreeSet::from([DataLabelId::parse("restricted").unwrap()]),
            7,
            11,
        )
        .unwrap();
        assert_eq!(descriptor.expected_byte_len, bytes.len() as u64);
        assert_eq!(
            descriptor.expected_sha256.as_str(),
            Sha256Digest::from_bytes(Sha256::digest(&bytes).into()).hex()
        );
        let produced = serde_json::json!({"descriptor": descriptor,
            "descriptorSha256": hex::encode(Sha256::digest(serde_json::to_vec(&descriptor).unwrap())),
            "prettyJson": String::from_utf8(bytes.clone()).unwrap(),
            "byteLen": bytes.len(), "sha256": Sha256Digest::from_bytes(Sha256::digest(&bytes).into()).hex(),
            "filename": "rollover.recording-v10.json"});
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/manifest-preimage.json");
        if std::env::var_os("UPDATE_RECORDING_FORMAT_FIXTURES").is_some() {
            std::fs::write(
                &path,
                format!("{}\n", serde_json::to_string_pretty(&produced).unwrap()),
            )
            .unwrap();
        }
        let captured: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(produced, captured);
    }
}
