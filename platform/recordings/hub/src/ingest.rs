//! Authenticated batch journal and materializer for external recording streams.

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Cursor, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};
use prost::Message;
use re_log_encoding::Decoder;
use re_log_types::{LogMsg, StoreKind};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    GatewayInternalResourceIdentity, PrincipalKind as ContractPrincipalKind, ProtectedResourceId,
};
use veoveo_platform_store::{
    PlatformIdentity, PlatformStore, PrincipalId, PrincipalKind, RecordId, RecordIdKey,
    RecordingDraft, RecordingId, RecordingIngestBatchDraft, RecordingIngestBatchState,
    RecordingIngestStreamId, RecordingIngestStreamRecord, RecordingIngestStreamState, SegmentDraft,
    SegmentId, SegmentState, TenantId,
};
use veoveo_recording_protocol::{
    DEFAULT_MAXIMUM_BATCH_BYTES, REQUIRED_SCOPE,
    v1::{
        AppendRecordingBatchResult, AuthorizedRecordingProducer, RecordingBatch, RecordingStream,
        RecordingStreamState, RerunPayloadFormat,
    },
};

use crate::inspect_segment;

#[derive(Clone, Debug)]
pub struct RecordingIngestServiceConfig {
    pub journal_root: PathBuf,
    pub spool_root: PathBuf,
    pub protected_resource: ProtectedResourceId,
    pub maximum_batch_bytes: u64,
}

impl RecordingIngestServiceConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.journal_root.is_absolute(),
            "journal root must be absolute"
        );
        ensure!(self.spool_root.is_absolute(), "spool root must be absolute");
        ensure!(
            self.maximum_batch_bytes > 0 && self.maximum_batch_bytes <= DEFAULT_MAXIMUM_BATCH_BYTES,
            "maximum batch bytes must be in 1..={DEFAULT_MAXIMUM_BATCH_BYTES}"
        );
        ensure!(
            self.journal_root != self.spool_root,
            "journal and spool roots must be distinct"
        );
        Ok(())
    }
}

#[derive(Clone)]
pub struct RecordingIngestService {
    store: PlatformStore,
    config: RecordingIngestServiceConfig,
}

impl RecordingIngestService {
    pub fn new(store: PlatformStore, config: RecordingIngestServiceConfig) -> Result<Self> {
        config.validate()?;
        std::fs::create_dir_all(&config.journal_root).with_context(|| {
            format!("creating ingest journal {}", config.journal_root.display())
        })?;
        std::fs::create_dir_all(&config.spool_root)
            .with_context(|| format!("creating recording spool {}", config.spool_root.display()))?;
        let mut config = config;
        config.journal_root = config.journal_root.canonicalize()?;
        config.spool_root = config.spool_root.canonicalize()?;
        Ok(Self { store, config })
    }

    pub async fn open(
        &self,
        gateway: &GatewayInternalResourceIdentity,
        producer: &AuthorizedRecordingProducer,
        source_stream_id: &str,
        application_id: &str,
        recording_key: &str,
    ) -> Result<RecordingStream> {
        let identity = self
            .authorize(gateway, producer, Some(application_id))
            .await?;
        validate_text("source_stream_id", source_stream_id)?;
        validate_text("recording_id", recording_key)?;
        let recording = self
            .store
            .create_recording(RecordingDraft {
                identity: identity.clone(),
                dataset: producer.dataset.clone(),
                application_id: application_id.to_owned(),
                recording_key: recording_key.to_owned(),
                classification: producer.classification.clone(),
                labels: producer.labels.clone(),
                metadata: std::collections::BTreeMap::from([
                    (
                        "source".to_owned(),
                        serde_json::json!("authenticated-recording-ingest"),
                    ),
                    (
                        "producer_id".to_owned(),
                        serde_json::json!(producer.producer_id),
                    ),
                ]),
                started_at: chrono::Utc::now(),
            })
            .await?;
        let recording_id = typed_record_uuid::<RecordingId>(&recording.id, RecordingId::TABLE)?;
        let stream = self
            .store
            .open_recording_ingest_stream(veoveo_platform_store::RecordingIngestStreamDraft {
                identity,
                recording_id,
                producer_id: producer.producer_id.clone(),
                oauth_client_id: producer.oauth_client_id.clone(),
                source_stream_id: source_stream_id.to_owned(),
                application_id: application_id.to_owned(),
                recording_key: recording_key.to_owned(),
                dataset: producer.dataset.clone(),
            })
            .await?;
        self.stream_response(&stream)
    }

    pub async fn status(
        &self,
        gateway: &GatewayInternalResourceIdentity,
        producer: &AuthorizedRecordingProducer,
        stream_id: RecordingIngestStreamId,
    ) -> Result<RecordingStream> {
        let identity = self.authorize(gateway, producer, None).await?;
        let stream = self
            .authorized_stream(&identity, producer, stream_id)
            .await?;
        self.stream_response(&stream)
    }

    pub async fn append(
        &self,
        gateway: &GatewayInternalResourceIdentity,
        producer: &AuthorizedRecordingProducer,
        stream_id: RecordingIngestStreamId,
        batch: &RecordingBatch,
    ) -> Result<AppendRecordingBatchResult> {
        batch.validate(self.config.maximum_batch_bytes)?;
        let identity = self.authorize(gateway, producer, None).await?;
        let stream = self
            .authorized_stream(&identity, producer, stream_id)
            .await?;
        ensure!(
            stream.byte_len
                + i64::try_from(batch.encoded_rrd.len()).context("batch length exceeds i64")?
                <= i64::try_from(producer.maximum_stream_bytes)
                    .context("stream limit exceeds i64")?,
            "stream byte quota exceeded"
        );
        validate_rrd_identity(
            &batch.encoded_rrd,
            batch.message_count,
            &stream.application_id,
            &stream.recording_key,
        )?;
        let (journal_path, relative_path) =
            self.write_journal(identity.tenant_id, stream_id, batch)?;
        let outcome = self
            .store
            .commit_recording_ingest_batch(RecordingIngestBatchDraft {
                identity: identity.clone(),
                stream_id,
                sequence: batch.sequence,
                payload_format: payload_format_name(batch.payload_format)?.to_owned(),
                sha256: hex::encode(&batch.sha256),
                relative_path,
                byte_len: batch.encoded_rrd.len() as u64,
                message_count: batch.message_count,
            })
            .await?;
        let mut stream = outcome.stream;
        if outcome.batch.state == RecordingIngestBatchState::Durable {
            stream = self
                .materialize(&identity, stream_id, &stream, batch, &journal_path)
                .await?;
        } else {
            remove_if_exists(&journal_path)?;
        }
        Ok(AppendRecordingBatchResult {
            durable_through_sequence: durable_through(&stream)?,
            materialized_through_sequence: materialized_through(&stream)?,
            duplicate: outcome.duplicate,
        })
    }

    pub async fn finish(
        &self,
        gateway: &GatewayInternalResourceIdentity,
        producer: &AuthorizedRecordingProducer,
        stream_id: RecordingIngestStreamId,
    ) -> Result<RecordingStream> {
        let identity = self.authorize(gateway, producer, None).await?;
        self.authorized_stream(&identity, producer, stream_id)
            .await?;
        let stream = self
            .store
            .finish_recording_ingest_stream(identity.tenant_id, stream_id)
            .await?;
        self.stream_response(&stream)
    }

    pub async fn reconcile(&self) -> Result<usize> {
        let mut reconciled = 0;
        for tenant_entry in std::fs::read_dir(&self.config.journal_root)? {
            let tenant_entry = tenant_entry?;
            if !tenant_entry.file_type()?.is_dir() {
                continue;
            }
            let tenant_id = TenantId::from_uuid(uuid::Uuid::parse_str(
                tenant_entry.file_name().to_string_lossy().as_ref(),
            )?);
            for stream_entry in std::fs::read_dir(tenant_entry.path())? {
                let stream_entry = stream_entry?;
                if !stream_entry.file_type()?.is_dir() {
                    continue;
                }
                let stream_id = RecordingIngestStreamId::from_uuid(uuid::Uuid::parse_str(
                    stream_entry.file_name().to_string_lossy().as_ref(),
                )?);
                let stream = self
                    .store
                    .recording_ingest_stream(tenant_id, stream_id)
                    .await?
                    .context("journal references an unknown recording ingest stream")?;
                let identity = identity_from_stream(&stream)?;
                for journal_entry in std::fs::read_dir(stream_entry.path())? {
                    let journal_entry = journal_entry?;
                    if journal_entry
                        .path()
                        .extension()
                        .and_then(|value| value.to_str())
                        != Some("pb")
                    {
                        continue;
                    }
                    let bytes = std::fs::read(journal_entry.path())?;
                    let batch = RecordingBatch::decode(bytes.as_slice())?;
                    batch.validate(self.config.maximum_batch_bytes)?;
                    let relative_path = journal_entry
                        .path()
                        .strip_prefix(&self.config.journal_root)?
                        .to_str()
                        .context("journal path is not UTF-8")?
                        .to_owned();
                    let outcome = self
                        .store
                        .commit_recording_ingest_batch(RecordingIngestBatchDraft {
                            identity: identity.clone(),
                            stream_id,
                            sequence: batch.sequence,
                            payload_format: payload_format_name(batch.payload_format)?.to_owned(),
                            sha256: hex::encode(&batch.sha256),
                            relative_path,
                            byte_len: batch.encoded_rrd.len() as u64,
                            message_count: batch.message_count,
                        })
                        .await?;
                    if outcome.batch.state == RecordingIngestBatchState::Durable {
                        self.materialize(
                            &identity,
                            stream_id,
                            &outcome.stream,
                            &batch,
                            &journal_entry.path(),
                        )
                        .await?;
                    } else {
                        remove_if_exists(&journal_entry.path())?;
                    }
                    reconciled += 1;
                }
            }
        }
        Ok(reconciled)
    }

    async fn authorize(
        &self,
        gateway: &GatewayInternalResourceIdentity,
        producer: &AuthorizedRecordingProducer,
        application_id: Option<&str>,
    ) -> Result<PlatformIdentity> {
        ensure!(
            gateway.protected_resource == self.config.protected_resource,
            "internal token protected resource mismatch"
        );
        ensure!(
            gateway.principal.kind == ContractPrincipalKind::Service,
            "recording ingest requires a service principal"
        );
        ensure!(
            gateway.principal.subject.as_str() == producer.oauth_client_id,
            "producer OAuth client binding mismatch"
        );
        ensure!(
            gateway
                .principal
                .tenant
                .as_ref()
                .map(|tenant| tenant.as_str())
                == Some(producer.tenant_id.as_str()),
            "producer tenant binding mismatch"
        );
        ensure!(
            gateway
                .principal
                .scopes
                .iter()
                .any(|scope| scope.as_str() == REQUIRED_SCOPE),
            "recording ingest scope is missing"
        );
        validate_producer(producer)?;
        if let Some(application_id) = application_id {
            ensure!(
                producer
                    .allowed_application_ids
                    .iter()
                    .any(|allowed| allowed == application_id),
                "application_id is not allowed for producer"
            );
        }
        Ok(self
            .store
            .ensure_identity(
                &producer.tenant_id,
                &producer.producer_id,
                gateway.principal.issuer.as_str(),
                gateway.principal.subject.as_str(),
                PrincipalKind::Service,
            )
            .await?)
    }

    async fn authorized_stream(
        &self,
        identity: &PlatformIdentity,
        producer: &AuthorizedRecordingProducer,
        stream_id: RecordingIngestStreamId,
    ) -> Result<RecordingIngestStreamRecord> {
        let stream = self
            .store
            .recording_ingest_stream(identity.tenant_id, stream_id)
            .await?
            .context("recording ingest stream was not found")?;
        ensure!(
            stream.owner == identity.principal_id.record_id()
                && stream.producer_id == producer.producer_id
                && stream.oauth_client_id == producer.oauth_client_id
                && stream.dataset == producer.dataset,
            "recording ingest stream ownership mismatch"
        );
        Ok(stream)
    }

    fn write_journal(
        &self,
        tenant_id: TenantId,
        stream_id: RecordingIngestStreamId,
        batch: &RecordingBatch,
    ) -> Result<(PathBuf, String)> {
        let directory = self
            .config
            .journal_root
            .join(tenant_id.to_string())
            .join(stream_id.to_string());
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!("{:020}.pb", batch.sequence));
        let encoded = batch.encode_to_vec();
        if path.exists() {
            ensure!(std::fs::read(&path)? == encoded, "journal batch conflict");
        } else {
            let temporary = directory.join(format!(
                ".{:020}.{}.tmp",
                batch.sequence,
                uuid::Uuid::now_v7()
            ));
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&encoded)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &path)?;
            sync_directory(&directory)?;
        }
        let relative = path
            .strip_prefix(&self.config.journal_root)?
            .to_str()
            .context("journal path is not UTF-8")?
            .to_owned();
        Ok((path, relative))
    }

    async fn materialize(
        &self,
        identity: &PlatformIdentity,
        stream_id: RecordingIngestStreamId,
        stream: &RecordingIngestStreamRecord,
        batch: &RecordingBatch,
        journal_path: &Path,
    ) -> Result<RecordingIngestStreamRecord> {
        let directory = self
            .config
            .spool_root
            .join(&stream.dataset)
            .join(stream.opened_at.date_naive().format("%Y-%m-%d").to_string());
        std::fs::create_dir_all(&directory)?;
        let path = directory.join(format!(
            "{}.ingest-{}-{:020}.rrd",
            sanitize(&stream.recording_key),
            stream_id,
            batch.sequence
        ));
        publish_segment(&path, &batch.encoded_rrd)?;
        let inspection = inspect_segment(&path)?;
        ensure!(
            inspection.application_id == stream.application_id
                && inspection.recording_key == stream.recording_key
                && inspection.sha256 == hex::encode(&batch.sha256),
            "materialized segment identity or digest changed"
        );
        let recording_id = typed_record_uuid::<RecordingId>(&stream.recording, RecordingId::TABLE)?;
        let relative_path = path
            .strip_prefix(&self.config.spool_root)?
            .to_str()
            .context("segment path is not UTF-8")?
            .to_owned();
        let segment = match self
            .store
            .segment_by_key(identity.tenant_id, recording_id, &relative_path)
            .await?
        {
            Some(segment) => segment,
            None => {
                self.store
                    .open_segment(SegmentDraft {
                        identity: identity.clone(),
                        recording_id,
                        segment_key: relative_path.clone(),
                        ordinal: i64::try_from(batch.sequence - 1)?,
                        relative_path,
                        start_time: Some(stream.opened_at),
                    })
                    .await?
            }
        };
        if segment.state == SegmentState::Writing {
            let segment_id = typed_record_uuid::<SegmentId>(&segment.id, SegmentId::TABLE)?;
            self.store
                .freeze_segment(
                    identity,
                    segment_id,
                    i64::try_from(inspection.byte_len)?,
                    i64::try_from(batch.message_count)?,
                    &inspection.sha256,
                    Some(chrono::Utc::now()),
                )
                .await?;
        }
        let stream = self
            .store
            .mark_recording_ingest_materialized(identity.tenant_id, stream_id, batch.sequence)
            .await?;
        remove_if_exists(journal_path)?;
        Ok(stream)
    }

    fn stream_response(&self, stream: &RecordingIngestStreamRecord) -> Result<RecordingStream> {
        let stream_id = typed_record_uuid::<RecordingIngestStreamId>(
            &stream.id,
            RecordingIngestStreamId::TABLE,
        )?;
        let recording_id = typed_record_uuid::<RecordingId>(&stream.recording, RecordingId::TABLE)?;
        Ok(RecordingStream {
            stream_id: stream_id.to_string(),
            recording_uri: format!("recording://recordings/{recording_id}"),
            state: match stream.state {
                RecordingIngestStreamState::Open => RecordingStreamState::Open.into(),
                RecordingIngestStreamState::Finished => RecordingStreamState::Finished.into(),
                RecordingIngestStreamState::Failed => RecordingStreamState::Failed.into(),
            },
            next_sequence: u64::try_from(stream.next_sequence)?,
            durable_through_sequence: durable_through(stream)?,
            materialized_through_sequence: materialized_through(stream)?,
            maximum_batch_bytes: self.config.maximum_batch_bytes,
        })
    }
}

fn validate_rrd_identity(
    encoded_rrd: &[u8],
    declared_message_count: u64,
    application_id: &str,
    recording_key: &str,
) -> Result<()> {
    let decoder = Decoder::<LogMsg>::decode_eager(BufReader::new(Cursor::new(encoded_rrd)))?;
    let mut count = 0_u64;
    for message in decoder {
        let message = message?;
        ensure!(
            message.store_id().kind() == StoreKind::Recording,
            "RRD batch contains a non-recording store"
        );
        ensure!(
            message.store_id().application_id().as_str() == application_id
                && message.store_id().recording_id().as_str() == recording_key,
            "RRD batch identity does not match its stream"
        );
        count += 1;
    }
    ensure!(
        count == declared_message_count,
        "RRD message count mismatch"
    );
    Ok(())
}

fn validate_producer(producer: &AuthorizedRecordingProducer) -> Result<()> {
    for (field, value) in [
        ("producer_id", producer.producer_id.as_str()),
        ("oauth_client_id", producer.oauth_client_id.as_str()),
        ("tenant_id", producer.tenant_id.as_str()),
        ("dataset", producer.dataset.as_str()),
        ("classification", producer.classification.as_str()),
    ] {
        validate_text(field, value)?;
    }
    ensure!(
        !producer.allowed_application_ids.is_empty(),
        "producer application allowlist must not be empty"
    );
    ensure!(
        producer.maximum_stream_bytes > 0,
        "producer stream byte limit must be positive"
    );
    Ok(())
}

fn validate_text(field: &str, value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= 512 && !value.chars().any(char::is_control),
        "{field} is empty or invalid"
    );
    Ok(())
}

fn payload_format_name(value: i32) -> Result<&'static str> {
    match RerunPayloadFormat::try_from(value) {
        Ok(RerunPayloadFormat::Rrd0341) => Ok("rrd_0_34_1"),
        _ => anyhow::bail!("unsupported Rerun payload format"),
    }
}

fn durable_through(stream: &RecordingIngestStreamRecord) -> Result<u64> {
    Ok(u64::try_from(stream.next_sequence - 1)?)
}

fn materialized_through(stream: &RecordingIngestStreamRecord) -> Result<u64> {
    Ok(stream
        .materialized_through_sequence
        .map(u64::try_from)
        .transpose()?
        .unwrap_or(0))
}

fn publish_segment(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        ensure!(
            Sha256::digest(std::fs::read(path)?) == Sha256::digest(bytes),
            "materialized segment conflict"
        );
        return Ok(());
    }
    let directory = path.parent().context("segment path has no parent")?;
    let temporary = path.with_extension(format!("rrd.{}.tmp", uuid::Uuid::now_v7()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    sync_directory(directory)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
        if let Some(parent) = path.parent() {
            sync_directory(parent)?;
        }
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn identity_from_stream(stream: &RecordingIngestStreamRecord) -> Result<PlatformIdentity> {
    let tenant_id = typed_record_uuid::<TenantId>(&stream.tenant, TenantId::TABLE)?;
    Ok(PlatformIdentity {
        tenant_id,
        principal_id: typed_record_uuid::<PrincipalId>(&stream.owner, PrincipalId::TABLE)?,
        tenant_key: tenant_id.to_string(),
        principal_key: stream.producer_id.clone(),
    })
}

trait TypedRecordId: Sized {
    const TABLE: &'static str;
    fn from_uuid(value: uuid::Uuid) -> Self;
}

macro_rules! typed_record_id {
    ($type:ty) => {
        impl TypedRecordId for $type {
            const TABLE: &'static str = <$type>::TABLE;
            fn from_uuid(value: uuid::Uuid) -> Self {
                <$type>::from_uuid(value)
            }
        }
    };
}

typed_record_id!(TenantId);
typed_record_id!(PrincipalId);
typed_record_id!(RecordingId);
typed_record_id!(SegmentId);
typed_record_id!(RecordingIngestStreamId);

fn typed_record_uuid<T: TypedRecordId>(record: &RecordId, expected_table: &str) -> Result<T> {
    ensure!(
        expected_table == T::TABLE && record.table.as_str() == expected_table,
        "record has the wrong table"
    );
    let raw = match &record.key {
        RecordIdKey::Uuid(value) => value.to_string(),
        RecordIdKey::String(value) => value.clone(),
        other => anyhow::bail!("record key is not a UUID: {other:?}"),
    };
    let uuid = uuid::Uuid::parse_str(&raw)?;
    ensure!(uuid.get_version_num() == 7, "record ID is not UUIDv7");
    Ok(T::from_uuid(uuid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_filename_is_confined() {
        assert_eq!(sanitize("run/../camera"), "run_.._camera");
    }

    #[test]
    fn service_config_rejects_overlarge_batches() {
        let config = RecordingIngestServiceConfig {
            journal_root: PathBuf::from("/journal"),
            spool_root: PathBuf::from("/spool"),
            protected_resource: ProtectedResourceId::new("https://example.test/ingest").unwrap(),
            maximum_batch_bytes: DEFAULT_MAXIMUM_BATCH_BYTES + 1,
        };
        assert!(config.validate().is_err());
    }
}
