//! OCSF 1.9.0 mapping. Required attributes select the class; unavailable facts
//! stay unavailable. `unmapped.veoveo` preserves the complete signed source record.
use serde::Serialize;
use std::net::IpAddr;
use veoveo_audit_contract::*;
use veoveo_types::{PrincipalId, TenantId};

#[derive(Serialize)]
pub struct Event<'a> {
    category_uid: u16,
    class_uid: u16,
    activity_id: u8,
    activity_name: &'static str,
    type_uid: u32,
    severity_id: u8,
    status_id: u8,
    status: &'static str,
    time: i64,
    metadata: Metadata<'a>,
    #[serde(flatten)]
    fields: Fields<'a>,
    unmapped: Source<'a>,
}
#[derive(Serialize)]
struct Metadata<'a> {
    version: &'static str,
    product: Product,
    uid: AuditRecordId,
    original_event_uid: AuditRecordId,
    correlation_uid: AuditRequestId,
    #[serde(skip_serializing_if = "Option::is_none")]
    tenant_uid: Option<&'a TenantId>,
    processed_time: i64,
}
#[derive(Serialize)]
struct Product {
    name: &'static str,
    vendor_name: &'static str,
}
#[derive(Serialize)]
struct Source<'a> {
    veoveo: &'a AuditRecord,
}
#[derive(Serialize)]
#[serde(untagged)]
enum Fields<'a> {
    Api {
        actor: Actor<'a>,
        api: Api,
        src_endpoint: Endpoint,
    },
    Authentication {
        user: User<'a>,
        service: Service,
    },
    Account {
        user: User<'a>,
    },
    File {
        actor: Actor<'a>,
        file: File,
        src_endpoint: Endpoint,
    },
    Base {},
}
#[derive(Serialize)]
struct Actor<'a> {
    user: User<'a>,
}
#[derive(Serialize)]
struct User<'a> {
    uid: &'a PrincipalId,
    type_id: u8,
}
#[derive(Serialize)]
struct Api {
    operation: &'static str,
}
#[derive(Serialize)]
struct Service {
    name: &'static str,
}
#[derive(Serialize)]
struct Endpoint {
    ip: IpAddr,
}
#[derive(Serialize)]
struct File {
    name: String,
    uid: String,
    type_id: u8,
}
impl<'a> User<'a> {
    fn actor(actor: &'a AuditActor) -> Self {
        Self {
            uid: &actor.principal,
            type_id: match actor.kind {
                AuditPrincipalKind::User => 1,
                AuditPrincipalKind::Service => 4,
                AuditPrincipalKind::Capability => 0,
            },
        }
    }
}
impl<'a> Event<'a> {
    pub fn from_record(record: &'a AuditRecord) -> Self {
        let draft = &record.draft;
        let (class_uid, activity_id, activity_name, fields) = classify(record);
        let (status_id, status) = match draft.outcome() {
            AuditOutcome::Succeeded => (1, "Success"),
            AuditOutcome::Failed | AuditOutcome::Denied => (2, "Failure"),
            // Admission does not assert that an operation has completed.
            AuditOutcome::Allowed => (99, "Allowed"),
        };
        Self {
            category_uid: class_uid / 1000,
            class_uid,
            activity_id,
            activity_name,
            type_uid: u32::from(class_uid) * 100 + u32::from(activity_id),
            severity_id: 1,
            status_id,
            status,
            time: draft.occurred_at().timestamp_millis(),
            metadata: Metadata {
                version: "1.9.0",
                product: Product {
                    name: "Veoveo",
                    vendor_name: "Bioma",
                },
                uid: draft.id(),
                original_event_uid: draft.id(),
                correlation_uid: draft.request().id,
                tenant_uid: draft.actor().and_then(|actor| actor.tenant.as_ref()),
                processed_time: record.recorded_at.timestamp_millis(),
            },
            fields,
            unmapped: Source { veoveo: record },
        }
    }
}
fn classify(record: &AuditRecord) -> (u16, u8, &'static str, Fields<'_>) {
    let draft = &record.draft;
    if let (AuditDetail::Authentication { activity, .. }, Some(actor)) =
        (draft.detail(), draft.actor())
    {
        let (id, name) = match activity {
            AuthenticationActivity::Login => (1, "Logon"),
            AuthenticationActivity::Logout => (2, "Logoff"),
            _ => (99, draft.detail().activity()),
        };
        return (
            3002,
            id,
            name,
            Fields::Authentication {
                user: User::actor(actor),
                service: Service {
                    name: "veoveo-gateway",
                },
            },
        );
    }
    if let (AuditDetail::AccountChange { activity }, AuditTarget::Principal { principal, .. }) =
        (draft.detail(), draft.target())
    {
        let (id, name) = match activity {
            AccountActivity::Create => (1, "Create"),
            AccountActivity::Delete => (6, "Delete"),
            _ => (99, draft.detail().activity()),
        };
        return (
            3001,
            id,
            name,
            Fields::Account {
                user: User {
                    uid: principal,
                    type_id: 0,
                },
            },
        );
    }
    if let (Some(actor), Some(ip)) = (draft.actor(), draft.request().source_ip) {
        let actor = Actor {
            user: User::actor(actor),
        };
        let src_endpoint = Endpoint { ip };
        if let (AuditDetail::Artifact { activity, .. }, AuditTarget::Artifact { artifact }) =
            (draft.detail(), draft.target())
        {
            let (id, name) = match activity {
                ArtifactActivity::Publish => (1, "Upload"),
                ArtifactActivity::Download => (2, "Download"),
                ArtifactActivity::Delete => (4, "Delete"),
                ArtifactActivity::Share => (12, "Share"),
                ArtifactActivity::Unshare => (13, "Unshare"),
                _ => (99, draft.detail().activity()),
            };
            return (
                6006,
                id,
                name,
                Fields::File {
                    actor,
                    src_endpoint,
                    file: File {
                        name: artifact.to_string(),
                        uid: artifact.to_string(),
                        type_id: 0,
                    },
                },
            );
        }
        let (id, name) = match draft.detail() {
            AuditDetail::Read { .. }
            | AuditDetail::Discovery { .. }
            | AuditDetail::KnowledgeRead { .. }
            | AuditDetail::IndexingWindow { .. } => (2, "Read"),
            AuditDetail::AccountChange {
                activity: AccountActivity::Create,
            } => (1, "Create"),
            AuditDetail::AccountChange {
                activity: AccountActivity::Delete,
            } => (4, "Delete"),
            AuditDetail::AccountChange { .. } => (3, "Update"),
            _ => (99, draft.detail().activity()),
        };
        return (
            6003,
            id,
            name,
            Fields::Api {
                actor,
                src_endpoint,
                api: Api {
                    operation: draft.detail().activity(),
                },
            },
        );
    }
    (0, 99, draft.detail().activity(), Fields::Base {})
}
