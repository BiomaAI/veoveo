use std::collections::BTreeSet;

use veoveo_mcp_contract::{
    AccessLevel, AccessSubject, InvocationAuthority, InvocationProvenance,
    WorkContextMembershipLevel as ContractMembership,
};
use veoveo_platform_store::{
    ArtifactGrantSubjectKind, GrantPermission, InvocationAuthorityRecord,
    InvocationMode as StoreInvocationMode, WorkContextInitialGrantRecord,
    WorkContextMembershipLevel as StoreMembership,
};

pub(crate) fn authority_record(authority: &InvocationAuthority) -> InvocationAuthorityRecord {
    let (invocation_mode, initiator_key, delegation_id) = match &authority.provenance {
        InvocationProvenance::Direct { initiator } => (
            StoreInvocationMode::Direct,
            Some(initiator.to_string()),
            None,
        ),
        InvocationProvenance::Delegated {
            initiator,
            delegation_id,
        } => (
            StoreInvocationMode::Delegated,
            Some(initiator.to_string()),
            Some(delegation_id.to_string()),
        ),
        InvocationProvenance::Automated => (StoreInvocationMode::Automated, None, None),
    };
    let (owner_kind, owner_key) = subject_record(&authority.output_policy.owner);
    InvocationAuthorityRecord {
        context_key: authority.work_context.to_string(),
        membership: membership_record(authority.membership),
        policy_revision: authority.policy_revision.to_string(),
        owner_kind,
        owner_key,
        initial_grants: authority
            .output_policy
            .initial_grants
            .iter()
            .map(|grant| {
                let (subject_kind, subject_key) = subject_record(&grant.subject);
                WorkContextInitialGrantRecord {
                    subject_kind,
                    subject_key,
                    permission: permission_record(grant.level),
                }
            })
            .collect(),
        classification: authority
            .output_policy
            .classification
            .as_ref()
            .map(ToString::to_string),
        data_labels: authority
            .output_policy
            .data_labels
            .iter()
            .map(ToString::to_string)
            .collect(),
        invocation_mode,
        initiator_key,
        delegation_id,
    }
}

pub(crate) fn governed_classification(
    authority: &InvocationAuthorityRecord,
    requested: &str,
) -> String {
    authority
        .classification
        .clone()
        .unwrap_or_else(|| requested.to_owned())
}

pub(crate) fn governed_labels(
    authority: &InvocationAuthorityRecord,
    requested: &[String],
) -> Vec<String> {
    authority
        .data_labels
        .iter()
        .chain(requested)
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn subject_record(subject: &AccessSubject) -> (ArtifactGrantSubjectKind, String) {
    match subject {
        AccessSubject::Principal(principal) => {
            (ArtifactGrantSubjectKind::Principal, principal.to_string())
        }
        AccessSubject::Group(group) => (ArtifactGrantSubjectKind::Group, group.to_string()),
    }
}

fn membership_record(level: ContractMembership) -> StoreMembership {
    match level {
        ContractMembership::Viewer => StoreMembership::Viewer,
        ContractMembership::Contributor => StoreMembership::Contributor,
        ContractMembership::Custodian => StoreMembership::Custodian,
        ContractMembership::Owner => StoreMembership::Owner,
    }
}

fn permission_record(level: AccessLevel) -> GrantPermission {
    match level {
        AccessLevel::Read => GrantPermission::Read,
        AccessLevel::Write => GrantPermission::Write,
        AccessLevel::Admin => GrantPermission::Admin,
    }
}
