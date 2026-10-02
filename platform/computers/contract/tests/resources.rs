use serde_json::json;
use veoveo_computers_contract::*;
use veoveo_types::{ResourceAddress, ScopeName};

#[test]
fn native_identity_admission_rejects_aliases_wrong_versions_and_variants() {
    let good = "019b7b88-7f03-7123-8123-abcdefabcdef";
    for invalid in [
        good.to_uppercase(),
        good.replace('-', ""),
        format!("urn:uuid:{good}"),
        format!("{good}\n"),
        "00000000-0000-0000-0000-000000000000".to_owned(),
        "019b7b88-7f03-4123-8123-abcdefabcdef".to_owned(),
        "019b7b88-7f03-7123-c123-abcdefabcdef".to_owned(),
    ] {
        assert!(invalid.parse::<ComputerId>().is_err());
        assert!(invalid.parse::<ExecutionId>().is_err());
        assert!(invalid.parse::<FileTransferId>().is_err());
        assert!(invalid.parse::<AutomationGrantId>().is_err());
        assert!(invalid.parse::<AccessGrantId>().is_err());
        assert!(invalid.parse::<CliPairingId>().is_err());
        assert!(invalid.parse::<AccessConnectionId>().is_err());
        assert!(serde_json::from_value::<ComputerId>(json!(invalid)).is_err());
        assert!(serde_json::from_value::<AccessGrantId>(json!(invalid)).is_err());
        assert!(serde_json::from_value::<CliPairingId>(json!(invalid)).is_err());
        assert!(serde_json::from_value::<AccessConnectionId>(json!(invalid)).is_err());
    }
    let computer: ComputerId = good.parse().unwrap();
    assert_eq!(serde_json::to_value(computer).unwrap(), json!(good));
    assert_eq!(
        serde_json::from_value::<ComputerId>(json!(good)).unwrap(),
        computer
    );
    assert_eq!(
        ComputerId::try_from(computer.into_uuid()).unwrap(),
        computer
    );
    let execution = ExecutionId::new();
    assert_eq!(execution.task_id().as_uuid(), execution.into_uuid());
    let transfer = FileTransferId::new();
    assert_eq!(transfer.task_id().as_uuid(), transfer.into_uuid());
    let grant = AccessGrantId::new();
    let pairing = CliPairingId::new();
    let connection = AccessConnectionId::new();
    assert_eq!(
        serde_json::from_value::<AccessGrantId>(json!(grant)).unwrap(),
        grant
    );
    assert_eq!(
        serde_json::from_value::<CliPairingId>(json!(pairing)).unwrap(),
        pairing
    );
    assert_eq!(
        serde_json::from_value::<AccessConnectionId>(json!(connection)).unwrap(),
        connection
    );
}

#[test]
fn complete_resource_vocabulary_preserves_typed_identity_and_wire_shape() {
    let computer = ComputerId::new();
    let grant = AutomationGrantId::new();
    let execution = ExecutionId::new();
    let transfer = FileTransferId::new();
    for resource in [
        ComputerResource::Collection(None),
        ComputerResource::Collection(Some(computer)),
        ComputerResource::Computer(computer),
        ComputerResource::Access(computer),
        ComputerResource::Maintenance(computer),
        ComputerResource::Automation(computer),
        ComputerResource::Grant { computer, grant },
        ComputerResource::Execution(execution),
        ComputerResource::Transfer(transfer),
        ComputerResource::Docs,
        ComputerResource::Document(ComputerDocument::Agents),
        ComputerResource::Document(ComputerDocument::Design),
        ComputerResource::Contract,
    ] {
        let uri = resource.to_uri();
        assert_eq!(ComputerResource::parse(uri.as_str()).unwrap(), resource);
        assert_eq!(
            <ComputerResource as ResourceAddress>::parse(&uri).unwrap(),
            resource
        );
        assert_eq!(
            serde_json::from_value::<ComputerResource>(json!(uri)).unwrap(),
            resource
        );
        assert_eq!(serde_json::to_value(resource).unwrap(), json!(uri));
    }
    assert_eq!(
        computer_uri(computer).as_str(),
        format!("computer://computers/{computer}")
    );
    assert_eq!(
        maintenance_uri(computer).as_str(),
        format!("computer://computers/{computer}/maintenance")
    );
    assert_eq!(
        automation_grant_uri(computer, grant).as_str(),
        format!("computer://computers/{computer}/automation/{grant}")
    );
    assert_eq!(
        String::from(ExecutionResultUri::new(execution)),
        format!("computer://executions/{execution}")
    );
    assert_eq!(
        String::from(FileTransferResultUri::new(transfer)),
        format!("computer://transfers/{transfer}")
    );
}

#[test]
fn resource_admission_rejects_wrong_families_queries_and_documents() {
    let id = ComputerId::new();
    for value in [
        format!("computer://computers/{id}/extra"),
        format!("computer://computers/{id}/access?after={id}"),
        format!("computer://computers?after={id}&after={id}"),
        format!("computer://computers?cursor={id}"),
        format!("computer://computers/{id}#fragment"),
        format!("other://computers/{id}"),
        "computer://computers/".into(),
        "computer://computers?".into(),
        "computer://docs/missing".into(),
        "computer://docs/%64esign".into(),
        "computer://docs/design/".into(),
        "computer://executions/not-an-id".into(),
    ] {
        assert!(ComputerResource::parse(&value).is_err(), "{value}");
    }
    assert!(
        ExecutionResultUri::try_from(String::from(FileTransferResultUri::new(
            FileTransferId::new()
        )))
        .is_err()
    );
    assert!(
        FileTransferResultUri::try_from(String::from(ExecutionResultUri::new(ExecutionId::new())))
            .is_err()
    );
    assert!(ComputerScope::try_from(&ScopeName::new("computers:read").unwrap()).is_err());
}
