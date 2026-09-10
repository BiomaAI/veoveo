use veoveo_task_runtime::TaskOwner;
#[path = "../../../../testing/fixtures/store.rs"]
mod store;
pub use store::TestDb;

pub const FINGERPRINT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

pub fn owner(subject: &str) -> TaskOwner {
    let principal = format!("https://computers.test#{subject}");
    serde_json::from_value(serde_json::json!({
        "principal_key": principal, "principal_kind": "user", "issuer": "https://computers.test",
        "subject": subject, "profile": "operator", "tenant_key": "test", "data_labels": [],
        "authority": {"work_context": "computers-test", "tenant": "test", "membership": "contributor",
            "policy_revision": "test-1", "output_policy": {"owner": {"kind": "principal", "id": principal}},
            "provenance": {"mode": "direct", "initiator": principal}}
    })).unwrap()
}
