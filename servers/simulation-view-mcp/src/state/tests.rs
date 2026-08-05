use std::collections::BTreeSet;

use veoveo_mcp_contract::{
    AccessSubject, ArtifactId, FrameId, FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri,
    GroupId, LiveViewerInstanceId, PolicyVersion, PrincipalId, TenantId, WorkContextId,
    WorldFrameUri,
};
use veoveo_simulation_pose::{
    EntityId, EpochId, FrameRevision, POSE_INGRESS_CONTROL_SCHEMA, PoseIngressStatus, SessionId,
    Sha256Digest,
};

use super::lease::active_lease;
use super::*;
use crate::contract::{
    AuthorizePoseProducerRequest, BindSceneRequest, CameraAdmission, CameraRecordingPolicy,
    CameraRig, GovernedArtifact, InterpolationPolicy, LocalTransform, ProducerId, PrototypeId,
    QuaternionXyzw, RendererMode, SCENE_SCHEMA, SceneAttribution, SceneDeclaration,
    SceneDeclarationBody, SceneEntity, SceneLighting, SceneQualityPolicy, Vector3,
    VisualAssetFormat, VisualPrototype,
};

fn view_owner(principal: &str) -> LiveViewOwner {
    let principal = PrincipalId::new(principal).unwrap();
    LiveViewOwner {
        subject: AccessSubject::Principal(principal),
        tenant: TenantId::new("tenant-a").unwrap(),
        work_context: WorkContextId::new("exercise-a").unwrap(),
        policy_revision: PolicyVersion::new("2026-07-26").unwrap(),
        data_labels: BTreeSet::new(),
    }
}

fn shared_view_owner(principal: &str) -> LiveViewOwner {
    let mut owner = view_owner(principal);
    owner.subject = AccessSubject::Group(GroupId::new("flight").unwrap());
    owner
}

fn viewer_actor() -> PrincipalId {
    PrincipalId::new("issuer#viewer").unwrap()
}

fn viewer_instance(value: &str) -> LiveViewerInstanceId {
    LiveViewerInstanceId::new(value).unwrap()
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
    let revision_uri =
        FrameWorldRevisionUri::new(&world_id, &FrameWorldRevisionId::new("revision-1").unwrap());
    let simulation_frame = WorldFrameUri::new(&revision_uri, &FrameId::new("simulation").unwrap());
    let digest = Sha256Digest::new(format!("sha256:{}", "1".repeat(64))).unwrap();
    SceneDeclaration::from_body(SceneDeclarationBody {
        schema_version: SCENE_SCHEMA.to_owned(),
        session_id,
        epoch_id,
        frame_revision: FrameRevision {
            uri: revision_uri.to_string(),
            digest: digest.clone(),
        },
        simulation_frame,
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
        allowed_camera_kinds: vec![
            veoveo_mcp_contract::LiveCameraSource::Fixed,
            veoveo_mcp_contract::LiveCameraSource::FollowEntity,
        ],
        lighting: SceneLighting {
            sun_intensity_lux: 10_000.0,
            sky_intensity: 500.0,
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

fn camera_definition(stream_policy: CameraStreamPolicy) -> CameraDefinition {
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
        stream_policy,
        recording_policy: CameraRecordingPolicy::Disabled,
    }
}

fn bound_session(service: &SimulationViewService, owner: &LiveViewOwner) -> SimulationViewSession {
    let epoch_id = EpochId::new("epoch-1").unwrap();
    let session = service
        .create_session(
            owner.clone(),
            CreateSessionRequest {
                session_id: LiveSessionId::new("simulation-session-1").unwrap(),
                epoch_id: epoch_id.clone(),
            },
        )
        .unwrap();
    service
        .bind_scene(
            owner,
            BindSceneRequest {
                session_id: session.session_id.clone(),
                expected_revision: session.revision,
                scene: scene(session.session_id, epoch_id),
            },
        )
        .unwrap()
}

#[test]
fn caller_selected_session_identity_is_idempotent_and_owner_scoped() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let request = CreateSessionRequest {
        session_id: LiveSessionId::new("external-session-1").unwrap(),
        epoch_id: EpochId::new("epoch-1").unwrap(),
    };
    let created = service
        .create_session(owner.clone(), request.clone())
        .unwrap();
    let repeated = service
        .create_session(owner.clone(), request.clone())
        .unwrap();
    assert_eq!(created, repeated);

    assert!(matches!(
        service.create_session(
            owner,
            CreateSessionRequest {
                session_id: request.session_id.clone(),
                epoch_id: EpochId::new("epoch-2").unwrap(),
            },
        ),
        Err(SimulationViewError::SessionAlreadyExists(session_id))
            if session_id == request.session_id
    ));
    assert!(matches!(
        service.create_session(view_owner("issuer#other"), request),
        Err(SimulationViewError::Ownership)
    ));
}

#[test]
fn work_context_output_owner_shares_state_without_crossing_contexts() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let automated = shared_view_owner("issuer#automation");
    let console_member = shared_view_owner("issuer#operator");
    let request = CreateSessionRequest {
        session_id: LiveSessionId::new("shared-session-1").unwrap(),
        epoch_id: EpochId::new("epoch-1").unwrap(),
    };
    let created = service.create_session(automated, request.clone()).unwrap();
    assert_eq!(
        service
            .get_session(&console_member, &request.session_id)
            .unwrap(),
        created
    );

    let mut other_context = console_member;
    other_context.work_context = WorkContextId::new("exercise-b").unwrap();
    assert!(matches!(
        service.get_session(&other_context, &request.session_id),
        Err(SimulationViewError::Ownership)
    ));
}

#[test]
fn closed_session_identity_can_start_a_new_epoch() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let first = service
        .create_session(
            owner.clone(),
            CreateSessionRequest {
                session_id: LiveSessionId::new("repeatable-session").unwrap(),
                epoch_id: EpochId::new("epoch-1").unwrap(),
            },
        )
        .unwrap();
    let first = service
        .bind_scene(
            &owner,
            BindSceneRequest {
                session_id: first.session_id.clone(),
                expected_revision: first.revision,
                scene: scene(first.session_id.clone(), first.epoch_id.clone()),
            },
        )
        .unwrap();
    let first_source = service
        .authorize_pose_producer(
            &owner,
            AuthorizePoseProducerRequest {
                session_id: first.session_id.clone(),
                expected_revision: first.revision,
                producer_id: ProducerId::new("first-epoch-producer").unwrap(),
                spiffe_id: "spiffe://veoveo.test/first-epoch-producer".to_owned(),
                expires_at: Utc::now() + chrono::Duration::minutes(10),
            },
        )
        .unwrap();
    let first = service.get_session(&owner, &first.session_id).unwrap();
    service
        .close_session(
            &owner,
            CloseSessionRequest {
                session_id: first.session_id.clone(),
                expected_revision: first.revision,
            },
        )
        .unwrap();
    let restarted = service
        .create_session(
            owner,
            CreateSessionRequest {
                session_id: first.session_id,
                epoch_id: EpochId::new("epoch-2").unwrap(),
            },
        )
        .unwrap();
    assert_eq!(restarted.epoch_id.as_str(), "epoch-2");
    assert_eq!(restarted.lifecycle, SessionLifecycle::Created);
    assert_eq!(restarted.revision, 1);
    assert!(restarted.scene.is_none());
    assert!(restarted.pose_source.is_none());
    assert!(restarted.reconciliation.authorization_revision > first_source.authorization_revision);

    let restarted = service
        .bind_scene(
            &restarted.owner,
            BindSceneRequest {
                session_id: restarted.session_id.clone(),
                expected_revision: restarted.revision,
                scene: scene(restarted.session_id.clone(), restarted.epoch_id.clone()),
            },
        )
        .unwrap();
    let second_source = service
        .authorize_pose_producer(
            &restarted.owner,
            AuthorizePoseProducerRequest {
                session_id: restarted.session_id,
                expected_revision: restarted.revision,
                producer_id: ProducerId::new("second-epoch-producer").unwrap(),
                spiffe_id: "spiffe://veoveo.test/second-epoch-producer".to_owned(),
                expires_at: Utc::now() + chrono::Duration::minutes(10),
            },
        )
        .unwrap();
    assert!(second_source.authorization_revision > first_source.authorization_revision);
}

#[test]
fn admits_multiple_cameras_and_returns_typed_capacity_rejection() {
    let mut config = SimulationViewConfig::default();
    config.capacity.maximum_logical_cameras = 2;
    config.capacity.maximum_rendered_cameras = 2;
    config.capacity.maximum_cameras_per_owner = 2;
    let service = SimulationViewService::new(config).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);

    for _ in 0..2 {
        assert!(matches!(
            service
                .create_camera(
                    &owner,
                    CreateCameraRequest {
                        session_id: session.session_id.clone(),
                        definition: camera_definition(CameraStreamPolicy::OnDemand),
                    }
                )
                .unwrap(),
            CameraAdmission::Admitted { .. }
        ));
    }
    let admission = service
        .create_camera(
            &owner,
            CreateCameraRequest {
                session_id: session.session_id,
                definition: camera_definition(CameraStreamPolicy::OnDemand),
            },
        )
        .unwrap();
    assert!(matches!(
        admission,
        CameraAdmission::Rejected {
            rejection: CapacityRejection {
                dimension: CapacityDimension::LogicalCameras,
                requested: 3,
                available: 2,
                ..
            }
        }
    ));
}

#[test]
fn runtime_pose_status_refreshes_the_public_health_without_exposing_transport() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);
    let expires_at = Utc::now() + chrono::Duration::minutes(10);
    let source = service
        .authorize_pose_producer(
            &owner,
            AuthorizePoseProducerRequest {
                session_id: session.session_id.clone(),
                expected_revision: session.revision,
                producer_id: ProducerId::new("anonymous-producer").unwrap(),
                spiffe_id: "spiffe://veoveo.test/anonymous-producer".to_owned(),
                expires_at,
            },
        )
        .unwrap();
    assert!(source.stale);

    service
        .apply_pose_status(
            &session.session_id,
            &PoseIngressStatus {
                schema_version: POSE_INGRESS_CONTROL_SCHEMA.to_owned(),
                session_id: SessionId::new(session.session_id.as_str()).unwrap(),
                epoch_id: session.epoch_id.clone(),
                producer_id: source.producer_id.to_string(),
                producer_spiffe_id: source.spiffe_id,
                authorization_revision: source.authorization_revision,
                authorized_until: expires_at,
                revoked: false,
                stale: false,
                last_sequence: Some(42),
                last_snapshot_at: Some(Utc::now()),
            },
        )
        .unwrap();

    let refreshed = service.get_session(&owner, &session.session_id).unwrap();
    let source = refreshed.pose_source.unwrap();
    assert!(!source.stale);
    assert_eq!(source.last_sequence, Some(42));
    assert!(source.last_snapshot_at.is_some());
}

#[test]
fn bounded_authorization_renews_and_explicit_revocation_stops_reconciliation() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);
    let authorized_at = Utc::now();
    let source = service
        .authorize_pose_producer(
            &owner,
            AuthorizePoseProducerRequest {
                session_id: session.session_id.clone(),
                expected_revision: session.revision,
                producer_id: ProducerId::new("renewable-producer").unwrap(),
                spiffe_id: "spiffe://veoveo.test/renewable-producer".to_owned(),
                expires_at: authorized_at + chrono::Duration::seconds(30),
            },
        )
        .unwrap();
    let renewed = service
        .renew_pose_authorization(
            &session.session_id,
            authorized_at + chrono::Duration::seconds(20),
        )
        .unwrap();
    assert_eq!(
        renewed.authorization_revision,
        source.authorization_revision + 1
    );
    assert_eq!(
        renewed.authorization_lifetime_seconds,
        source.authorization_lifetime_seconds
    );
    assert!((29..=30).contains(&renewed.authorization_lifetime_seconds));
    assert!(renewed.expires_at > source.expires_at);

    let current = service.get_session(&owner, &session.session_id).unwrap();
    let revoked = service
        .revoke_pose_producer(
            &owner,
            RevokePoseProducerRequest {
                session_id: session.session_id.clone(),
                expected_revision: current.revision,
                producer_id: renewed.producer_id,
            },
        )
        .unwrap();
    assert!(revoked.revoked);
    assert!(matches!(
        service.renew_pose_authorization(&session.session_id, Utc::now()),
        Err(SimulationViewError::ProducerRevoked)
    ));
    let current = service.get_session(&owner, &session.session_id).unwrap();
    assert_eq!(current.reconciliation.phase, ReconciliationPhase::Revoked);
    assert_eq!(
        current.reconciliation.renewal_state,
        PoseAuthorizationRenewalState::Revoked
    );
}

#[test]
fn durable_state_restores_runtime_intent_without_transient_health() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);
    let source = service
        .authorize_pose_producer(
            &owner,
            AuthorizePoseProducerRequest {
                session_id: session.session_id.clone(),
                expected_revision: session.revision,
                producer_id: ProducerId::new("durable-producer").unwrap(),
                spiffe_id: "spiffe://veoveo.test/durable-producer".to_owned(),
                expires_at: Utc::now() + chrono::Duration::minutes(10),
            },
        )
        .unwrap();
    let CameraAdmission::Admitted { camera } = service
        .create_camera(
            &owner,
            CreateCameraRequest {
                session_id: session.session_id.clone(),
                definition: camera_definition(CameraStreamPolicy::OnDemand),
            },
        )
        .unwrap()
    else {
        panic!("camera should be admitted");
    };
    let stream = service
        .open_live_view(
            &owner,
            &viewer_actor(),
            OpenLiveViewRequest {
                session_id: session.session_id.clone(),
                camera_id: camera.camera_id.clone(),
                viewer_instance_id: viewer_instance("browser-1"),
            },
        )
        .unwrap();
    let desired = service.durable_state(&session.session_id).unwrap();

    let restored = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    restored.restore_durable_state(desired).unwrap();
    let restored_session = restored.get_session(&owner, &session.session_id).unwrap();
    assert_eq!(restored_session.lifecycle, SessionLifecycle::SceneBound);
    assert_eq!(
        restored_session.reconciliation.phase,
        ReconciliationPhase::Pending
    );
    assert!(!restored.reconciliation_ready());
    restored.mark_reconciliation_phase(&session.session_id, ReconciliationPhase::RendererSession);
    assert!(!restored.reconciliation_ready());
    assert_eq!(
        restored_session
            .pose_source
            .as_ref()
            .unwrap()
            .authorization_revision,
        source.authorization_revision
    );
    assert!(restored_session.pose_source.as_ref().unwrap().stale);
    assert_eq!(
        restored
            .get_camera(&owner, &camera.camera_id)
            .unwrap()
            .health,
        LiveCameraHealth::Warming
    );
    assert!(matches!(
        restored.get_stream(&owner, &viewer_actor(), &stream.stream.live_view_id),
        Err(SimulationViewError::LiveViewNotFound(_))
    ));
}

#[test]
fn blocked_reconciliation_waits_without_hiding_degraded_state() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#durability-operator");
    let session = bound_session(&service, &owner);
    service.mark_reconciliation_failed(
        &session.session_id,
        "store",
        ReconciliationFailureCode::StoreUnavailable,
        "durable state is unavailable",
        Utc::now() + chrono::Duration::minutes(1),
    );

    assert!(service.reconciliation_sessions().is_empty());
    assert!(!service.reconciliation_ready());
    let blocked = service.get_session(&owner, &session.session_id).unwrap();
    assert_eq!(blocked.reconciliation.phase, ReconciliationPhase::Blocked);
    assert_eq!(
        blocked.reconciliation.failure_code,
        Some(ReconciliationFailureCode::StoreUnavailable)
    );
    assert_eq!(
        blocked.reconciliation.failed_dependency.as_deref(),
        Some("store")
    );

    service.mark_reconciliation_failed(
        &session.session_id,
        "store",
        ReconciliationFailureCode::StoreUnavailable,
        "durable state is unavailable",
        Utc::now() - chrono::Duration::seconds(1),
    );
    assert_eq!(service.reconciliation_sessions().len(), 1);
}

#[test]
fn retry_deadline_does_not_replace_pose_renewal_deadline() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#deadline-operator");
    let session = bound_session(&service, &owner);
    service
        .authorize_pose_producer(
            &owner,
            AuthorizePoseProducerRequest {
                session_id: session.session_id.clone(),
                expected_revision: session.revision,
                producer_id: ProducerId::new("deadline-producer").unwrap(),
                spiffe_id: "spiffe://veoveo.test/deadline-producer".to_owned(),
                expires_at: Utc::now() + chrono::Duration::minutes(10),
            },
        )
        .unwrap();
    let renewal_at = Utc::now() + chrono::Duration::minutes(7);
    let retry_at = Utc::now() + chrono::Duration::seconds(30);

    service.schedule_pose_renewal(&session.session_id, renewal_at);
    service.mark_reconciliation_failed(
        &session.session_id,
        "renderer",
        ReconciliationFailureCode::RendererUnavailable,
        "renderer is unavailable",
        retry_at,
    );

    let blocked = service.get_session(&owner, &session.session_id).unwrap();
    assert_eq!(
        blocked.reconciliation.pose_authorization_renewal_at,
        Some(renewal_at)
    );
    assert_eq!(blocked.reconciliation.retry_at, Some(retry_at));
    assert_eq!(service.next_reconciliation_at(), Some(retry_at));

    service.mark_reconciliation_healthy(&session.session_id, Utc::now());
    let recovered = service.get_session(&owner, &session.session_id).unwrap();
    assert_eq!(
        recovered.reconciliation.pose_authorization_renewal_at,
        Some(renewal_at)
    );
    assert_eq!(recovered.reconciliation.retry_at, None);
    assert_eq!(service.next_reconciliation_at(), Some(renewal_at));
}

#[test]
fn routine_reconciliation_keeps_converged_durable_state_ready() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#durability-operator");
    let session = bound_session(&service, &owner);
    service.mark_reconciliation_healthy(&session.session_id, Utc::now());

    assert!(service.reconciliation_ready());
    service.mark_reconciliation_phase(&session.session_id, ReconciliationPhase::RendererSession);

    assert!(service.reconciliation_ready());
    let current = service.get_session(&owner, &session.session_id).unwrap();
    assert_eq!(current.reconciliation.phase, ReconciliationPhase::Healthy);
    assert_eq!(
        current.reconciliation.desired_revision,
        current.reconciliation.realized_revision
    );
}

#[test]
fn pose_renewal_deadline_survives_later_reconciliation_phases() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#renewal-operator");
    let session = bound_session(&service, &owner);
    let source = service
        .authorize_pose_producer(
            &owner,
            AuthorizePoseProducerRequest {
                session_id: session.session_id.clone(),
                expected_revision: session.revision,
                producer_id: ProducerId::new("renewed-producer").unwrap(),
                spiffe_id: "spiffe://veoveo.test/renewed-producer".to_owned(),
                expires_at: Utc::now() + chrono::Duration::minutes(10),
            },
        )
        .unwrap();
    let renewal_at = source.expires_at - chrono::Duration::minutes(3);

    service.schedule_pose_renewal(&session.session_id, renewal_at);
    service.mark_reconciliation_phase(&session.session_id, ReconciliationPhase::Cameras);
    service.mark_reconciliation_healthy(&session.session_id, Utc::now());

    let current = service.get_session(&owner, &session.session_id).unwrap();
    assert_eq!(
        current.reconciliation.pose_authorization_renewal_at,
        Some(renewal_at)
    );
    assert_eq!(current.reconciliation.retry_at, None);
    assert!(service.reconciliation_ready());

    service
        .state
        .lock()
        .expect("simulation-view state lock poisoned")
        .sessions
        .get_mut(&session.session_id)
        .unwrap()
        .reconciliation
        .pose_authorization_renewal_at = None;
    assert!(!service.reconciliation_ready());

    let mut state = service
        .state
        .lock()
        .expect("simulation-view state lock poisoned");
    let session = state.sessions.get_mut(&session.session_id).unwrap();
    session.reconciliation.pose_authorization_renewal_at = Some(renewal_at);
    session.pose_source.as_mut().unwrap().expires_at = Utc::now();
    drop(state);
    assert!(!service.reconciliation_ready());
}

#[test]
fn lease_tokens_are_rotated_and_checked_without_uri_authority() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);
    let CameraAdmission::Admitted { camera } = service
        .create_camera(
            &owner,
            CreateCameraRequest {
                session_id: session.session_id.clone(),
                definition: camera_definition(CameraStreamPolicy::OnDemand),
            },
        )
        .unwrap()
    else {
        panic!("camera should be admitted");
    };
    let first = service
        .open_live_view(
            &owner,
            &viewer_actor(),
            OpenLiveViewRequest {
                session_id: session.session_id.clone(),
                camera_id: camera.camera_id,
                viewer_instance_id: viewer_instance("browser-1"),
            },
        )
        .unwrap();
    assert!(
        service
            .authorize_signaling(&first.stream.live_view_id, "not-the-token")
            .is_err()
    );
    let second = service
        .renew_live_view(
            &owner,
            &viewer_actor(),
            RenewLiveViewRequest {
                session_id: session.session_id,
                live_view_id: first.stream.live_view_id.clone(),
                viewer_instance_id: viewer_instance("browser-1"),
            },
        )
        .unwrap();
    assert_ne!(
        first.access_token.expose_for_signaling(),
        second.access_token.expose_for_signaling()
    );
    assert!(
        service
            .authorize_signaling(
                &second.stream.live_view_id,
                first.access_token.expose_for_signaling()
            )
            .is_err()
    );
    assert!(
        service
            .authorize_signaling(
                &second.stream.live_view_id,
                second.access_token.expose_for_signaling()
            )
            .is_ok()
    );
}

#[test]
fn shared_camera_uses_distinct_actor_leases_and_one_stream_product() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = shared_view_owner("issuer#session-creator");
    let session = bound_session(&service, &owner);
    let CameraAdmission::Admitted { camera } = service
        .create_camera(
            &owner,
            CreateCameraRequest {
                session_id: session.session_id.clone(),
                definition: camera_definition(CameraStreamPolicy::OnDemand),
            },
        )
        .unwrap()
    else {
        panic!("camera should be admitted");
    };
    let actor_a = PrincipalId::new("issuer#viewer-a").unwrap();
    let actor_b = PrincipalId::new("issuer#viewer-b").unwrap();
    let first = service
        .open_live_view(
            &owner,
            &actor_a,
            OpenLiveViewRequest {
                session_id: session.session_id.clone(),
                camera_id: camera.camera_id.clone(),
                viewer_instance_id: viewer_instance("browser-a"),
            },
        )
        .unwrap();
    let second = service
        .open_live_view(
            &owner,
            &actor_b,
            OpenLiveViewRequest {
                session_id: session.session_id.clone(),
                camera_id: camera.camera_id,
                viewer_instance_id: viewer_instance("browser-b"),
            },
        )
        .unwrap();

    assert_ne!(first.stream.live_view_id, second.stream.live_view_id);
    assert_eq!(
        first.stream.stream_product_id,
        second.stream.stream_product_id
    );
    assert_eq!(service.capacity().usage.streamed_cameras, 1);
    assert_eq!(service.capacity().usage.nvenc_sessions, 1);
    assert!(
        service
            .authorize_signaling(
                &first.stream.live_view_id,
                first.access_token.expose_for_signaling()
            )
            .is_ok()
    );
    assert!(
        service
            .authorize_signaling(
                &second.stream.live_view_id,
                second.access_token.expose_for_signaling()
            )
            .is_ok()
    );
    assert!(matches!(
        service.close_live_view(
            &owner,
            &actor_b,
            CloseLiveViewRequest {
                session_id: session.session_id.clone(),
                live_view_id: first.stream.live_view_id.clone(),
                viewer_instance_id: viewer_instance("browser-b"),
            },
        ),
        Err(SimulationViewError::Ownership)
    ));
    service
        .close_live_view(
            &owner,
            &actor_b,
            CloseLiveViewRequest {
                session_id: session.session_id,
                live_view_id: second.stream.live_view_id,
                viewer_instance_id: viewer_instance("browser-b"),
            },
        )
        .unwrap();
    assert!(active_lease(
        &service
            .get_stream(&owner, &actor_a, &first.stream.live_view_id)
            .unwrap()
    ));
}

#[test]
fn physical_render_slots_drive_unique_media_ports() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);
    let mut cameras = Vec::new();
    for _ in 0..2 {
        let CameraAdmission::Admitted { camera } = service
            .create_camera(
                &owner,
                CreateCameraRequest {
                    session_id: session.session_id.clone(),
                    definition: camera_definition(CameraStreamPolicy::OnDemand),
                },
            )
            .unwrap()
        else {
            panic!("camera should be admitted");
        };
        cameras.push(camera);
    }
    assert_eq!(service.render_slot(&cameras[0].camera_id).unwrap(), 0);
    assert_eq!(service.render_slot(&cameras[1].camera_id).unwrap(), 1);
    let streams = cameras
        .into_iter()
        .map(|camera| {
            service
                .open_live_view(
                    &owner,
                    &viewer_actor(),
                    OpenLiveViewRequest {
                        session_id: session.session_id.clone(),
                        camera_id: camera.camera_id,
                        viewer_instance_id: viewer_instance("browser-1"),
                    },
                )
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(streams[0].stream.endpoint.media_port, 47998);
    assert_eq!(streams[1].stream.endpoint.media_port, 47999);
}

#[test]
fn cancelled_stream_admission_does_not_change_durable_intent() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);
    let CameraAdmission::Admitted { camera } = service
        .create_camera(
            &owner,
            CreateCameraRequest {
                session_id: session.session_id.clone(),
                definition: camera_definition(CameraStreamPolicy::OnDemand),
            },
        )
        .unwrap()
    else {
        panic!("camera should be admitted");
    };
    let opened = service
        .open_live_view(
            &owner,
            &viewer_actor(),
            OpenLiveViewRequest {
                session_id: session.session_id.clone(),
                camera_id: camera.camera_id,
                viewer_instance_id: viewer_instance("browser-1"),
            },
        )
        .unwrap();
    let revision_after_open = service
        .get_session(&owner, &session.session_id)
        .unwrap()
        .reconciliation
        .desired_revision;

    service.cancel_stream_admission(&opened.stream.live_view_id);

    let cancelled = service
        .get_stream(&owner, &viewer_actor(), &opened.stream.live_view_id)
        .unwrap();
    assert_eq!(cancelled.lifecycle, LiveViewLifecycle::Closed);
    assert_eq!(cancelled.connected_viewers, 0);
    let desired = service.durable_state(&session.session_id).unwrap();
    assert_eq!(
        desired.session.reconciliation.desired_revision,
        revision_after_open
    );
}

#[test]
fn changing_camera_revision_closes_existing_stream() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let session = bound_session(&service, &owner);
    let CameraAdmission::Admitted { camera } = service
        .create_camera(
            &owner,
            CreateCameraRequest {
                session_id: session.session_id.clone(),
                definition: camera_definition(CameraStreamPolicy::OnDemand),
            },
        )
        .unwrap()
    else {
        panic!("camera should be admitted");
    };
    let stream = service
        .open_live_view(
            &owner,
            &viewer_actor(),
            OpenLiveViewRequest {
                session_id: session.session_id.clone(),
                camera_id: camera.camera_id.clone(),
                viewer_instance_id: viewer_instance("browser-1"),
            },
        )
        .unwrap();
    service
        .set_camera(
            &owner,
            SetCameraRequest {
                session_id: session.session_id,
                camera_id: camera.camera_id,
                expected_revision: camera.revision,
                definition: camera_definition(CameraStreamPolicy::Continuous),
            },
        )
        .unwrap();
    assert_eq!(
        service
            .get_stream(&owner, &viewer_actor(), &stream.stream.live_view_id)
            .unwrap()
            .lifecycle,
        LiveViewLifecycle::Closed
    );
}

#[test]
fn scene_binding_is_immutable_and_owner_scoped() {
    let service = SimulationViewService::new(SimulationViewConfig::default()).unwrap();
    let owner = view_owner("issuer#operator");
    let other = view_owner("issuer#other");
    let session = bound_session(&service, &owner);
    assert!(matches!(
        service.get_session(&other, &session.session_id),
        Err(SimulationViewError::Ownership)
    ));

    let mut replacement = scene(session.session_id.clone(), session.epoch_id.clone());
    replacement.body.attribution[0].source = "Different".to_owned();
    replacement = SceneDeclaration::from_body(replacement.body).unwrap();
    assert!(matches!(
        service.bind_scene(
            &owner,
            BindSceneRequest {
                session_id: session.session_id,
                expected_revision: session.revision,
                scene: replacement,
            }
        ),
        Err(SimulationViewError::SceneAlreadyBound)
    ));
}
