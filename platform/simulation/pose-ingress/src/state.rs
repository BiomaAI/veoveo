use std::{
    collections::BTreeMap,
    os::unix::fs::PermissionsExt,
    os::unix::net::UnixDatagram,
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
    LatestPoseStore, MAXIMUM_SHARED_POSE_SLOTS, POSE_INGRESS_CONTROL_SCHEMA, PoseBinding,
    PoseError, PoseIngressBinding, PoseIngressReadiness, PoseIngressStatus, PoseSnapshot,
    PublishDisposition, SessionId, SharedPoseWriter, encode_snapshot,
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
    declaration: Mutex<PoseIngressBinding>,
    path: PathBuf,
    store: LatestPoseStore,
    limits: veoveo_simulation_pose::PoseLimits,
    writer: Mutex<SharedPoseWriter>,
    notifier: UnixDatagram,
    notification_path: PathBuf,
    status: Mutex<SessionStatus>,
}

pub(crate) struct PoseIngress {
    config: PoseIngressConfig,
    sessions: RwLock<BTreeMap<SessionId, Arc<SessionSlot>>>,
    revocation_floors: RwLock<BTreeMap<SessionId, u64>>,
    control: tokio::sync::Mutex<()>,
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
            revocation_floors: RwLock::new(BTreeMap::new()),
            control: tokio::sync::Mutex::new(()),
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
        let _control = self.control.lock().await;
        if declaration.producer.revoked {
            let revision = declaration.producer.authorization_revision;
            let mut floors = self.revocation_floors.write().await;
            let floor = floors.entry(declaration.session_id.clone()).or_default();
            *floor = (*floor).max(revision);
            drop(floors);
            if let Some(slot) = self.sessions.write().await.remove(&declaration.session_id) {
                match std::fs::remove_file(&slot.path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(PoseError::Io(error).into()),
                }
            }
            return Ok(revoked_status(&declaration));
        }
        if self
            .revocation_floors
            .read()
            .await
            .get(&declaration.session_id)
            .is_some_and(|floor| *floor >= declaration.producer.authorization_revision)
        {
            return Err(PoseIngressError::AuthorizationRevision);
        }
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
            let mut current = existing
                .declaration
                .lock()
                .expect("pose declaration lock poisoned");
            if *current == declaration {
                drop(current);
                return Ok(status(existing));
            }
            if declaration.producer.authorization_revision
                <= current.producer.authorization_revision
                || !same_binding_identity(&current, &declaration)
            {
                return Err(PoseIngressError::AuthorizationRevision);
            }
            *current = declaration;
            drop(current);
            return Ok(status(existing));
        } else if sessions.len() >= self.config.maximum_sessions {
            return Err(PoseIngressError::Capacity);
        }
        let path = self
            .config
            .directory
            .join(format!("{}.pose", declaration.session_id.as_str()));
        let history_slots = shared_pose_history_slots(&limits)?;
        let writer = SharedPoseWriter::replace(&path, limits.max_message_bytes, history_slots)?;
        let notifier = UnixDatagram::unbound().map_err(PoseError::from)?;
        notifier.set_nonblocking(true).map_err(PoseError::from)?;
        let slot = Arc::new(SessionSlot {
            declaration: Mutex::new(declaration.clone()),
            path,
            store: LatestPoseStore::new(pose_binding, limits)?,
            limits,
            writer: Mutex::new(writer),
            notifier,
            notification_path: pose_notification_path(
                &self.config.directory,
                &declaration.session_id,
            ),
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
        let declaration = slot
            .declaration
            .lock()
            .expect("pose declaration lock poisoned")
            .clone();
        if declaration.producer.spiffe_id != producer_spiffe_id
            || declaration.producer.revoked
            || declaration.producer.expires_at <= Utc::now()
        {
            return Err(PoseIngressError::Producer);
        }
        let sequence = snapshot.sequence;
        let encoded = encode_snapshot(&snapshot, &slot.limits)?;
        let disposition = slot.store.publish(snapshot)?;
        if disposition == PublishDisposition::Accepted {
            let generation = slot
                .writer
                .lock()
                .expect("pose shared-memory writer lock poisoned")
                .publish(&encoded)?;
            {
                let mut state = slot
                    .status
                    .lock()
                    .expect("pose ingress status lock poisoned");
                state.last_sequence = Some(sequence);
                state.last_snapshot_at = Some(Utc::now());
            }
            // The shared ring is authoritative. Notification is a best-effort
            // wake edge only, so a missing or saturated renderer can never
            // apply backpressure to simulation pose publication. The next
            // delivered edge drains every retained generation.
            let _ = slot
                .notifier
                .send_to(&generation.to_ne_bytes(), &slot.notification_path);
        }
        Ok(disposition)
    }
}

fn pose_notification_path(directory: &std::path::Path, session_id: &SessionId) -> PathBuf {
    let digest = Sha256::digest(session_id.as_str().as_bytes());
    let mut name = String::with_capacity(32 + ".notify".len());
    for byte in &digest[..16] {
        use std::fmt::Write as _;
        write!(&mut name, "{byte:02x}").expect("writing to a String cannot fail");
    }
    name.push_str(".notify");
    directory.join(name)
}

fn shared_pose_history_slots(
    limits: &veoveo_simulation_pose::PoseLimits,
) -> Result<usize, PoseIngressError> {
    let cadence = u128::from(limits.max_cadence_hz);
    let stale_nanoseconds = limits.stale_after.as_nanos();
    let samples = cadence
        .checked_mul(stale_nanoseconds)
        .and_then(|value| value.checked_add(999_999_999))
        .map(|value| value / 1_000_000_000)
        .and_then(|value| value.checked_add(2))
        .ok_or_else(|| PoseError::SharedSlot("shared pose history size overflow".to_owned()))?;
    let slots = usize::try_from(samples)
        .map_err(|_| PoseError::SharedSlot("shared pose history size overflow".to_owned()))?;
    if slots > MAXIMUM_SHARED_POSE_SLOTS {
        return Err(PoseError::SharedSlot(
            "pose cadence and stale window exceed shared history capacity".to_owned(),
        )
        .into());
    }
    Ok(slots.max(2))
}

fn validate_binding(declaration: &PoseIngressBinding) -> Result<(), PoseIngressError> {
    let now = Utc::now();
    if declaration.schema_version != POSE_INGRESS_CONTROL_SCHEMA
        || declaration.entity_table_revision == 0
        || declaration.producer.authorization_revision == 0
        || declaration.producer.producer_id.is_empty()
        || declaration.producer.producer_id.len() > 128
        || !declaration.producer.spiffe_id.starts_with("spiffe://")
        || declaration.producer.spiffe_id.len() > 512
        || declaration
            .producer
            .spiffe_id
            .chars()
            .any(char::is_whitespace)
        || (!declaration.producer.revoked
            && (declaration.producer.expires_at <= now
                || declaration.producer.expires_at > now + chrono::Duration::hours(24)))
    {
        return Err(PoseIngressError::Binding);
    }
    declaration.frame_revision.validate()?;
    let _: veoveo_simulation_pose::PoseLimits = declaration.limits.clone().try_into()?;
    Ok(())
}

fn status(slot: &SessionSlot) -> PoseIngressStatus {
    let declaration = slot
        .declaration
        .lock()
        .expect("pose declaration lock poisoned")
        .clone();
    let state = slot
        .status
        .lock()
        .expect("pose ingress status lock poisoned");
    PoseIngressStatus {
        schema_version: POSE_INGRESS_CONTROL_SCHEMA.to_owned(),
        session_id: declaration.session_id,
        epoch_id: declaration.epoch_id,
        producer_id: declaration.producer.producer_id,
        producer_spiffe_id: declaration.producer.spiffe_id,
        authorization_revision: declaration.producer.authorization_revision,
        authorized_until: declaration.producer.expires_at,
        revoked: declaration.producer.revoked,
        stale: slot.store.is_stale() || declaration.producer.expires_at <= Utc::now(),
        last_sequence: state.last_sequence,
        last_snapshot_at: state.last_snapshot_at,
    }
}

fn revoked_status(declaration: &PoseIngressBinding) -> PoseIngressStatus {
    PoseIngressStatus {
        schema_version: POSE_INGRESS_CONTROL_SCHEMA.to_owned(),
        session_id: declaration.session_id.clone(),
        epoch_id: declaration.epoch_id.clone(),
        producer_id: declaration.producer.producer_id.clone(),
        producer_spiffe_id: declaration.producer.spiffe_id.clone(),
        authorization_revision: declaration.producer.authorization_revision,
        authorized_until: declaration.producer.expires_at,
        revoked: true,
        stale: true,
        last_sequence: None,
        last_snapshot_at: None,
    }
}

fn same_binding_identity(left: &PoseIngressBinding, right: &PoseIngressBinding) -> bool {
    left.schema_version == right.schema_version
        && left.session_id == right.session_id
        && left.epoch_id == right.epoch_id
        && left.frame_revision == right.frame_revision
        && left.entity_table_revision == right.entity_table_revision
        && left.entity_table_digest == right.entity_table_digest
        && left.limits == right.limits
        && left.producer.producer_id == right.producer.producer_id
        && left.producer.spiffe_id == right.producer.spiffe_id
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
    #[error("pose authorization revision is stale or conflicts with the current binding")]
    AuthorizationRevision,
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
                authorization_revision: 1,
                expires_at: Utc::now() + chrono::Duration::minutes(5),
                revoked: false,
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

    #[test]
    fn shared_history_covers_the_complete_stale_window() {
        let limits = veoveo_simulation_pose::PoseLimits {
            max_entities: 8,
            max_message_bytes: 64 * 1024,
            max_cadence_hz: 120,
            stale_after: std::time::Duration::from_millis(500),
        };
        assert_eq!(shared_pose_history_slots(&limits).unwrap(), 62);

        let excessive = veoveo_simulation_pose::PoseLimits {
            max_cadence_hz: 1_000,
            stale_after: std::time::Duration::from_secs(5),
            ..limits
        };
        assert!(shared_pose_history_slots(&excessive).is_err());
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
        let notification_path = pose_notification_path(directory.path(), &declaration.session_id);
        assert_eq!(
            notification_path.file_name().unwrap(),
            "84097828fc31a8c8d29210df48901a85.notify"
        );
        let notification = UnixDatagram::bind(&notification_path).unwrap();
        notification
            .set_read_timeout(Some(std::time::Duration::from_secs(1)))
            .unwrap();
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
        let mut generation = [0_u8; 8];
        assert_eq!(notification.recv(&mut generation).unwrap(), 8);
        assert_eq!(u64::from_ne_bytes(generation), 1);
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

    #[tokio::test]
    async fn renewal_preserves_pose_state_and_revocation_rejects_stale_replay() {
        let directory = tempdir().unwrap();
        let ingress = PoseIngress::new(PoseIngressConfig::new(
            directory.path().to_owned(),
            &"a".repeat(32),
            2,
        ))
        .unwrap();
        let first = binding(SessionId::new("session-renewal").unwrap());
        ingress.bind(first.clone()).await.unwrap();
        ingress
            .publish("spiffe://example.test/fixture", snapshot(&first, 1))
            .await
            .unwrap();

        let mut renewed = first.clone();
        renewed.producer.authorization_revision = 2;
        renewed.producer.expires_at += chrono::Duration::minutes(5);
        let renewed_status = ingress.bind(renewed.clone()).await.unwrap();
        assert_eq!(renewed_status.authorization_revision, 2);
        assert_eq!(renewed_status.last_sequence, Some(1));
        ingress
            .publish("spiffe://example.test/fixture", snapshot(&renewed, 2))
            .await
            .unwrap();
        assert_eq!(
            ingress
                .status(&renewed.session_id)
                .await
                .unwrap()
                .last_sequence,
            Some(2)
        );
        assert!(matches!(
            ingress.bind(first).await,
            Err(PoseIngressError::AuthorizationRevision)
        ));

        let mut revoked = renewed.clone();
        revoked.producer.authorization_revision = 3;
        revoked.producer.revoked = true;
        let status = ingress.bind(revoked).await.unwrap();
        assert!(status.revoked);
        assert!(status.stale);
        assert!(!directory.path().join("session-renewal.pose").exists());
        assert!(matches!(
            ingress.bind(renewed).await,
            Err(PoseIngressError::AuthorizationRevision)
        ));
    }
}
