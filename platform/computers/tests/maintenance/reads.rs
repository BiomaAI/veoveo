use super::*;

#[tokio::test]
async fn maintenance_admits_actor_parent_provider_and_claim_before_private_state() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let (a, _, _, claim) = journal::queued(&db).await;
        let operation = a.maintenance_for_claim(&claim).await.unwrap();
        let id = operation.operation_id;
        let caller = &operation.actor;
        let row = RecordId::new("computer_maintenance", StoreUuid::from(id.as_uuid()));
        db.a.client().query("UPDATE ONLY $row SET execution_authority.request_context.access_token.expires_at = 42;")
            .bind(("row", row.clone())).await.unwrap().check().unwrap();
        assert!(matches!(a.maintenance(caller, id).await, Err(ComputerError::Unavailable)));
        assert!(matches!(a.maintenance(&owner("bob"), id).await, Err(ComputerError::NotFound)));
        let mut other_context = caller.clone();
        other_context.authority.work_context = veoveo_types::WorkContextId::new("another-context").unwrap();
        assert!(matches!(a.maintenance(&other_context, id).await, Err(ComputerError::NotFound)));
        let foreign = ComputersStore::new(db.b.clone(), "00000000-0000-7000-8000-000000000063".parse::<veoveo_computers::api::ProviderInstanceId>().unwrap(), veoveo_gateway_catalog::registry().expect("installed owner catalog recipe")).unwrap();
        assert!(matches!(foreign.maintenance(caller, id).await, Err(ComputerError::NotFound)));
        assert!(matches!(foreign.maintenance_for_claim(&claim).await, Err(ComputerError::StateConflict)));
        let mut denied = claim.clone();
        denied.snapshot.owner = owner("bob");
        assert!(matches!(a.maintenance_for_claim(&denied).await, Err(ComputerError::StateConflict)));
        let mut denied = claim.clone();
        denied.snapshot.request["computerId"] = veoveo_computers_contract::ComputerId::new().to_string().into();
        assert!(matches!(a.maintenance_for_claim(&denied).await, Err(ComputerError::StateConflict)));
        let mut denied = claim.clone();
        denied.snapshot.request["unexpected"] = true.into();
        assert!(matches!(a.maintenance_for_claim(&denied).await, Err(ComputerError::StateConflict)));
        assert!(matches!(a.maintenance_for_claim(&claim).await, Err(ComputerError::Unavailable)));
        // The operation still names this actor, but the parent needs higher clearance.
        db.a.client().query("UPDATE ONLY $computer SET owner_context.data_labels = ['private'];")
            .bind(("computer", RecordId::new("computer", StoreUuid::from(operation.computer_id.into_uuid()))))
            .await.unwrap().check().unwrap();
        assert!(matches!(a.maintenance(caller, id).await, Err(ComputerError::NotFound)));
        assert!(matches!(a.maintenance_for_request(caller, operation.computer_id, operation.request_id).await, Err(ComputerError::NotFound)));
    }).await.expect("maintenance read admission exceeded 90 seconds");
}

#[tokio::test]
async fn accepted_resume_receipts_require_parent_access_before_input_decoding() {
    tokio::time::timeout(Duration::from_secs(90), async {
        let db = TestDb::new().await;
        let (a, _, tasks, claim) = journal::queued(&db).await;
        let _ticket = a.begin_maintenance_step(&claim).await.unwrap();
        let paused = a.pause_maintenance(&claim, veoveo_computers::maintenance::MaintenanceRecovery::AuthorityDenied).await.unwrap();
        tasks.release_observation(&claim).await.unwrap();
        let actor = support::authenticated(&paused.actor);
        let input = resume::input(&paused);
        assert!(a.maintenance_resume_for_request(&actor, &input).await.unwrap().is_none());
        let resumed = a.resume_maintenance(&actor, &input).await.unwrap();
        db.a.client().query("UPDATE computer_maintenance_resume SET input.expected_updated_at = 42 WHERE operation_id = $operation;")
            .bind(("operation", paused.operation_id.as_uuid())).await.unwrap().check().unwrap();
        assert!(matches!(a.maintenance_resume_for_request(&actor, &input).await, Err(ComputerError::Unavailable)));
        db.a.client().query("UPDATE ONLY $computer SET owner_context.data_labels = ['private'];")
            .bind(("computer", RecordId::new("computer", StoreUuid::from(input.computer_id.into_uuid()))))
            .await.unwrap().check().unwrap();
        assert!(matches!(a.maintenance_resume_for_request(&actor, &input).await, Err(ComputerError::NotFound)));
        let retained = a.maintenance_for_claim(&claim).await.unwrap();
        assert_eq!(retained.updated_at, resumed.updated_at);
        assert_eq!(retained.target_instance_id, resumed.target_instance_id);
    }).await.expect("maintenance receipt admission exceeded 90 seconds");
}
