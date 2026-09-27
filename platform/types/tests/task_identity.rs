use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;
use veoveo_types::TaskId;

// The previous Store representation, without its driver-only derive.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
struct PreviousTaskId(Uuid);

#[test]
fn native_task_identity_preserves_uuid_admission_and_canonical_serialization() {
    for canonical in [
        "0195dabe-7777-7abc-8def-0123456789ab",
        "550e8400-e29b-41d4-a716-446655440000",
        "00000000-0000-0000-0000-000000000000",
        "ffffffff-ffff-ffff-ffff-ffffffffffff",
    ] {
        let uuid: Uuid = canonical.parse().unwrap();
        for text in [
            canonical.to_owned(),
            canonical.to_uppercase(),
            uuid.simple().to_string(),
            uuid.braced().to_string(),
            uuid.urn().to_string(),
        ] {
            let id: TaskId = text.parse().unwrap();
            assert_eq!(id, TaskId::from_uuid(uuid));
            assert_eq!(id.to_string(), canonical);
            assert_eq!(Uuid::from(id), uuid);
            let previous = PreviousTaskId(uuid);
            let encoded = serde_json::to_string(&previous).unwrap();
            assert_eq!(serde_json::to_string(&id).unwrap(), encoded);
            assert_eq!(serde_json::from_str::<TaskId>(&encoded).unwrap(), id);
            assert_eq!(serde_json::from_value::<TaskId>(json!(text)).unwrap(), id);
            assert_eq!(
                serde_json::from_value::<PreviousTaskId>(json!(text)).unwrap(),
                previous
            );
        }
    }
}

#[test]
fn native_task_identity_preserves_invalid_input_rejection() {
    for text in [
        "",
        "task:0195dabe-7777-7abc-8def-0123456789ab",
        "0195dabe-7777-7abc-8def-0123456789ag",
        " 0195dabe-7777-7abc-8def-0123456789ab",
        "0195dabe-7777-7abc-8def-0123456789ab\n",
        "URN:UUID:0195dabe-7777-7abc-8def-0123456789ab",
        "{0195dabe77777abc8def0123456789ab}",
        "操作者",
    ] {
        assert!(text.parse::<TaskId>().is_err(), "accepted {text:?}");
        assert!(text.parse::<Uuid>().is_err());
        assert!(serde_json::from_value::<TaskId>(json!(text)).is_err());
        assert!(serde_json::from_value::<PreviousTaskId>(json!(text)).is_err());
    }
    // Preserve UUID's Serde behavior even for non-string deserializers.
    for value in [
        Value::Null,
        json!(7),
        json!({"id": "task"}),
        json!(vec![0; 16]),
    ] {
        let previous = serde_json::from_value::<PreviousTaskId>(value.clone());
        let current = serde_json::from_value::<TaskId>(value);
        assert_eq!(previous.is_ok(), current.is_ok());
        if let Ok(previous) = previous {
            assert_eq!(current.unwrap().as_uuid(), previous.0);
        }
    }
}

#[test]
fn new_native_tasks_use_uuid_v7() {
    let a = TaskId::new();
    let b = TaskId::default();
    assert_eq!(a.as_uuid().get_version_num(), 7);
    assert_eq!(b.as_uuid().get_version_num(), 7);
    assert_ne!(a, b);
}
