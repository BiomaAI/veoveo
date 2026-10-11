use super::*;
use crate::contract::LiveViewerInstanceId;
use crate::{adapter::FakeAdapter, server::service::fake_state};
use surrealdb::types::SurrealValue;
use tokio::sync::Mutex as TokioMutex;
use veoveo_types::{AccessSubject, DataLabelId, GroupId, PolicyVersion, TenantId, WorkContextId};

#[derive(surrealdb::types::SurrealValue)]
struct AuditCount {
    count: i64,
}

async fn audit_counts(store: &veoveo_platform_store::PlatformStore) -> Vec<i64> {
    let mut response = store
        .client()
        .query("SELECT count() AS count FROM audit_record GROUP ALL;")
        .await
        .unwrap()
        .check()
        .unwrap();
    let rows: Vec<AuditCount> = response.take(0).unwrap();
    rows.into_iter().map(|row| row.count).collect()
}

fn owner() -> LiveViewOwner {
    LiveViewOwner {
        subject: AccessSubject::Group(GroupId::parse("operators").unwrap()),
        tenant: TenantId::parse("tenant-a").unwrap(),
        work_context: WorkContextId::parse("context-a").unwrap(),
        policy_revision: PolicyVersion::parse("policy-1").unwrap(),
        data_labels: [DataLabelId::parse("simulation").unwrap()]
            .into_iter()
            .collect(),
    }
}

fn audit_context(principal: PrincipalId) -> AuditContext {
    use veoveo_mcp_contract::audit::{
        AuditActor, AuditAuthority, AuditPrincipalKind, AuditRequest,
    };
    let owner = owner();
    AuditContext {
        actor: AuditActor {
            principal,
            kind: AuditPrincipalKind::User,
            tenant: Some(owner.tenant),
            oauth_client: None,
            session_family: None,
            delegating_principal: None,
            managed_agent: None,
        },
        authority: AuditAuthority {
            work_context: Some(owner.work_context),
            policy_revision: Some(owner.policy_revision),
            data_labels: owner.data_labels,
            ..Default::default()
        },
        request: AuditRequest::background(),
    }
}

async fn service() -> (
    Arc<LiveViewService>,
    Arc<Adapter>,
    crate::server::test_support::fixture::TestDb,
) {
    let db = crate::server::test_support::database(
        crate::server::test_support::fixture::StoreBackend::Memory,
    )
    .await;
    let adapter = Arc::new(Adapter::Fake(Arc::new(TokioMutex::new(FakeAdapter::new(
        fake_state().unwrap(),
    )))));
    let service = LiveViewService::new_for_test(
        db.a.clone(),
        adapter.clone(),
        LiveViewConfig {
            session_duration: Duration::from_secs(30),
            public_stream_url: "wss://example.test/uav-sim/live".to_owned(),
            maximum_frame_age_ms: 2_000,
        },
    )
    .await
    .unwrap();
    (service, adapter, db)
}

fn request(instance: &str) -> OpenLiveViewRequest {
    OpenLiveViewRequest {
        session_id: LiveSessionId::parse("session-alpha").unwrap(),
        camera_id: LiveCameraId::parse("follow").unwrap(),
        viewer_instance_id: LiveViewerInstanceId::parse(instance).unwrap(),
    }
}

#[tokio::test]
async fn twenty_five_viewers_share_one_camera_atlas() {
    let (service, adapter, _db) = service().await;
    let mut opened = Vec::new();
    for index in 0..25 {
        opened.push(
            service
                .open(
                    owner(),
                    audit_context(PrincipalId::parse(format!("viewer-{index}")).unwrap()),
                    request(&format!("browser-{index}")),
                )
                .await
                .unwrap(),
        );
    }
    assert!(opened.windows(2).all(|pair| {
        pair[0].stream.stream_product_id == pair[1].stream.stream_product_id
            && pair[0].stream.stream_product_id == pair[1].stream.stream_product_id
            && pair[0].stream.live_view_id != pair[1].stream.live_view_id
    }));
    let mut runtime = adapter.state().await.unwrap();
    service.project_product_usage(&mut runtime).await.unwrap();
    let product = runtime
        .stream_products
        .iter()
        .find(|product| {
            product
                .camera_regions
                .iter()
                .any(|region| region.camera_id.as_str() == "follow")
        })
        .unwrap();
    assert_eq!(product.active_viewers, 25);
    assert_eq!(product.nvenc_sessions, 1);
}

#[tokio::test]
async fn viewer_pages_apply_ownership_before_the_limit() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (service, _, _db) = service().await;
        let actor = PrincipalId::parse("alice").unwrap();
        let other = PrincipalId::parse("bob").unwrap();
        for i in 0..102 {
            service
                .open(
                    owner(),
                    audit_context(other.clone()),
                    request(&format!("other-{i}")),
                )
                .await
                .unwrap();
        }
        let mut expected = std::collections::BTreeSet::new();
        for i in 0..102 {
            let connection = service
                .open(
                    owner(),
                    audit_context(actor.clone()),
                    request(&format!("own-{i}")),
                )
                .await
                .unwrap();
            expected.insert(connection.stream.live_view_id);
        }
        let session = LiveSessionId::parse("session-alpha").unwrap();
        let cursor =
            |view: &LiveViewState| -> anyhow::Result<String> {
                Ok(crate::contract::UavLiveViewCursor::new(
                    session.clone(),
                    view.live_view_id.clone(),
                )?
                .as_str()
                .to_owned())
            };
        let rows = service
            .page(&owner(), &actor, &session, None)
            .await
            .unwrap();
        assert_eq!(rows.len(), 101);
        let first = super::super::index::page(rows, cursor, Ok).unwrap();
        assert_eq!(first.items.len(), 100);
        let after = crate::contract::UavLiveViewCursor::parse(&session, first.next_cursor.unwrap())
            .unwrap();
        let rows = service
            .page(&owner(), &actor, &session, Some(after.position()))
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        let last = super::super::index::page(rows, cursor, Ok).unwrap();
        assert!(last.next_cursor.is_none());
        assert_eq!(
            first
                .items
                .into_iter()
                .chain(last.items)
                .map(|view| view.live_view_id)
                .collect::<std::collections::BTreeSet<_>>(),
            expected
        );
        assert!(
            service
                .page(
                    &owner(),
                    &actor,
                    &LiveSessionId::parse("other-session").unwrap(),
                    None
                )
                .await
                .unwrap()
                .is_empty()
        );
        let mut denied = owner();
        denied.work_context = WorkContextId::parse("other-context").unwrap();
        assert!(
            service
                .page(&denied, &actor, &session, None)
                .await
                .unwrap()
                .is_empty()
        );
    })
    .await
    .expect("live-view page test exceeded 60 seconds");
}

#[tokio::test]
async fn closing_one_viewer_does_not_stop_the_shared_product() {
    let (service, adapter, _db) = service().await;
    let actor = PrincipalId::parse("alice").unwrap();
    let first = service
        .open(owner(), audit_context(actor.clone()), request("browser-a"))
        .await
        .unwrap();
    let second = service
        .open(owner(), audit_context(actor.clone()), request("browser-b"))
        .await
        .unwrap();
    service
        .close(
            &owner(),
            &audit_context(actor.clone()),
            CloseLiveViewRequest {
                session_id: first.stream.session_id,
                live_view_id: first.stream.live_view_id,
                viewer_instance_id: LiveViewerInstanceId::parse("browser-a").unwrap(),
            },
        )
        .await
        .unwrap();
    assert!(
        service
            .authorize_stream(
                &second.stream.live_view_id,
                second.access_token.expose_for_stream(),
            )
            .await
            .is_ok()
    );
    assert_eq!(
        adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
        1
    );
}

#[tokio::test]
async fn stream_disconnect_releases_connection_state_immediately() {
    let (service, adapter, _db) = service().await;
    let actor = PrincipalId::parse("alice").unwrap();
    let opened = service
        .open(owner(), audit_context(actor), request("browser-a"))
        .await
        .unwrap();
    service
        .authorize_stream(
            &opened.stream.live_view_id,
            opened.access_token.expose_for_stream(),
        )
        .await
        .unwrap();
    let mut connected = adapter.state().await.unwrap();
    service.project_product_usage(&mut connected).await.unwrap();
    assert_eq!(connected.stream_products[0].connected_viewers, 1);
    service.finish_stream(&opened.stream.live_view_id, 1).await;
    let mut disconnected = adapter.state().await.unwrap();
    service
        .project_product_usage(&mut disconnected)
        .await
        .unwrap();
    assert_eq!(disconnected.stream_products[0].connected_viewers, 0);
    assert_eq!(disconnected.stream_products[0].active_viewers, 1);
}

#[tokio::test]
async fn unavailable_open_and_rejected_renew_prove_only_the_correlated_request() {
    use super::super::service::live_view_non_admission_error;
    use crate::contract::{
        LiveStreamProductLifecycle, LiveViewNonAdmission, LiveViewNonAdmissionOperation,
    };
    let (service, adapter, _db) = service().await;
    let actor = PrincipalId::parse("alice").unwrap();
    let opening = request("browser-a");
    let mut failed = adapter.state().await.unwrap();
    for product in &mut failed.stream_products {
        product.lifecycle = LiveStreamProductLifecycle::Failed;
    }
    let Adapter::Fake(fake) = adapter.as_ref() else {
        panic!("expected fake adapter");
    };
    *fake.lock().await = FakeAdapter::new(failed);
    let error = service
        .open(owner(), audit_context(actor.clone()), opening.clone())
        .await
        .unwrap_err();
    assert!(service.state.lock().await.sessions.is_empty());
    let error = live_view_non_admission_error(
        error,
        LiveViewNonAdmissionOperation::Open {
            request: opening.clone(),
        },
    );
    let marker: LiveViewNonAdmission = serde_json::from_value(error.data.unwrap()).unwrap();
    assert_eq!(
        marker.operation,
        LiveViewNonAdmissionOperation::Open { request: opening }
    );

    // A different existing authorization survives a rejected renewal unchanged.
    *fake.lock().await = FakeAdapter::new(fake_state().unwrap());
    let opened = service
        .open(owner(), audit_context(actor.clone()), request("browser-a"))
        .await
        .unwrap();
    let before = service.state.lock().await.sessions[&opened.stream.live_view_id].clone();
    let renewing = RenewLiveViewRequest {
        session_id: opened.stream.session_id.clone(),
        live_view_id: opened.stream.live_view_id.clone(),
        viewer_instance_id: LiveViewerInstanceId::parse("wrong-browser").unwrap(),
    };
    let error = service
        .renew(&owner(), &audit_context(actor.clone()), renewing.clone())
        .await
        .unwrap_err();
    let after = service.state.lock().await.sessions[&opened.stream.live_view_id].clone();
    assert_eq!(before.generation, after.generation);
    assert_eq!(before.token_hash, after.token_hash);
    assert_eq!(before.state.expires_at, after.state.expires_at);
    let marker: LiveViewNonAdmission = serde_json::from_value(
        live_view_non_admission_error(
            error,
            LiveViewNonAdmissionOperation::Renew {
                request: renewing.clone(),
            },
        )
        .data
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        marker.operation,
        LiveViewNonAdmissionOperation::Renew { request: renewing }
    );
    service
        .authorize_stream(
            &opened.stream.live_view_id,
            opened.access_token.expose_for_stream(),
        )
        .await
        .unwrap();

    let mut revoked_owner = owner();
    revoked_owner.policy_revision = PolicyVersion::parse("policy-2").unwrap();
    let renewing = RenewLiveViewRequest {
        session_id: opened.stream.session_id,
        live_view_id: opened.stream.live_view_id.clone(),
        viewer_instance_id: LiveViewerInstanceId::parse("browser-a").unwrap(),
    };
    let mut revoked_audit = audit_context(actor);
    revoked_audit.authority.policy_revision = Some(revoked_owner.policy_revision.clone());
    let error = service
        .renew(&revoked_owner, &revoked_audit, renewing.clone())
        .await
        .unwrap_err();
    assert!(matches!(error, LiveViewError::AuthorityRevoked));
    assert!(
        live_view_non_admission_error(
            error,
            LiveViewNonAdmissionOperation::Renew { request: renewing }
        )
        .data
        .is_none()
    );
    service.finish_stream(&opened.stream.live_view_id, 1).await;
    assert!(
        service
            .authorize_stream(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_stream()
            )
            .await
            .is_err()
    );
}

#[test]
fn runtime_readiness_is_marked_but_uncertain_errors_are_not() {
    use super::super::service::live_view_non_admission_error;
    use crate::contract::LiveViewNonAdmissionOperation;
    let operation = LiveViewNonAdmissionOperation::Open {
        request: request("browser-a"),
    };
    assert!(
        live_view_non_admission_error(
            LiveViewError::Runtime("runtime unavailable".into()),
            operation.clone()
        )
        .data
        .is_some()
    );
    for error in [
        LiveViewError::Audit,
        LiveViewError::Contract,
        LiveViewError::Access,
        LiveViewError::AuthorityRevoked,
    ] {
        assert!(
            live_view_non_admission_error(error, operation.clone())
                .data
                .is_none()
        );
    }
}

#[tokio::test]
async fn unavailable_runtime_open_returns_correlated_non_admission() {
    use crate::adapter::HttpAdapter;
    use crate::contract::{LiveViewNonAdmission, LiveViewNonAdmissionOperation};
    tokio::time::timeout(Duration::from_secs(20), async {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let db = super::super::test_support::database(
            super::super::test_support::fixture::StoreBackend::Memory,
        )
        .await;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let adapter = Arc::new(Adapter::Http(Box::new(
            HttpAdapter::new(
                format!("http://{address}/").parse().unwrap(),
                Duration::from_secs(1),
                Duration::from_secs(1),
                "native-fixture".into(),
                db.a.clone(),
                "readiness-test",
            )
            .unwrap(),
        )));
        let service = LiveViewService::new_for_test(
            db.a.clone(),
            adapter,
            LiveViewConfig {
                session_duration: Duration::from_secs(30),
                public_stream_url: "wss://example.test/uav-sim/live".into(),
                maximum_frame_age_ms: 2_000,
            },
        )
        .await
        .unwrap();
        let request = request("browser-a");
        let error = service
            .open(
                owner(),
                audit_context(PrincipalId::parse("alice").unwrap()),
                request.clone(),
            )
            .await
            .unwrap_err();
        assert!(matches!(error, LiveViewError::Runtime(_)));
        assert!(service.state.lock().await.sessions.is_empty());
        let response = super::super::service::live_view_non_admission_error(
            error,
            LiveViewNonAdmissionOperation::Open {
                request: request.clone(),
            },
        );
        let marker: LiveViewNonAdmission = serde_json::from_value(response.data.unwrap()).unwrap();
        assert_eq!(
            marker.operation,
            LiveViewNonAdmissionOperation::Open { request }
        );
    })
    .await
    .expect("runtime readiness refusal exceeded 20 seconds");
}

#[tokio::test]
async fn persisted_grants_close_across_issuers_and_replay_after_shutdown() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (first, adapter, db) = service().await;
        let replacement =
            LiveViewService::new_for_test(db.b.clone(), adapter.clone(), first.config.clone())
                .await
                .unwrap();
        let actor = PrincipalId::parse("alice").unwrap();
        let opened = first
            .open(owner(), audit_context(actor.clone()), request("browser-a"))
            .await
            .unwrap();
        let mut authorization = first
            .authorize_stream(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_stream(),
            )
            .await
            .unwrap();
        assert!(
            replacement
                .authorize_stream(
                    &opened.stream.live_view_id,
                    opened.access_token.expose_for_stream()
                )
                .await
                .is_err()
        );
        let renewal = RenewLiveViewRequest {
            session_id: opened.stream.session_id.clone(),
            live_view_id: opened.stream.live_view_id.clone(),
            viewer_instance_id: opened.stream.viewer_instance_id.clone(),
        };
        assert!(
            replacement
                .renew(&owner(), &audit_context(actor.clone()), renewal)
                .await
                .is_err()
        );
        let closing = CloseLiveViewRequest {
            session_id: opened.stream.session_id.clone(),
            live_view_id: opened.stream.live_view_id.clone(),
            viewer_instance_id: opened.stream.viewer_instance_id.clone(),
        };
        let closed = replacement
            .close(&owner(), &audit_context(actor.clone()), closing.clone())
            .await
            .unwrap();
        assert!(closed.closed);
        assert_eq!(closed.resource_uri, opened.stream.resource_uri.as_str());
        tokio::time::timeout(Duration::from_secs(5), async {
            while authorization.events.borrow().active {
                authorization.events.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        let before = audit_counts(&db.a).await;
        let replay = replacement
            .close(&owner(), &audit_context(actor.clone()), closing)
            .await
            .unwrap();
        assert_eq!(replay.resource_uri, closed.resource_uri);
        assert!(replay.closed);
        let after = audit_counts(&db.a).await;
        assert_eq!(before, after);
        assert_eq!(before, vec![1]);

        assert_eq!(
            replacement
                .get(&owner(), &actor, &opened.stream.live_view_id)
                .await
                .unwrap()
                .lifecycle,
            LiveViewLifecycle::Closed
        );
        first.shutdown().await.unwrap();
        let second = replacement
            .open(owner(), audit_context(actor.clone()), request("browser-b"))
            .await
            .unwrap();
        replacement.shutdown().await.unwrap();
        let third =
            LiveViewService::new_for_test(db.a.clone(), adapter.clone(), first.config.clone())
                .await
                .unwrap();
        assert!(
            third
                .close(
                    &owner(),
                    &audit_context(actor),
                    CloseLiveViewRequest {
                        session_id: second.stream.session_id,
                        live_view_id: second.stream.live_view_id,
                        viewer_instance_id: second.stream.viewer_instance_id
                    }
                )
                .await
                .unwrap()
                .closed
        );
        assert_eq!(
            adapter.state().await.unwrap().stream_products[0].nvenc_sessions,
            1
        );
        third.shutdown().await.unwrap();
    })
    .await
    .expect("cross-issuer close exceeded 60 seconds");
}

#[tokio::test]
async fn close_renew_race_and_stale_stream_finish_never_revive_or_clear_current_grant() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (first, adapter, db) = service().await;
        let second = LiveViewService::new_for_test(db.b.clone(), adapter, first.config.clone())
            .await
            .unwrap();
        let actor = PrincipalId::parse("alice").unwrap();
        let opened = first
            .open(owner(), audit_context(actor.clone()), request("browser-a"))
            .await
            .unwrap();
        let original = first
            .grants
            .get(&opened.stream.live_view_id)
            .await
            .unwrap()
            .unwrap();
        let old = first
            .authorize_stream(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_stream(),
            )
            .await
            .unwrap();
        let renewing = RenewLiveViewRequest {
            session_id: opened.stream.session_id.clone(),
            live_view_id: opened.stream.live_view_id.clone(),
            viewer_instance_id: opened.stream.viewer_instance_id.clone(),
        };
        let renewed = first
            .renew(&owner(), &audit_context(actor.clone()), renewing.clone())
            .await
            .unwrap();
        let current = first
            .authorize_stream(
                &opened.stream.live_view_id,
                renewed.access_token.expose_for_stream(),
            )
            .await
            .unwrap();
        assert_ne!(old.generation, current.generation);
        first
            .finish_stream(&opened.stream.live_view_id, old.generation)
            .await;
        assert_eq!(
            first
                .grants
                .get(&opened.stream.live_view_id)
                .await
                .unwrap()
                .unwrap()
                .state
                .0
                .connected_viewers,
            1
        );
        // An expiry CAS retaining the old generation loses to the committed renewal.
        let mut expired = original.clone();
        expired.state.0.lifecycle = LiveViewLifecycle::Closed;
        expired.state.0.connected_viewers = 0;
        expired.token_hash.fill(0);
        expired.generation += 1;
        expired.closed_at = Some(Utc::now());
        let draft = LiveViewAudit::draft(
            &audit_context(actor.clone()),
            crate::uris::live_view(&opened.stream.session_id, &opened.stream.live_view_id),
            LiveViewActivity::Expire,
            AuditOutcome::Succeeded,
            AuditReason::Accepted,
        )
        .unwrap();
        assert!(
            !first
                .commit_revocation(&original, expired, draft)
                .await
                .unwrap()
        );
        assert!(current.events.borrow().active);
        assert_eq!(
            first.state.lock().await.sessions[&opened.stream.live_view_id]
                .state
                .connected_viewers,
            1
        );
        let closing = CloseLiveViewRequest {
            session_id: opened.stream.session_id.clone(),
            live_view_id: opened.stream.live_view_id.clone(),
            viewer_instance_id: opened.stream.viewer_instance_id.clone(),
        };
        let selected_owner = owner();
        let first_audit = audit_context(actor.clone());
        let second_audit = audit_context(actor.clone());
        let (renewed, closed) = tokio::join!(
            first.renew(&selected_owner, &first_audit, renewing),
            second.close(&selected_owner, &second_audit, closing)
        );
        let _ = renewed;
        assert!(closed.unwrap().closed);
        assert_eq!(audit_counts(&db.a).await, vec![1]);
        assert_eq!(
            first
                .grants
                .get(&opened.stream.live_view_id)
                .await
                .unwrap()
                .unwrap()
                .state
                .0
                .lifecycle,
            LiveViewLifecycle::Closed
        );
        assert!(
            first
                .authorize_stream(
                    &opened.stream.live_view_id,
                    opened.access_token.expose_for_stream()
                )
                .await
                .is_err()
        );
        first.shutdown().await.unwrap();
        second.shutdown().await.unwrap();
    })
    .await
    .expect("close/renew control exceeded 60 seconds");
}

#[tokio::test]
async fn foreign_stable_identity_cannot_read_close_or_rotate_persisted_grant() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (first, adapter, db) = service().await;
        let second = LiveViewService::new_for_test(db.b.clone(), adapter, first.config.clone())
            .await
            .unwrap();
        let actor = PrincipalId::parse("alice").unwrap();
        let opened = first
            .open(owner(), audit_context(actor.clone()), request("browser-a"))
            .await
            .unwrap();
        let original = first
            .grants
            .get(&opened.stream.live_view_id)
            .await
            .unwrap()
            .unwrap();
        let closing = CloseLiveViewRequest {
            session_id: opened.stream.session_id.clone(),
            live_view_id: opened.stream.live_view_id.clone(),
            viewer_instance_id: opened.stream.viewer_instance_id.clone(),
        };
        for kind in 0..4 {
            let mut selected_owner = owner();
            let mut audit = audit_context(actor.clone());
            let mut request = closing.clone();
            match kind {
                0 => {
                    selected_owner.work_context = WorkContextId::parse("foreign-context").unwrap();
                    audit.authority.work_context = Some(selected_owner.work_context.clone());
                }
                1 => audit.actor.principal = PrincipalId::parse("bob").unwrap(),
                2 => {
                    request.viewer_instance_id =
                        LiveViewerInstanceId::parse("foreign-browser").unwrap()
                }
                _ => request.session_id = LiveSessionId::parse("foreign-session").unwrap(),
            }
            assert!(
                second
                    .close(&selected_owner, &audit, request)
                    .await
                    .is_err()
            );
        }
        let mut foreign = owner();
        foreign.work_context = WorkContextId::parse("foreign-context").unwrap();
        assert!(
            second
                .get(&foreign, &actor, &opened.stream.live_view_id)
                .await
                .is_err()
        );
        assert!(
            second
                .page(&foreign, &actor, &opened.stream.session_id, None)
                .await
                .unwrap()
                .is_empty()
        );
        let retained = first
            .grants
            .get(&opened.stream.live_view_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(retained.generation, original.generation);
        assert_eq!(retained.token_hash, original.token_hash);
        assert!(retained.closed_at.is_none());
        // Mutable policy changes still permit revocation-only close of one's known grant.
        let mut changed = owner();
        changed.policy_revision = PolicyVersion::parse("policy-2").unwrap();
        let mut audit = audit_context(actor);
        audit.authority.policy_revision = Some(changed.policy_revision.clone());
        assert!(
            second
                .close(&changed, &audit, closing)
                .await
                .unwrap()
                .closed
        );
        first.shutdown().await.unwrap();
        second.shutdown().await.unwrap();
    })
    .await
    .expect("foreign-identity control exceeded 60 seconds");
}

#[tokio::test]
async fn observer_reread_failure_revokes_stream_and_stops_admission() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (service, _, db) = service().await;
        let opened = service
            .open(
                owner(),
                audit_context(PrincipalId::parse("alice").unwrap()),
                request("browser-a"),
            )
            .await
            .unwrap();
        let mut stream = service
            .authorize_stream(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_stream(),
            )
            .await
            .unwrap();
        // Schema permits the integer, but the checked owner DTO rejects this invalid current row.
        db.a.client()
            .query(
                "UPDATE uav_live_view SET state.cameraRevision = 0 WHERE state.liveViewId = $view;",
            )
            .bind(("view", opened.stream.live_view_id.to_string()))
            .await
            .unwrap()
            .check()
            .unwrap();
        tokio::time::timeout(Duration::from_secs(5), service.closed())
            .await
            .unwrap();
        while stream.events.borrow().active {
            stream.events.changed().await.unwrap();
        }
        assert!(
            service
                .authorize_stream(
                    &opened.stream.live_view_id,
                    opened.access_token.expose_for_stream()
                )
                .await
                .is_err()
        );
        assert!(
            service
                .open(
                    owner(),
                    audit_context(PrincipalId::parse("alice").unwrap()),
                    request("another")
                )
                .await
                .is_err()
        );
        assert!(service.shutdown().await.is_err()); // Invalid persisted outcomes stay unresolved.
    })
    .await
    .expect("observer failure control exceeded 60 seconds");
}

#[tokio::test]
async fn uncertain_audited_close_fail_closes_local_stream_without_false_receipt() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (service, _, db) = service().await;
        let actor = PrincipalId::parse("alice").unwrap();
        let opened = service
            .open(owner(), audit_context(actor.clone()), request("browser-a"))
            .await
            .unwrap();
        let stream = service
            .authorize_stream(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_stream(),
            )
            .await
            .unwrap();
        db.admin()
            .await
            .client()
            .query("REMOVE FUNCTION fn::append_audit;")
            .await
            .unwrap()
            .check()
            .unwrap();
        let error = service
            .close(
                &owner(),
                &audit_context(actor),
                CloseLiveViewRequest {
                    session_id: opened.stream.session_id,
                    live_view_id: opened.stream.live_view_id.clone(),
                    viewer_instance_id: opened.stream.viewer_instance_id,
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(error, LiveViewError::Storage));
        assert!(!stream.events.borrow().active);
        assert!(service.stop.is_cancelled());
        // Transaction rollback left an active row: no closed:true was returned or inferred.
        assert!(
            service
                .grants
                .get(&opened.stream.live_view_id)
                .await
                .unwrap()
                .unwrap()
                .closed_at
                .is_none()
        );
        assert!(service.shutdown().await.is_err());
    })
    .await
    .expect("uncertain close control exceeded 60 seconds");
}

#[tokio::test]
async fn closed_receipts_prune_only_after_72_hours_and_last_expiry() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (service, _, _db) = service().await;
        let actor = PrincipalId::parse("alice").unwrap();
        let opened = service.open(owner(), audit_context(actor.clone()), request("browser-a")).await.unwrap();
        service.close(&owner(), &audit_context(actor), CloseLiveViewRequest {
            session_id: opened.stream.session_id, live_view_id: opened.stream.live_view_id.clone(), viewer_instance_id: opened.stream.viewer_instance_id,
        }).await.unwrap();
        let now = Utc::now();
        // A closed receipt older than 72h still survives its future last token expiry.
        service.grants.store.client().query("UPDATE uav_live_view SET closed_at = $closed WHERE state.liveViewId = $view;")
            .bind(("closed",now-chrono::TimeDelta::hours(73))).bind(("view",opened.stream.live_view_id.to_string())).await.unwrap().check().unwrap();
        service.grants.prune().await.unwrap();
        assert!(service.grants.get(&opened.stream.live_view_id).await.unwrap().is_some());
        // Backdate only this isolated fixture to qualify the two independent cleanup conditions.
        service.grants.store.client().query("UPDATE uav_live_view SET closed_at = $closed, expires_at = $expired, state.expiresAt = $expiry_text, state.createdAt = $created_text WHERE state.liveViewId = $view;")
            .bind(("closed",now)).bind(("expired",now-chrono::TimeDelta::hours(1)))
            .bind(("expiry_text",(now-chrono::TimeDelta::hours(1)).to_rfc3339()))
            .bind(("created_text",(now-chrono::TimeDelta::hours(2)).to_rfc3339()))
            .bind(("view",opened.stream.live_view_id.to_string())).await.unwrap().check().unwrap();
        service.grants.prune().await.unwrap();
        assert!(service.grants.get(&opened.stream.live_view_id).await.unwrap().is_some());
        service.grants.store.client().query("UPDATE uav_live_view SET closed_at = $closed WHERE state.liveViewId = $view;")
            .bind(("closed",now-chrono::TimeDelta::hours(73))).bind(("view",opened.stream.live_view_id.to_string())).await.unwrap().check().unwrap();
        service.grants.prune().await.unwrap();
        assert!(service.grants.get(&opened.stream.live_view_id).await.unwrap().is_none());
        service.shutdown().await.unwrap();
    }).await.expect("closed receipt retention control exceeded 60 seconds");
}

#[tokio::test]
async fn observer_snapshot_serializes_with_renewal_commit() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (service, _, _db) = service().await;
        let actor = PrincipalId::parse("alice").unwrap();
        let opened = service
            .open(owner(), audit_context(actor.clone()), request("browser-a"))
            .await
            .unwrap();
        let (snapshot_ready, snapshot_seen) = tokio::sync::oneshot::channel();
        let (release_snapshot, snapshot_released) = tokio::sync::oneshot::channel();
        let observer = {
            let service = service.clone();
            tokio::spawn(async move {
                observer::reconcile_current(
                    &service.state,
                    &service.grants,
                    &service.issuer,
                    async {
                        snapshot_ready.send(()).unwrap();
                        snapshot_released.await.unwrap();
                    },
                )
                .await
                .unwrap();
            })
        };
        snapshot_seen.await.unwrap();
        // The snapshot must retain the same commit lock until it is applied.
        // Reading before acquiring that lock lets this exact interleaving apply
        // an older generation after renewal has published its replacement.
        assert!(service.state.try_lock().is_err());
        let renewal = {
            let service = service.clone();
            let request = RenewLiveViewRequest {
                session_id: opened.stream.session_id,
                live_view_id: opened.stream.live_view_id.clone(),
                viewer_instance_id: opened.stream.viewer_instance_id,
            };
            tokio::spawn(async move {
                service
                    .renew(&owner(), &audit_context(actor), request)
                    .await
                    .unwrap()
            })
        };
        release_snapshot.send(()).unwrap();
        observer.await.unwrap();
        let renewed = renewal.await.unwrap();
        observer::reconcile_current(
            &service.state,
            &service.grants,
            &service.issuer,
            std::future::ready(()),
        )
        .await
        .unwrap();
        let local = service.state.lock().await;
        let session = local.sessions.get(&renewed.stream.live_view_id).unwrap();
        assert!(active(&session.state));
        assert_eq!(session.events.borrow().generation, session.generation);
        assert_eq!(session.generation, 2);
        drop(local);
        assert!(
            service
                .authorize_stream(
                    &renewed.stream.live_view_id,
                    renewed.access_token.expose_for_stream()
                )
                .await
                .is_ok()
        );
        service.shutdown().await.unwrap();
    })
    .await
    .expect("observer/renewal interleaving exceeded 60 seconds");
}

#[tokio::test]
async fn product_usage_counts_all_issuers_only_within_selected_session() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (first, adapter, db) = service().await;
        let second =
            LiveViewService::new_for_test(db.b.clone(), adapter.clone(), first.config.clone())
                .await
                .unwrap();
        let actor = PrincipalId::parse("alice").unwrap();
        let current = first
            .open(owner(), audit_context(actor.clone()), request("browser-a"))
            .await
            .unwrap();
        second
            .open(owner(), audit_context(actor), request("browser-b"))
            .await
            .unwrap();
        let mut foreign = first
            .grants
            .get(&current.stream.live_view_id)
            .await
            .unwrap()
            .unwrap();
        foreign.state.0.live_view_id = LiveViewId::parse("other-session-view").unwrap();
        foreign.state.0.session_id =
            crate::contract::LiveSessionId::parse("session-other").unwrap();
        foreign.state.0.resource_uri = LiveViewUri::new(
            crate::uris::live_view(&foreign.state.0.session_id, &foreign.state.0.live_view_id)
                .as_str(),
        )
        .unwrap();
        foreign.issuer = super::super::live_view_store::LiveViewIssuerId::new().as_uuid();
        first.grants.create(foreign).await.unwrap();
        let mut state = adapter.state().await.unwrap();
        first.project_product_usage(&mut state).await.unwrap();
        let product = state
            .stream_products
            .iter()
            .find(|p| p.stream_product_id == current.stream.stream_product_id)
            .unwrap();
        assert_eq!(product.active_viewers, 2);
        assert_eq!(product.connected_viewers, 0);
        assert_eq!(product.nvenc_sessions, 1);
        state.session_id = crate::contract::SessionId::parse("session-other").unwrap();
        first.project_product_usage(&mut state).await.unwrap();
        assert_eq!(
            state
                .stream_products
                .iter()
                .find(|p| p.stream_product_id == current.stream.stream_product_id)
                .unwrap()
                .active_viewers,
            1
        );
        first.shutdown().await.unwrap();
        second.shutdown().await.unwrap();
    })
    .await
    .expect("session-scoped usage control exceeded 60 seconds");
}

#[tokio::test]
async fn source_reestablishment_reconcile_revokes_streams_and_stops_host() {
    tokio::time::timeout(Duration::from_secs(60), async {
        let (service, _, _db) = service().await;
        let opened = service
            .open(
                owner(),
                audit_context(PrincipalId::parse("alice").unwrap()),
                request("browser-a"),
            )
            .await
            .unwrap();
        let stream = service
            .authorize_stream(
                &opened.stream.live_view_id,
                opened.access_token.expose_for_stream(),
            )
            .await
            .unwrap();
        use futures::FutureExt;
        let local = service.state.lock().await;
        let mut delivery = Box::pin(observer::apply_delivery(
            &service.state,
            &service.grants,
            &service.issuer,
            &service.stop,
            Some(Ok(veoveo_platform_store::ChangefeedDelivery::Reconcile {
                cursor: veoveo_platform_store::ChangefeedCursor::initial(),
            })),
        ));
        // Real issuance/renewal holds this lock across IO. A known source gap
        // must notify the host before the local signal drain can acquire it.
        assert!(delivery.as_mut().now_or_never().is_none());
        assert!(service.stop.is_cancelled());
        tokio::time::timeout(Duration::from_millis(250), service.closed())
            .await
            .unwrap();
        assert!(stream.events.borrow().active);
        drop(local);
        assert!(!delivery.await);
        assert!(!stream.events.borrow().active);
        assert!(service.stop.is_cancelled());
        assert!(
            service
                .authorize_stream(
                    &opened.stream.live_view_id,
                    opened.access_token.expose_for_stream()
                )
                .await
                .is_err()
        );
        service.shutdown().await.unwrap();
    })
    .await
    .expect("source gap failure control exceeded 60 seconds");
}

#[test]
fn only_definitive_transaction_abort_can_retry_the_same_close_transaction() {
    use super::super::live_view_store::retryable_transaction_abort;
    use surrealdb::types::{ConnectionError, Error, QueryError};
    assert!(retryable_transaction_abort(&Error::query(
        "aborted conflict".into(),
        QueryError::TransactionConflict
    )));
    assert!(!retryable_transaction_abort(&Error::thrown(
        "uav_live_view_cas_conflict".into()
    )));
    for uncertain in [
        Error::connection("transport lost".into(), ConnectionError::ConnectionFailed),
        Error::query(
            "timeout".into(),
            QueryError::TimedOut {
                duration: std::time::Duration::from_secs(1),
            },
        ),
        Error::query("not executed".into(), QueryError::NotExecuted),
        Error::query("cancelled".into(), QueryError::Cancelled),
        Error::query("Transaction conflict:".into(), None),
    ] {
        assert!(
            !retryable_transaction_abort(&uncertain),
            "uncertain outcome must remain an error"
        );
    }
}

#[tokio::test]
async fn exhausted_transaction_aborts_never_attempt_a_fourth_close() {
    use super::super::live_view_store::retry_revoke_transaction;
    use surrealdb::types::{Error, QueryError};
    let attempts = std::sync::atomic::AtomicUsize::new(0);
    let outcome = retry_revoke_transaction(|| {
        let attempt = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        async move {
            if attempt < 3 {
                Err(Error::query(
                    "aborted conflict".into(),
                    QueryError::TransactionConflict,
                ))
            } else {
                Ok(())
            }
        }
    })
    .await;
    assert!(matches!(
        outcome.unwrap_err().query_details(),
        Some(QueryError::TransactionConflict)
    ));
    assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 3);
}
