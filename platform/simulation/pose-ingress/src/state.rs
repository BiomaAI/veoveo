use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use chrono::Utc;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tokio::sync::RwLock;
use veoveo_simulation_pose::{
    LatestPoseStore, POSE_INGRESS_CONTROL_SCHEMA, PoseBinding, PoseError, PoseIngressBinding,
    PoseIngressReadiness, PoseIngressStatus, PoseSnapshot, PublishDisposition, SessionId,
    SharedPoseWriter, encode_snapshot,
};

#[derive(Debug, Clone)]
pub(crate) struct PoseIngressConfig {
    pub directory: PathBuf,
    pub control_token_hash: [u8; 32],
    pub maximum_sessions: usize,
}

impl PoseIngressConfig {
    pub fn new(directory: PathBuf, control_token: &str, maximum_sessions: usize) -> Self {
        Self {
            directory,
            control_token_hash: Sha256::digest(control_token.as_bytes()).into(),
            maximum_sessions,
        }
    }
}

#[derive(Debug)]
struct SessionStatus {
    last_sequence: Option<u64>,
    last_snapshot_at: Option<chrono::DateTime<Utc>>,
}

struct SessionSlot {
    declaration: PoseIngressBinding,
    path: PathBuf,
    store: LatestPoseStore,
    limits: veoveo_simulation_pose::PoseLimits,
    writer: Mutex<SharedPoseWriter>,
    status: Mutex<SessionStatus>,
}

pub(crate) struct PoseIngress {
    config: PoseIngressConfig,
    sessions: RwLock<BTreeMap<SessionId, Arc<SessionSlot>>>,
    tls_listening: AtomicBool,
}

impl PoseIngress {
    pub fn new(config: PoseIngressConfig) -> Result<Self, PoseError> {
        if config.maximum_sessions == 0 {
            return Err(PoseError::SharedSlot(
                "maximum pose sessions must be positive".to_owned(),
            ));
        }
        std::fs::create_dir_all(&config.directory)?;
        std::fs::set_permissions(&config.directory, std::fs::Permissions::from_mode(0o700))?;
        Ok(Self {
            config,
            sessions: RwLock::new(BTreeMap::new()),
            tls_listening: AtomicBool::new(false),
        })
    }

    pub fn authorize_control(&self, token: &str) -> bool {
        let supplied: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        supplied.ct_eq(&self.config.control_token_hash).unwrap_u8() == 1
    }

    pub fn mark_tls_listening(&self) {
        self.tls_listening.store(true, Ordering::Release);
    }

    pub fn readiness(&self) -> PoseIngressReadiness {
        PoseIngressReadiness {
            ready: self.tls_listening.load(Ordering::Acquire),
            protocol_schema: veoveo_simulation_pose::POSE_PROTOCOL_SCHEMA.to_owned(),
            mutually_authenticated: true,
        }
    }

    pub async fn bind(
        &self,
        declaration: PoseIngressBinding,
    ) -> Result<PoseIngressStatus, PoseIngressError> {
        validate_binding(&declaration)?;
        let limits: veoveo_simulation_pose::PoseLimits = declaration.limits.clone().try_into()?;
        let pose_binding = PoseBinding {
            session_id: declaration.session_id.clone(),
            epoch_id: declaration.epoch_id.clone(),
            frame_revision: declaration.frame_revision.clone(),
            entity_table_revision: declaration.entity_table_revision,
            entity_table_digest: declaration.entity_table_digest.clone(),
        };
        let mut sessions = self.sessions.write().await;
        if let Some(existing) = sessions.get(&declaration.session_id) {
            if existing.declaration == declaration {
                return Ok(status(existing));
            }
        } else if sessions.len() >= self.config.maximum_sessions {
            return Err(PoseIngressError::Capacity);
        }
        let path = self
            .config
            .directory
            .join(format!("{}.pose", declaration.session_id.as_str()));
        let writer = SharedPoseWriter::replace(&path, limits.max_message_bytes)?;
        let slot = Arc::new(SessionSlot {
            declaration: declaration.clone(),
            path,
            store: LatestPoseStore::new(pose_binding, limits)?,
            limits,
            writer: Mutex::new(writer),
            status: Mutex::new(SessionStatus {
                last_sequence: None,
                last_snapshot_at: None,
            }),
        });
        sessions.insert(declaration.session_id, slot.clone());
        Ok(status(&slot))
    }

    pub async fn revoke(&self, session_id: &SessionId) -> Result<(), PoseIngressError> {
        let slot = self
            .sessions
            .write()
            .await
            .remove(session_id)
            .ok_or(PoseIngressError::NotFound)?;
        match std::fs::remove_file(&slot.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(PoseError::Io(error).into()),
        }
    }

    pub async fn status(
        &self,
        session_id: &SessionId,
    ) -> Result<PoseIngressStatus, PoseIngressError> {
        self.sessions
            .read()
            .await
            .get(session_id)
            .map(|slot| status(slot))
            .ok_or(PoseIngressError::NotFound)
    }

    pub async fn publish(
        &self,
        producer_spiffe_id: &str,
        snapshot: PoseSnapshot,
    ) -> Result<PublishDisposition, PoseIngressError> {
        let slot = self
            .sessions
            .read()
            .await
            .get(&snapshot.session_id)
            .cloned()
            .ok_or(PoseIngressError::NotFound)?;
        if slot.declaration.producer.spiffe_id != producer_spiffe_id
            || slot.declaration.producer.expires_at <= Utc::now()
        {
            return Err(PoseIngressError::Producer);
        }
        let sequence = snapshot.sequence;
        let encoded = encode_snapshot(&snapshot, &slot.limits)?;
        let disposition = slot.store.publish(snapshot)?;
        if disposition == PublishDisposition::Accepted {
            slot.writer
                .lock()
                .expect("pose shared-memory writer lock poisoned")
                .publish(&encoded)?;
            let mut state = slot
                .status
                .lock()
                .expect("pose ingress status lock poisoned");
            state.last_sequence = Some(sequence);
            state.last_snapshot_at = Some(Utc::now());
        }
        Ok(disposition)
    }
}

fn validate_binding(declaration: &PoseIngressBinding) -> Result<(), PoseIngressError> {
    let now = Utc::now();
    if declaration.schema_version != POSE_INGRESS_CONTROL_SCHEMA
        || declaration.entity_table_revision == 0
        || declaration.producer.producer_id.is_empty()
        || declaration.producer.producer_id.len() > 128
        || !declaration.producer.spiffe_id.starts_with("spiffe://")
        || declaration.producer.spiffe_id.len() > 512
        || declaration
            .producer
            .spiffe_id
            .chars()
            .any(char::is_whitespace)
        || declaration.producer.expires_at <= now
        || declaration.producer.expires_at > now + chrono::Duration::hours(24)
    {
        return Err(PoseIngressError::Binding);
    }
    declaration.frame_revision.validate()?;
    let _: veoveo_simulation_pose::PoseLimits = declaration.limits.clone().try_into()?;
    Ok(())
}

fn status(slot: &SessionSlot) -> PoseIngressStatus {
    let state = slot
        .status
        .lock()
        .expect("pose ingress status lock poisoned");
    PoseIngressStatus {
        schema_version: POSE_INGRESS_CONTROL_SCHEMA.to_owned(),
        session_id: slot.declaration.session_id.clone(),
        epoch_id: slot.declaration.epoch_id.clone(),
        producer_id: slot.declaration.producer.producer_id.clone(),
        producer_spiffe_id: slot.declaration.producer.spiffe_id.clone(),
        authorized_until: slot.declaration.producer.expires_at,
        stale: slot.store.is_stale() || slot.declaration.producer.expires_at <= Utc::now(),
        last_sequence: state.last_sequence,
        last_snapshot_at: state.last_snapshot_at,
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PoseIngressError {
    #[error("pose binding is invalid")]
    Binding,
    #[error("pose session was not found")]
    NotFound,
    #[error("pose session capacity is exhausted")]
    Capacity,
    #[error("producer identity is not authorized")]
    Producer,
    #[error(transparent)]
    Pose(#[from] PoseError),
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;
    use veoveo_simulation_pose::{
        CoordinateConvention, EntityId, EntityPose, EnuPosition, EpochId, FrameRevision,
        PoseIngressLimits, PoseProducerAuthorization, QuaternionXyzw, Rgba8, SemanticDisplayState,
        Sha256Digest, SharedPoseReader, entity_table_digest,
    };

    use super::*;

    fn binding(session_id: SessionId) -> PoseIngressBinding {
        let entities = vec![EntityPose {
            entity_id: EntityId::new("entity-1").unwrap(),
            position: EnuPosition {
                east_m: 0.0,
                north_m: 0.0,
                up_m: 1.0,
            },
            orientation: QuaternionXyzw {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            active: true,
            visible: true,
            velocity: None,
            display: Some(SemanticDisplayState {
                color: Rgba8 {
                    red: 1,
                    green: 2,
                    blue: 3,
                    alpha: 255,
                },
                status_code: 7,
            }),
        }];
        PoseIngressBinding {
            schema_version: POSE_INGRESS_CONTROL_SCHEMA.to_owned(),
            session_id,
            epoch_id: EpochId::new("epoch-1").unwrap(),
            frame_revision: FrameRevision {
                uri: "frames://world/synthetic/revision/r1".to_owned(),
                digest: Sha256Digest::new(format!("sha256:{}", "1".repeat(64))).unwrap(),
            },
            entity_table_revision: 1,
            entity_table_digest: entity_table_digest(1, &entities),
            limits: PoseIngressLimits {
                maximum_entities: 8,
                maximum_message_bytes: 64 * 1024,
                maximum_cadence_hz: 120,
                stale_after_ms: 500,
            },
            producer: PoseProducerAuthorization {
                producer_id: "fixture".to_owned(),
                spiffe_id: "spiffe://example.test/fixture".to_owned(),
                expires_at: Utc::now() + chrono::Duration::minutes(5),
            },
        }
    }

    fn snapshot(binding: &PoseIngressBinding, sequence: u64) -> PoseSnapshot {
        PoseSnapshot {
            protocol_version: veoveo_simulation_pose::POSE_PROTOCOL_VERSION,
            session_id: binding.session_id.clone(),
            epoch_id: binding.epoch_id.clone(),
            sequence,
            simulation_timestamp_ns: i64::try_from(sequence).unwrap() * 10_000_000,
            frame_revision: binding.frame_revision.clone(),
            coordinate_convention: CoordinateConvention::EnuMetersFluXyzw,
            entity_table_revision: binding.entity_table_revision,
            entity_table_digest: binding.entity_table_digest.clone(),
            entities: vec![EntityPose {
                entity_id: EntityId::new("entity-1").unwrap(),
                position: EnuPosition {
                    east_m: sequence as f64,
                    north_m: 0.0,
                    up_m: 1.0,
                },
                orientation: QuaternionXyzw {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                    w: 1.0,
                },
                active: true,
                visible: true,
                velocity: None,
                display: None,
            }],
        }
    }

    #[tokio::test]
    async fn publishes_only_authorized_latest_snapshots_to_atomic_shared_memory() {
        let directory = tempdir().unwrap();
        let ingress = PoseIngress::new(PoseIngressConfig::new(
            directory.path().to_owned(),
            &"a".repeat(32),
            2,
        ))
        .unwrap();
        assert!(ingress.authorize_control(&"a".repeat(32)));
        assert!(!ingress.authorize_control(&"b".repeat(32)));
        assert!(!ingress.readiness().ready);
        ingress.mark_tls_listening();
        assert!(ingress.readiness().ready);

        let declaration = binding(SessionId::new("session-1").unwrap());
        ingress.bind(declaration.clone()).await.unwrap();
        assert!(matches!(
            ingress
                .publish("spiffe://example.test/other", snapshot(&declaration, 1))
                .await,
            Err(PoseIngressError::Producer)
        ));
        assert_eq!(
            ingress
                .publish("spiffe://example.test/fixture", snapshot(&declaration, 1))
                .await
                .unwrap(),
            PublishDisposition::Accepted
        );
        assert_eq!(
            ingress
                .publish("spiffe://example.test/fixture", snapshot(&declaration, 1))
                .await
                .unwrap(),
            PublishDisposition::DroppedStale
        );
        let reader = SharedPoseReader::open(&directory.path().join("session-1.pose")).unwrap();
        assert!(reader.latest().unwrap().is_some());
        let pose_path = directory.path().join("session-1.pose");
        assert_eq!(
            std::fs::metadata(&pose_path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let status = ingress.status(&declaration.session_id).await.unwrap();
        assert_eq!(status.last_sequence, Some(1));
        assert!(!status.stale);
        ingress.revoke(&declaration.session_id).await.unwrap();
        assert!(!pose_path.exists());
        assert!(matches!(
            ingress.status(&declaration.session_id).await,
            Err(PoseIngressError::NotFound)
        ));
        assert!(reader.latest().unwrap().is_some());
    }
}
