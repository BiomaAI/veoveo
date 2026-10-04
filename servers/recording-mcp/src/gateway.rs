//! Recording-specific admission for the gateway's producer transport adapter.

#[cfg(test)]
mod tests {
    use super::test_store;
    use crate::contract::{
        RECORDING_INGEST_SECTION, RecordingCatalog, RecordingCatalogSection, RecordingProducerScope,
    };
    use std::collections::BTreeSet;
    use veoveo_mcp_contract::GatewayControlPlane;

    use veoveo_mcp_gateway::{GatewayCatalog, GatewayCatalogAdmission};
    use veoveo_types::ScopeName;

    fn section(plane: &GatewayControlPlane) -> RecordingCatalogSection {
        serde_json::from_value(plane.extensions[RECORDING_INGEST_SECTION].clone()).unwrap()
    }
    fn mutate_section(
        plane: &mut GatewayControlPlane,
        change: impl FnOnce(&mut RecordingCatalogSection),
    ) {
        let mut section = section(plane);
        change(&mut section);
        plane.extensions.insert(
            RECORDING_INGEST_SECTION.to_owned(),
            serde_json::to_value(section).unwrap(),
        );
    }
    fn admission() -> GatewayCatalogAdmission {
        GatewayCatalogAdmission::unbound()
            .bind(veoveo_gateway_catalog::registry().unwrap())
            .unwrap()
    }
    #[tokio::test]
    async fn producer_scope_denial_cannot_publish_a_control_revision() {
        use crate::contract::RecordingProducerScope;
        use std::time::Duration;
        use veoveo_audit_contract::{AuditActor, AuditContext, AuditPrincipalKind, AuditRequest};
        use veoveo_mcp_contract::{GatewayControlPlaneRevision, GatewayControlPlaneRevisionSource};
        use veoveo_mcp_gateway::{GatewayControlStore, new_gateway_control_plane_revision_id};
        use veoveo_types::PrincipalId;

        let db = test_store::TestDb::new().await;
        let store = GatewayControlStore::from_platform_store(db.a.clone(), admission());
        let mut control_plane: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
        mutate_section(&mut control_plane, |section| {
            section.0[0].required_scopes = BTreeSet::from([RecordingProducerScope::Publish.into()])
        });
        for client in &mut control_plane.oauth_clients {
            client
                .allowed_scopes
                .insert(RecordingProducerScope::Publish.into());
        }

        let revision = GatewayControlPlaneRevision {
            revision_id: new_gateway_control_plane_revision_id().unwrap(),
            sha256: "a".repeat(64),
            source: GatewayControlPlaneRevisionSource::SeedFile,
            applied_at: chrono::Utc::now(),
            applied_by: PrincipalId::parse("fixture-admin").unwrap(),
            tenant: None,
            control_plane,
        };
        let context = AuditContext {
            actor: AuditActor {
                principal: revision.applied_by.clone(),
                kind: AuditPrincipalKind::Service,
                tenant: None,
                oauth_client: None,
                session_family: None,
                delegating_principal: None,
                managed_agent: None,
            },
            authority: Default::default(),
            request: AuditRequest::background(),
        };
        tokio::time::timeout(Duration::from_secs(30), async {
            let error = store
                .record_revision(&revision, &context)
                .await
                .unwrap_err();
            let diagnostic = format!("{error:#}");
            assert!(
                diagnostic.contains("recording:ingest"),
                "publication must reject missing ingest permission: {diagnostic}"
            );
            assert_eq!(store.revision_count().await.unwrap(), 0);
            assert!(store.load_active_revision_head().await.unwrap().is_none());
            let content = veoveo_platform_store::GatewayControlRevisionContent {
                revision_id: revision.revision_id.to_string(),
                sha256: revision.sha256.clone(),
                source: veoveo_platform_store::GatewayControlRevisionSource::SeedFile,
                applied_at: revision.applied_at,
                applied_by: revision.applied_by.to_string(),
                tenant: None,
                control_plane: veoveo_platform_store::OpenObject::new(
                    serde_json::from_value(serde_json::to_value(&revision.control_plane).unwrap())
                        .unwrap(),
                ),
            };
            let record = veoveo_platform_store::RecordId::new(
                "gateway_control_revision",
                revision.revision_id.as_str(),
            );
            db.a.client()
                .query(include_str!("../tests/fixtures/gateway_revision.surql"))
                .bind(("record", record))
                .bind(("content", content))
                .bind(("revision_id", revision.revision_id.to_string()))
                .await
                .unwrap()
                .check()
                .unwrap();
            assert!(
                store.load_active_revision().await.is_err(),
                "stored scope denial bypassed admission on reload"
            );
            let unbound = GatewayControlStore::from_platform_store(
                db.a.clone(),
                GatewayCatalogAdmission::unbound(),
            );
            assert!(
                unbound.load_active_revision().await.is_err(),
                "unbound persisted decode admitted a revision"
            );
            assert!(
                unbound.record_revision(&revision, &context).await.is_err(),
                "unbound publication admitted a revision"
            );
        })
        .await
        .expect("producer scope admission exceeded 30 seconds");
    }

    #[test]
    fn recording_adapter_requires_ingest_and_preserves_additional_scopes() {
        let mut configuration: GatewayControlPlane =
            serde_json::from_str(include_str!("../../../configs/gateway.smoke.json")).unwrap();
        assert_eq!(section(&configuration).0.len(), 1);
        let installation_scope = ScopeName::parse("installation:producer").unwrap();
        mutate_section(&mut configuration, |section| {
            section.0[0]
                .required_scopes
                .insert(installation_scope.clone());
        });
        for client in &mut configuration.oauth_clients {
            client.allowed_scopes.extend([
                installation_scope.clone(),
                RecordingProducerScope::Publish.into(),
            ]);
        }
        let catalog =
            GatewayCatalog::from_control_plane(configuration.clone(), admission()).unwrap();
        assert!(
            RecordingCatalog::from_admitted(catalog.registry(), catalog.sections())
                .unwrap()
                .single_resource()
                .unwrap()
                .required_scopes
                .contains(&installation_scope)
        );

        mutate_section(&mut configuration, |section| {
            section.0[0].required_scopes =
                BTreeSet::from([RecordingProducerScope::Publish.into(), installation_scope])
        });
        // Generic MCP configuration admits names; the installed transport adapter
        // enforces the producer protocol without a domain dependency in MCP core.

        let error = GatewayCatalog::from_control_plane(configuration, admission()).unwrap_err();
        assert!(error.to_string().contains("recording:ingest"));
    }
}

#[cfg(test)]
#[path = "../../../testing/fixtures/store.rs"]
mod test_store;

pub mod routes;
