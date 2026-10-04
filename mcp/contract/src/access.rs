//! Layered access-control primitives for the shared artifact plane.
//!
//! Veoveo access control is several models composed, not a single ACL (see
//! `docs/TECH_DESIGN.md`, "Access control model"). This module owns the pure,
//! I/O-free core:
//!
//! - **DAC / ACL** — [`AccessGrant`] scopes one artifact to one [`AccessSubject`] at one
//!   [`AccessLevel`]. This is the discretionary "share with those people" layer.
//! - **Groups** — an [`AccessSubject::Group`] grant plus the caller's
//!   [`GroupMembership`] set (the `(GroupId, GroupRole)` pairing) resolve to an
//!   effective level via `min(member role, grant level)`.
//! - **MAC** — [`mac_satisfied`] checks that the caller's clearance dominates
//!   the artifact's labels; it is evaluated independently and can never be
//!   widened by a grant.
//! - **Tenancy** — a hard partition checked before anything else.
//!
//! [`decide`] composes these into a single [`AccessDecision`] with a reason,
//! and is exhaustively unit-tested below because it is the security core.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use veoveo_artifact_contract::ArtifactId;
#[cfg(test)]
use veoveo_artifact_contract::Grant;
use veoveo_types::AccessGrant;

use veoveo_types::WorkContextMembershipLevel;
use veoveo_types::{AccessLevel, AccessSubject, DataLabelId, GroupId, PrincipalId, TenantId};

/// The role a principal holds *within* a group. It has the same wire values and
/// ordering as a grant level, but remains a distinct domain type so callers
/// cannot accidentally substitute one axis for the other.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum GroupRole {
    Read,
    Write,
    Admin,
}

impl GroupRole {
    fn access_level(self) -> AccessLevel {
        match self {
            Self::Read => AccessLevel::Read,
            Self::Write => AccessLevel::Write,
            Self::Admin => AccessLevel::Admin,
        }
    }
}

/// One `(GroupId, GroupRole)` membership. A principal carries a set of these;
/// the pairing is the only genuinely new relationship the sharing feature adds
/// over today's flat `Principal.groups`.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct GroupMembership {
    pub group: GroupId,
    pub role: GroupRole,
}

/// The outcome of an access decision, carrying the reason for audit evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum AccessDecision {
    Allow,
    /// Different tenant — the hard isolation boundary. Checked first.
    DenyTenant,
    /// Clearance (MAC) does not dominate the artifact's labels. This is the
    /// mandatory backstop; it is reported even when a grant would otherwise
    /// suffice, because no grant can override it.
    DenyClearance,
    /// No grant (directly or via a group) confers the requested level.
    DenyNeedToKnow,
}

impl AccessDecision {
    pub fn is_allowed(self) -> bool {
        matches!(self, AccessDecision::Allow)
    }
}

/// The caller's role in `group`, if they are a member.
pub fn role_in_group(
    memberships: &BTreeSet<GroupMembership>,
    group: &GroupId,
) -> Option<GroupRole> {
    memberships
        .iter()
        .find(|m| &m.group == group)
        .map(|m| m.role)
}

/// The discretionary level a single `grant` confers on this caller, before MAC.
///
/// - A `User` grant confers its level directly to the named principal.
/// - A `Group` grant confers `min(role in group, grant level)` — the meet of
///   the two independent caps.
pub fn grant_level_for_caller<G: AccessGrant>(
    grant: &G,
    caller_id: &PrincipalId,
    memberships: &BTreeSet<GroupMembership>,
) -> Option<AccessLevel> {
    match grant.subject() {
        AccessSubject::Principal(principal) if principal == caller_id => Some(grant.level()),
        AccessSubject::Principal(_) => None,
        AccessSubject::Group(group) => {
            role_in_group(memberships, group).map(|role| role.access_level().min(grant.level()))
        }
    }
}

/// MAC: the caller's clearance dominates the artifact's labels iff every label
/// on the artifact is also held by the caller.
pub fn mac_satisfied(
    resource_labels: &BTreeSet<DataLabelId>,
    caller_labels: &BTreeSet<DataLabelId>,
) -> bool {
    resource_labels.is_subset(caller_labels)
}

/// Everything an access decision needs. Borrowed so callers assemble it from
/// the ledger and the signed identity without cloning.
pub struct AccessRequest<'a, G: AccessGrant> {
    /// Evaluation instant supplied by the caller; expired grants confer no access.
    pub now: chrono::DateTime<chrono::Utc>,
    pub caller_id: &'a PrincipalId,
    pub caller_tenant: Option<&'a TenantId>,
    pub caller_labels: &'a BTreeSet<DataLabelId>,
    pub memberships: &'a BTreeSet<GroupMembership>,
    /// Tenant the artifact lives in.
    pub resource_tenant: &'a TenantId,
    /// Labels the artifact carries (classification unioned into data labels).
    pub resource_labels: &'a BTreeSet<DataLabelId>,
    /// Grants recorded for this artifact.
    pub grants: &'a [G],
    /// Matching Work Context membership, when the caller selected the same
    /// context stamped on the artifact.
    pub context_membership: Option<WorkContextMembershipLevel>,
    pub requested: AccessLevel,
}

/// Compose tenancy, DAC, and MAC into one decision.
///
/// Order encodes the invariants: tenant isolation is the hard boundary and is
/// checked first; then both need-to-know (DAC) *and* clearance (MAC) must hold,
/// with a MAC failure reported as `DenyClearance` even if a grant would suffice.
pub fn decide<G: AccessGrant>(req: &AccessRequest<'_, G>) -> AccessDecision {
    if req.caller_tenant != Some(req.resource_tenant) {
        return AccessDecision::DenyTenant;
    }

    let best_dac = req
        .grants
        .iter()
        .filter(|grant| grant.expires_at().is_none_or(|expires| expires > req.now))
        .filter_map(|grant| grant_level_for_caller(grant, req.caller_id, req.memberships))
        .chain(req.context_membership.map(|level| level.artifact_access()))
        .max();

    let mac = mac_satisfied(req.resource_labels, req.caller_labels);

    match best_dac {
        _ if !mac => AccessDecision::DenyClearance,
        Some(level) if level.allows(req.requested) => AccessDecision::Allow,
        _ => AccessDecision::DenyNeedToKnow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pid(s: &str) -> PrincipalId {
        PrincipalId::parse(s).unwrap()
    }
    fn gid(s: &str) -> GroupId {
        GroupId::parse(s).unwrap()
    }
    fn tid(s: &str) -> TenantId {
        TenantId::parse(s).unwrap()
    }
    fn lid(s: &str) -> DataLabelId {
        DataLabelId::parse(s).unwrap()
    }
    fn artifact_id() -> ArtifactId {
        ArtifactId::new()
    }

    fn member(group: &str, role: GroupRole) -> BTreeSet<GroupMembership> {
        let mut s = BTreeSet::new();
        s.insert(GroupMembership {
            group: gid(group),
            role,
        });
        s
    }

    fn user_grant(user: &str, level: AccessLevel) -> Grant {
        Grant {
            artifact: artifact_id(),
            subject: AccessSubject::Principal(pid(user)),
            level,
            tenant: tid("acme"),
            data_labels: BTreeSet::new(),
            retention_expires_at: None,
        }
    }

    fn group_grant(group: &str, level: AccessLevel, labels: BTreeSet<DataLabelId>) -> Grant {
        Grant {
            artifact: artifact_id(),
            subject: AccessSubject::Group(gid(group)),
            level,
            tenant: tid("acme"),
            data_labels: labels,
            retention_expires_at: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn request<'a>(
        caller: &'a PrincipalId,
        tenant: Option<&'a TenantId>,
        labels: &'a BTreeSet<DataLabelId>,
        memberships: &'a BTreeSet<GroupMembership>,
        resource_tenant: &'a TenantId,
        resource_labels: &'a BTreeSet<DataLabelId>,
        grants: &'a [Grant],
        requested: AccessLevel,
    ) -> AccessRequest<'a, Grant> {
        AccessRequest {
            now: chrono::Utc::now(),
            caller_id: caller,
            caller_tenant: tenant,
            caller_labels: labels,
            memberships,
            resource_tenant,
            resource_labels,
            grants,
            context_membership: None,
            requested,
        }
    }

    #[test]
    fn access_level_is_ordered_read_write_admin() {
        assert!(AccessLevel::Read < AccessLevel::Write);
        assert!(AccessLevel::Write < AccessLevel::Admin);
        assert_eq!(AccessLevel::Read.min(AccessLevel::Admin), AccessLevel::Read);
    }

    fn no_labels() -> BTreeSet<DataLabelId> {
        BTreeSet::new()
    }
    fn no_members() -> BTreeSet<GroupMembership> {
        BTreeSet::new()
    }

    #[test]
    fn user_grant_confers_its_level_and_no_more() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let nl = no_labels();
        let nm = no_members();
        let grants = [user_grant("alice", AccessLevel::Read)];

        let read = request(
            &caller,
            Some(&tenant),
            &nl,
            &nm,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&read), AccessDecision::Allow);

        let write = request(
            &caller,
            Some(&tenant),
            &nl,
            &nm,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Write,
        );
        assert_eq!(decide(&write), AccessDecision::DenyNeedToKnow);
    }

    #[test]
    fn user_grant_does_not_apply_to_a_different_principal() {
        let caller = pid("bob");
        let tenant = tid("acme");
        let nl = no_labels();
        let nm = no_members();
        let grants = [user_grant("alice", AccessLevel::Admin)];
        let req = request(
            &caller,
            Some(&tenant),
            &nl,
            &nm,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&req), AccessDecision::DenyNeedToKnow);
    }

    #[test]
    fn group_grant_is_capped_by_the_lesser_of_role_and_level() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let nl = no_labels();

        // member role write, grant level read -> effective read.
        let memberships = member("eng", GroupRole::Write);
        let grants = [group_grant("eng", AccessLevel::Read, BTreeSet::new())];
        let read = request(
            &caller,
            Some(&tenant),
            &nl,
            &memberships,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&read), AccessDecision::Allow);
        let write = request(
            &caller,
            Some(&tenant),
            &nl,
            &memberships,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Write,
        );
        assert_eq!(decide(&write), AccessDecision::DenyNeedToKnow);

        // member role read, grant level admin -> still only read.
        let memberships = member("eng", GroupRole::Read);
        let grants = [group_grant("eng", AccessLevel::Admin, BTreeSet::new())];
        let write = request(
            &caller,
            Some(&tenant),
            &nl,
            &memberships,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Write,
        );
        assert_eq!(decide(&write), AccessDecision::DenyNeedToKnow);
    }

    #[test]
    fn non_member_gets_nothing_from_a_group_grant() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let nl = no_labels();
        let nm = no_members();
        let grants = [group_grant("eng", AccessLevel::Admin, BTreeSet::new())];
        let req = request(
            &caller,
            Some(&tenant),
            &nl,
            &nm,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&req), AccessDecision::DenyNeedToKnow);
    }

    #[test]
    fn mac_backstop_denies_even_an_admin_grant_without_clearance() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let caller_labels = BTreeSet::new(); // no clearance
        let resource_labels: BTreeSet<_> = [lid("cui")].into_iter().collect();
        let memberships = member("eng", GroupRole::Admin);
        let grants = [group_grant(
            "eng",
            AccessLevel::Admin,
            resource_labels.clone(),
        )];
        let req = request(
            &caller,
            Some(&tenant),
            &caller_labels,
            &memberships,
            &tenant,
            &resource_labels,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&req), AccessDecision::DenyClearance);
    }

    #[test]
    fn cleared_caller_with_grant_and_labels_is_allowed() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let caller_labels: BTreeSet<_> = [lid("cui"), lid("us_only")].into_iter().collect();
        let resource_labels: BTreeSet<_> = [lid("cui")].into_iter().collect();
        let memberships = member("eng", GroupRole::Write);
        let grants = [group_grant(
            "eng",
            AccessLevel::Write,
            resource_labels.clone(),
        )];
        let req = request(
            &caller,
            Some(&tenant),
            &caller_labels,
            &memberships,
            &tenant,
            &resource_labels,
            &grants,
            AccessLevel::Write,
        );
        assert_eq!(decide(&req), AccessDecision::Allow);
    }

    #[test]
    fn different_tenant_is_denied_before_anything_else() {
        let caller = pid("alice");
        let caller_tenant = tid("evil");
        let resource_tenant = tid("acme");
        let nl = no_labels();
        let nm = no_members();
        // A grant that would otherwise allow, plus full clearance.
        let grants = [user_grant("alice", AccessLevel::Admin)];
        let req = request(
            &caller,
            Some(&caller_tenant),
            &nl,
            &nm,
            &resource_tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&req), AccessDecision::DenyTenant);
    }

    #[test]
    fn tenantless_caller_is_denied() {
        let caller = pid("alice");
        let resource_tenant = tid("acme");
        let nl = no_labels();
        let nm = no_members();
        let grants = [user_grant("alice", AccessLevel::Admin)];
        let req = request(
            &caller,
            None,
            &nl,
            &nm,
            &resource_tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&req), AccessDecision::DenyTenant);
    }

    #[test]
    fn expired_grants_stop_conferring_access_at_the_evaluation_instant() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let labels = no_labels();
        let memberships = member("operators", GroupRole::Admin);
        let now = chrono::Utc::now();
        for mut grant in [
            user_grant("alice", AccessLevel::Admin),
            group_grant("operators", AccessLevel::Admin, no_labels()),
        ] {
            for (expiry, expected) in [
                (
                    now - chrono::TimeDelta::seconds(1),
                    AccessDecision::DenyNeedToKnow,
                ),
                (now, AccessDecision::DenyNeedToKnow),
                (now + chrono::TimeDelta::seconds(1), AccessDecision::Allow),
            ] {
                grant.retention_expires_at = Some(expiry);
                let grants = [grant.clone()];
                let mut req = request(
                    &caller,
                    Some(&tenant),
                    &labels,
                    &memberships,
                    &tenant,
                    &labels,
                    &grants,
                    AccessLevel::Read,
                );
                req.now = now;
                assert_eq!(decide(&req), expected);
                req.context_membership = Some(WorkContextMembershipLevel::Viewer);
                assert_eq!(decide(&req), AccessDecision::Allow);
            }
        }
    }

    #[test]
    fn best_grant_wins_across_multiple() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let nl = no_labels();
        let memberships = member("eng", GroupRole::Admin);
        let grants = [
            user_grant("alice", AccessLevel::Read),
            group_grant("eng", AccessLevel::Write, BTreeSet::new()),
        ];
        let req = request(
            &caller,
            Some(&tenant),
            &nl,
            &memberships,
            &tenant,
            &nl,
            &grants,
            AccessLevel::Write,
        );
        assert_eq!(decide(&req), AccessDecision::Allow);
    }
}
