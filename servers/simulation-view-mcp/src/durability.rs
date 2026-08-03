use std::sync::Arc;

#[cfg(test)]
use std::collections::BTreeMap;

use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::{
    LiveCameraHealth, LiveCameraId, LiveSessionId, LiveViewId, LiveViewLifecycle, LiveViewOwner,
};
use veoveo_platform_store::{
    AuditEventId, AuditEventRecord, AuditOutcome, OpenObject, PlatformStore,
    SIMULATION_VIEW_DESIRED_DIGEST_SCHEMA, SimulationViewStateDraft, StoreError,
    deterministic_tenant_id,
};
use veoveo_simulation_pose::EpochId;

use crate::{
    contract::{
        CameraDefinition, ProducerId, ReconciliationStatus, SceneDeclaration, SessionLifecycle,
    },
    state::{DurableSimulationViewState, SimulationViewService},
};

const DESIRED_INTENT_SCHEMA: &str = "veoveo.io/simulation-view-desired-intent/v1";

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SanitizedErrorCause {
    depth: usize,
    kind: &'static str,
}

pub(crate) fn sanitized_error_chain(error: &anyhow::Error) -> Vec<SanitizedErrorCause> {
    error
        .chain()
        .enumerate()
        .map(|(depth, cause)| SanitizedErrorCause {
            depth,
            kind: if let Some(store) = cause.downcast_ref::<StoreError>() {
                match store {
                    StoreError::SimulationViewStateNotFound(_) => "simulation_view_state_not_found",
                    StoreError::SimulationViewRevisionGap { .. } => "simulation_view_revision_gap",
                    StoreError::SimulationViewRevisionConflict { .. } => {
                        "simulation_view_revision_conflict"
                    }
                    StoreError::Database(_) => "platform_store_database",
                    StoreError::Config(_) => "platform_store_config",
                    StoreError::Migration(_) => "platform_store_migration",
                    _ => "platform_store",
                }
            } else if cause.is::<serde_json::Error>() {
                "serialization"
            } else if cause.is::<std::io::Error>() {
                "io"
            } else if cause.is::<crate::contract::SimulationViewError>() {
                "simulation_view_state"
            } else {
                "dependency_or_context"
            },
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesiredSimulationViewIntent<'a> {
    schema_version: &'static str,
    session: DesiredSessionIntent<'a>,
    cameras: Vec<DesiredCameraIntent<'a>>,
    streams: Vec<DesiredStreamIntent<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesiredSessionIntent<'a> {
    session_id: &'a LiveSessionId,
    epoch_id: &'a EpochId,
    owner: &'a LiveViewOwner,
    revision: u64,
    closed: bool,
    scene: Option<&'a SceneDeclaration>,
    pose_producer: Option<DesiredPoseProducerIntent<'a>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesiredPoseProducerIntent<'a> {
    producer_id: &'a ProducerId,
    spiffe_id: &'a str,
    authorization_revision: u64,
    authorization_lifetime_seconds: u64,
    revoked: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesiredCameraIntent<'a> {
    camera_id: &'a LiveCameraId,
    session_id: &'a LiveSessionId,
    owner: &'a LiveViewOwner,
    revision: u64,
    definition: &'a CameraDefinition,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesiredStreamIntent<'a> {
    live_view_id: &'a LiveViewId,
    session_id: &'a LiveSessionId,
    camera_id: &'a LiveCameraId,
    owner: &'a LiveViewOwner,
    camera_revision: u64,
    requested: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct SimulationViewRepository {
    store: PlatformStore,
}

impl SimulationViewRepository {
    pub fn new(store: PlatformStore) -> Arc<Self> {
        Arc::new(Self { store })
    }

    pub async fn ready(&self) -> bool {
        self.store.client().health().await.is_ok()
    }

    pub async fn restore(&self, service: &SimulationViewService) -> Result<usize> {
        let records = self
            .store
            .simulation_view_states()
            .await
            .context("read durable Simulation View desired state")?;
        let mut restored = 0;
        for record in records {
            let mut desired: DurableSimulationViewState = serde_json::from_value(
                serde_json::Value::Object(record.snapshot.into_map().into_iter().collect()),
            )
            .context("decode durable Simulation View desired state")?;
            desired.session.reconciliation = serde_json::from_value(serde_json::Value::Object(
                record.reconciliation.into_map().into_iter().collect(),
            ))
            .context("decode durable Simulation View reconciliation status")?;
            service
                .restore_durable_state(desired)
                .context("restore durable Simulation View desired state")?;
            restored += 1;
        }
        Ok(restored)
    }

    pub async fn persist(
        &self,
        service: &SimulationViewService,
        session_id: &veoveo_mcp_contract::LiveSessionId,
    ) -> Result<()> {
        let durable = service
            .durable_state(session_id)
            .context("snapshot Simulation View desired state")?;
        let desired_digest = desired_intent_digest(&durable)?;
        let reconciliation = durable.session.reconciliation.clone();
        let mut snapshot_state = durable;
        normalize_restorable_state(&mut snapshot_state);
        let snapshot_value = serde_json::to_value(&snapshot_state)?;
        let snapshot = object(snapshot_value, "Simulation View desired state")?;
        let reconciliation_object = object(
            serde_json::to_value(&reconciliation)?,
            "Simulation View reconciliation status",
        )?;
        let source = snapshot_state.session.pose_source.as_ref();
        self.store
            .commit_simulation_view_state(SimulationViewStateDraft {
                tenant_key: snapshot_state.session.owner.tenant.as_str().to_owned(),
                owner_key: serde_json::to_string(&snapshot_state.session.owner.subject)?,
                work_context_key: snapshot_state
                    .session
                    .owner
                    .work_context
                    .as_str()
                    .to_owned(),
                policy_revision: snapshot_state
                    .session
                    .owner
                    .policy_revision
                    .as_str()
                    .to_owned(),
                session_id: snapshot_state.session.session_id.as_str().to_owned(),
                epoch_id: snapshot_state.session.epoch_id.as_str().to_owned(),
                desired_revision: reconciliation.desired_revision,
                realized_revision: reconciliation.realized_revision,
                authorization_revision: reconciliation.authorization_revision,
                revoked: source.is_some_and(|value| value.revoked),
                authorization_expires_at: source.map(|value| value.expires_at),
                desired_digest,
                desired_digest_schema: SIMULATION_VIEW_DESIRED_DIGEST_SCHEMA.to_owned(),
                snapshot,
                reconciliation: reconciliation_object,
                updated_at: Utc::now(),
            })
            .await
            .context("commit Simulation View desired state")?;
        Ok(())
    }

    pub async fn audit(
        &self,
        owner: &LiveViewOwner,
        session_id: &str,
        action: &str,
        outcome: AuditOutcome,
        details: impl Serialize,
    ) -> Result<()> {
        let tenant = deterministic_tenant_id(owner.tenant.as_str())?.record_id();
        let occurred_at = Utc::now();
        self.store
            .append_simulation_view_audit(AuditEventRecord {
                id: AuditEventId::new().record_id(),
                tenant: Some(tenant),
                actor: None,
                action: action.to_owned(),
                resource_type: "simulation_view_reconciliation".to_owned(),
                resource_id: Some(session_id.to_owned()),
                outcome,
                request_id: None,
                trace_id: None,
                source_ip: None,
                details: object(serde_json::to_value(details)?, "audit details")?,
                occurred_at,
                search_text: format!("simulation_view_reconciliation {action} {session_id}"),
            })
            .await
            .context("append Simulation View reconciliation audit")
    }
}

pub(crate) fn desired_intent_digest(durable: &DurableSimulationViewState) -> Result<String> {
    let pose_producer =
        durable
            .session
            .pose_source
            .as_ref()
            .map(|source| DesiredPoseProducerIntent {
                producer_id: &source.producer_id,
                spiffe_id: &source.spiffe_id,
                authorization_revision: source.authorization_revision,
                authorization_lifetime_seconds: source.authorization_lifetime_seconds,
                revoked: source.revoked,
            });
    let mut cameras = durable
        .cameras
        .iter()
        .map(|durable| DesiredCameraIntent {
            camera_id: &durable.camera.camera_id,
            session_id: &durable.camera.session_id,
            owner: &durable.camera.owner,
            revision: durable.camera.revision,
            definition: &durable.camera.definition,
        })
        .collect::<Vec<_>>();
    cameras.sort_by(|left, right| left.camera_id.as_str().cmp(right.camera_id.as_str()));
    let mut streams = durable
        .leases
        .iter()
        .map(|lease| DesiredStreamIntent {
            live_view_id: &lease.state.live_view_id,
            session_id: &lease.state.session_id,
            camera_id: &lease.state.camera_id,
            owner: &lease.state.owner,
            camera_revision: lease.state.camera_revision,
            requested: lease.state.lifecycle != LiveViewLifecycle::Closed,
        })
        .collect::<Vec<_>>();
    streams.sort_by(|left, right| left.live_view_id.as_str().cmp(right.live_view_id.as_str()));
    let intent = DesiredSimulationViewIntent {
        schema_version: DESIRED_INTENT_SCHEMA,
        session: DesiredSessionIntent {
            session_id: &durable.session.session_id,
            epoch_id: &durable.session.epoch_id,
            owner: &durable.session.owner,
            revision: durable.session.revision,
            closed: durable.session.lifecycle == SessionLifecycle::Closed,
            scene: durable.session.scene.as_ref(),
            pose_producer,
        },
        cameras,
        streams,
    };
    Ok(hex_digest(&serde_json::to_vec(&intent)?))
}

fn normalize_restorable_state(desired: &mut DurableSimulationViewState) {
    let desired_revision = desired.session.reconciliation.desired_revision;
    desired.session.reconciliation = ReconciliationStatus::pending(desired_revision);
    if desired.session.lifecycle != SessionLifecycle::Closed {
        desired.session.lifecycle = if desired.session.scene.is_some() {
            SessionLifecycle::SceneBound
        } else {
            SessionLifecycle::Created
        };
    }
    if let Some(source) = desired.session.pose_source.as_mut() {
        source.last_sequence = None;
        source.last_snapshot_at = None;
        source.stale = true;
    }
    for durable in &mut desired.cameras {
        durable.camera.health = LiveCameraHealth::Warming;
        durable.camera.last_pose_sequence = None;
        durable.camera.last_frame_at = None;
    }
    for lease in &mut desired.leases {
        lease.state.connected_viewers = 0;
        if !matches!(
            lease.state.lifecycle,
            LiveViewLifecycle::Closed | LiveViewLifecycle::Failed
        ) {
            lease.state.lifecycle = LiveViewLifecycle::Ready;
            lease.state.camera_health = LiveCameraHealth::Warming;
            lease.state.last_frame_at = None;
        }
    }
}

fn object(value: serde_json::Value, label: &'static str) -> Result<OpenObject> {
    match value {
        serde_json::Value::Object(values) => Ok(OpenObject::new(values.into_iter().collect())),
        _ => anyhow::bail!("{label} must serialize as an object"),
    }
}

fn hex_digest(value: &[u8]) -> String {
    let digest = Sha256::digest(value);
    let mut output = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use secrecy::SecretString;
    use uuid::Uuid;
    use veoveo_mcp_contract::{
        AccessSubject, ArtifactId, FrameId, FrameWorldId, FrameWorldRevisionId,
        FrameWorldRevisionUri, PolicyVersion, PrincipalId, TenantId, WorkContextId, WorldFrameUri,
    };
    use veoveo_platform_store::{StoreConfig, StoreCredentials};
    use veoveo_simulation_pose::{EntityId, FrameRevision, Sha256Digest};

    use super::*;
    use crate::{
        contract::{
            AuthorizePoseProducerRequest, BindSceneRequest, CameraAdmission, CameraDefinition,
            CameraRecordingPolicy, CameraRig, CameraStreamPolicy, CreateCameraRequest,
            CreateSessionRequest, GovernedArtifact, InterpolationPolicy, LocalTransform,
            OpenLiveViewRequest, ProducerId, PrototypeId, QuaternionXyzw, RendererMode,
            SCENE_SCHEMA, SceneAttribution, SceneDeclaration, SceneDeclarationBody, SceneEntity,
            SceneLighting, SceneQualityPolicy, Vector3, VisualAssetFormat, VisualPrototype,
        },
        state::{SimulationViewConfig, SimulationViewService},
    };

    struct DurableFixture {
        service: Arc<SimulationViewService>,
        session_id: LiveSessionId,
        camera_ids: Vec<LiveCameraId>,
        stream_id: LiveViewId,
    }

    fn view_owner() -> LiveViewOwner {
        LiveViewOwner {
            subject: AccessSubject::Principal(
                PrincipalId::new("issuer#durability-operator").unwrap(),
            ),
            tenant: TenantId::new("tenant-a").unwrap(),
            work_context: WorkContextId::new("exercise-a").unwrap(),
            policy_revision: PolicyVersion::new("2026-08-02").unwrap(),
            data_labels: BTreeSet::new(),
        }
    }

    fn identity_transform() -> LocalTransform {
        LocalTransform {
            translation_m: Vector3 {
                x: 0.0,
                y: 0.0,
                z: 0.0,
            },
            orientation_xyzw: QuaternionXyzw {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                w: 1.0,
            },
            scale: Vector3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
        }
    }

    fn scene(session_id: LiveSessionId, epoch_id: EpochId) -> SceneDeclaration {
        let world_id = FrameWorldId::new("synthetic-world").unwrap();
        let revision_uri = FrameWorldRevisionUri::new(
            &world_id,
            &FrameWorldRevisionId::new("revision-1").unwrap(),
        );
        let digest = Sha256Digest::new(format!("sha256:{}", "1".repeat(64))).unwrap();
        SceneDeclaration::from_body(SceneDeclarationBody {
            schema_version: SCENE_SCHEMA.to_owned(),
            session_id,
            epoch_id,
            frame_revision: FrameRevision {
                uri: revision_uri.to_string(),
                digest: digest.clone(),
            },
            simulation_frame: WorldFrameUri::new(
                &revision_uri,
                &FrameId::new("simulation").unwrap(),
            ),
            geospatial_layer_id: None,
            environment: GovernedArtifact {
                artifact_uri: ArtifactId::new().plane_uri(),
                digest: digest.clone(),
                format: VisualAssetFormat::Usd,
                byte_length: 1024,
            },
            prototypes: vec![VisualPrototype {
                prototype_id: PrototypeId::new("marker").unwrap(),
                asset: GovernedArtifact {
                    artifact_uri: ArtifactId::new().plane_uri(),
                    digest,
                    format: VisualAssetFormat::Glb,
                    byte_length: 512,
                },
                local_alignment: identity_transform(),
            }],
            entities: vec![SceneEntity {
                entity_id: EntityId::new("entity-1").unwrap(),
                prototype_id: PrototypeId::new("marker").unwrap(),
                static_transform: None,
            }],
            allowed_camera_kinds: vec![veoveo_mcp_contract::LiveCameraSource::FollowEntity],
            lighting: SceneLighting {
                intensity_lux: 10_000.0,
                color_temperature_kelvin: 6500,
            },
            quality: SceneQualityPolicy {
                renderer: RendererMode::RaytracedLighting,
                maximum_texture_dimension: 4096,
                maximum_asset_bytes: 1_000_000,
                interpolation: InterpolationPolicy::Linear,
                maximum_pose_age_ms: 500,
            },
            attribution: vec![SceneAttribution {
                source: "Synthetic conformance fixture".to_owned(),
                license: "CC0-1.0".to_owned(),
                attribution_url: Some("https://example.test/fixture".to_owned()),
            }],
        })
        .unwrap()
    }

    fn camera_definition() -> CameraDefinition {
        CameraDefinition {
            rig: CameraRig::FollowEntity {
                target_entity: EntityId::new("entity-1").unwrap(),
                offset_flu_m: Vector3 {
                    x: -5.0,
                    y: 0.0,
                    z: 2.0,
                },
                smoothing_seconds: 0.1,
            },
            width_px: 1280,
            height_px: 720,
            frame_rate_millihertz: 30_000,
            vertical_fov_degrees: 60.0,
            near_clip_m: 0.1,
            far_clip_m: 10_000.0,
            stream_policy: CameraStreamPolicy::OnDemand,
            recording_policy: CameraRecordingPolicy::Disabled,
        }
    }

    async fn persist_if_configured(
        repository: Option<&SimulationViewRepository>,
        service: &SimulationViewService,
        session_id: &LiveSessionId,
    ) {
        if let Some(repository) = repository {
            repository.persist(service, session_id).await.unwrap();
        }
    }

    async fn durable_fixture(repository: Option<&SimulationViewRepository>) -> DurableFixture {
        let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
        let owner = view_owner();
        let epoch_id = EpochId::new("epoch-1").unwrap();
        let session = service
            .create_session(
                owner.clone(),
                CreateSessionRequest {
                    session_id: LiveSessionId::new("durable-session").unwrap(),
                    epoch_id: epoch_id.clone(),
                },
            )
            .unwrap();
        let session_id = session.session_id.clone();
        persist_if_configured(repository, &service, &session_id).await;
        let session = service
            .bind_scene(
                &owner,
                BindSceneRequest {
                    session_id: session_id.clone(),
                    expected_revision: session.revision,
                    scene: scene(session_id.clone(), epoch_id),
                },
            )
            .unwrap();
        persist_if_configured(repository, &service, &session_id).await;
        service
            .authorize_pose_producer(
                &owner,
                AuthorizePoseProducerRequest {
                    session_id: session_id.clone(),
                    expected_revision: session.revision,
                    producer_id: ProducerId::new("anonymous-producer").unwrap(),
                    spiffe_id: "spiffe://veoveo.test/anonymous-producer".to_owned(),
                    expires_at: Utc::now() + chrono::Duration::minutes(10),
                },
            )
            .unwrap();
        persist_if_configured(repository, &service, &session_id).await;
        let mut camera_ids = Vec::new();
        for _ in 0..3 {
            let CameraAdmission::Admitted { camera } = service
                .create_camera(
                    &owner,
                    CreateCameraRequest {
                        session_id: session_id.clone(),
                        definition: camera_definition(),
                    },
                )
                .unwrap()
            else {
                panic!("camera should be admitted");
            };
            camera_ids.push(camera.camera_id.clone());
            persist_if_configured(repository, &service, &session_id).await;
        }
        let stream_id = service
            .open_live_view(
                &owner,
                OpenLiveViewRequest {
                    session_id: session_id.clone(),
                    camera_id: camera_ids[0].clone(),
                },
            )
            .unwrap()
            .stream
            .live_view_id;
        persist_if_configured(repository, &service, &session_id).await;
        DurableFixture {
            service,
            session_id,
            camera_ids,
            stream_id,
        }
    }

    #[test]
    fn digest_is_stable_and_lowercase_hex() {
        assert_eq!(
            hex_digest(b"managed-session"),
            "6bc26d8cb052e27e7ac4b4bb616b3be4582500a0331b7b1a626424ec6efcafac"
        );
    }

    #[test]
    fn open_objects_reject_non_objects() {
        assert!(object(serde_json::json!([]), "fixture").is_err());
        assert_eq!(
            object(serde_json::json!({"healthy": true}), "fixture")
                .unwrap()
                .as_map(),
            &BTreeMap::from([("healthy".to_owned(), serde_json::json!(true))])
        );
    }

    #[test]
    fn persistence_logs_keep_the_complete_typed_chain_without_values() {
        let error = anyhow::Error::new(StoreError::SimulationViewRevisionConflict { revision: 7 })
            .context("commit Simulation View desired state");

        assert_eq!(
            sanitized_error_chain(&error),
            vec![
                SanitizedErrorCause {
                    depth: 0,
                    kind: "dependency_or_context",
                },
                SanitizedErrorCause {
                    depth: 1,
                    kind: "simulation_view_revision_conflict",
                },
            ]
        );
        assert!(!format!("{:?}", sanitized_error_chain(&error)).contains('7'));
    }

    #[tokio::test]
    async fn desired_intent_digest_excludes_runtime_and_realized_state() {
        let fixture = durable_fixture(None).await;
        let initial =
            desired_intent_digest(&fixture.service.durable_state(&fixture.session_id).unwrap())
                .unwrap();

        fixture.service.mutate_runtime_state_for_test(
            &fixture.session_id,
            &fixture.camera_ids,
            &fixture.stream_id,
            7,
        );
        let after_runtime_changes =
            desired_intent_digest(&fixture.service.durable_state(&fixture.session_id).unwrap())
                .unwrap();
        assert_eq!(after_runtime_changes, initial);

        fixture
            .service
            .mutate_camera_intent_for_test(&fixture.camera_ids[0]);
        let after_desired_change =
            desired_intent_digest(&fixture.service.durable_state(&fixture.session_id).unwrap())
                .unwrap();
        assert_ne!(after_desired_change, initial);
    }

    #[tokio::test]
    async fn real_store_commits_realized_state_across_transient_updates() {
        if std::env::var("VEOVEO_SURREAL_INTEGRATION").as_deref() != Ok("1") {
            return;
        }

        let endpoint = std::env::var("VEOVEO_SURREAL_URL")
            .unwrap_or_else(|_| "ws://127.0.0.1:8000".to_owned());
        let username = std::env::var("VEOVEO_SURREAL_USER").unwrap_or_else(|_| "root".to_owned());
        let password =
            std::env::var("VEOVEO_SURREAL_PASSWORD").unwrap_or_else(|_| "root".to_owned());
        let store = PlatformStore::connect(
            StoreConfig::builder(
                &endpoint,
                "veoveo_integration",
                format!("simulation_view_digest_test_{}", Uuid::now_v7().simple()),
                StoreCredentials::root(username, SecretString::from(password)),
            )
            .migrate_on_connect(true)
            .build()
            .unwrap(),
        )
        .await
        .unwrap();
        let repository = SimulationViewRepository::new(store.clone());
        let fixture = durable_fixture(Some(&repository)).await;
        let initial = store.simulation_view_states().await.unwrap().remove(0);

        for sequence in 1..=3 {
            fixture.service.mutate_runtime_state_for_test(
                &fixture.session_id,
                &fixture.camera_ids,
                &fixture.stream_id,
                sequence,
            );
            repository
                .persist(&fixture.service, &fixture.session_id)
                .await
                .unwrap();
            let persisted = store.simulation_view_states().await.unwrap().remove(0);
            assert_eq!(persisted.desired_digest, initial.desired_digest);
            assert_eq!(persisted.desired_revision, initial.desired_revision);
        }

        fixture
            .service
            .mark_reconciliation_healthy(&fixture.session_id, Utc::now());
        repository
            .persist(&fixture.service, &fixture.session_id)
            .await
            .unwrap();
        let realized = store.simulation_view_states().await.unwrap().remove(0);
        assert_eq!(realized.realized_revision, realized.desired_revision);
        assert_eq!(realized.desired_digest, initial.desired_digest);

        fixture
            .service
            .mutate_camera_intent_for_test(&fixture.camera_ids[0]);
        let error = repository
            .persist(&fixture.service, &fixture.session_id)
            .await
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("conflicts with its durable digest"),
            "unexpected error: {error:?}"
        );
    }
}
