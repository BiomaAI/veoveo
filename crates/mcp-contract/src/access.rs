//! Layered access-control primitives for the shared artifact plane.
//!
//! Veoveo access control is several models composed, not a single ACL (see
//! `docs/TECH_DESIGN.md`, "Access control model"). This module owns the pure,
//! I/O-free core:
//!
//! - **DAC / ACL** — [`Grant`] scopes one artifact to one [`Subject`] at one
//!   [`AccessLevel`]. This is the discretionary "share with those people" layer.
//! - **Groups** — a [`Subject::Group`] grant plus the caller's
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

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::gateway::{DataLabelId, GroupId, PrincipalId, TenantId};
use crate::uri::is_sha256;

/// A capability level. Ordered `Read < Write < Admin`, so `min` yields the
/// lesser privilege — exactly what capping a group role by a grant level needs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum AccessLevel {
    Read,
    Write,
    Admin,
}

impl AccessLevel {
    /// True when `self` is sufficient for a request that needs `required`.
    pub fn allows(self, required: AccessLevel) -> bool {
        self >= required
    }
}

/// The role a principal holds *within* a group. Same lattice as a grant level,
/// so the two compose under `min`. A distinct alias keeps call sites honest
/// about which axis they mean ("role in group" vs "level on artifact").
pub type GroupRole = AccessLevel;

/// Who a grant is made to. Groups enter the model here, as a grant subject —
/// not as a set of permissions and not as a resource container.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case", tag = "kind", content = "id")]
pub enum Subject {
    User(PrincipalId),
    Group(GroupId),
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

/// A content-addressed artifact identity: 64 lowercase hex characters.
///
/// The sha is the artifact's *content* identity (integrity, dedup within a
/// tenant). It is deliberately never the physical storage key — that is
/// `f(tenant, sha)` under a per-tenant key so identical bytes across tenants
/// never collide.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactSha256(String);

/// Error returned when an artifact sha is not 64 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactShaError;

impl std::fmt::Display for ArtifactShaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("artifact sha must be 64 lowercase hex characters")
    }
}

impl std::error::Error for ArtifactShaError {}

impl ArtifactSha256 {
    pub fn new(value: impl Into<String>) -> Result<Self, ArtifactShaError> {
        let value = value.into();
        if is_sha256(&value) {
            Ok(Self(value))
        } else {
            Err(ArtifactShaError)
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The neutral, cross-server plane URI for this artifact: `artifact://{sha}`.
    /// Distinct from per-server schemes (`media://artifact/{sha}`), so any
    /// server can resolve any artifact through the one artifact service.
    pub fn plane_uri(&self) -> String {
        format!("{ARTIFACT_PLANE_SCHEME}://{}", self.0)
    }
}

impl std::fmt::Display for ArtifactSha256 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<String> for ArtifactSha256 {
    type Error = ArtifactShaError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<ArtifactSha256> for String {
    fn from(value: ArtifactSha256) -> Self {
        value.0
    }
}

/// The neutral scheme every server uses to name any artifact on the shared plane.
pub const ARTIFACT_PLANE_SCHEME: &str = "artifact";

/// Parse an artifact reference into its sha, validating the sha. Accepts both
/// the neutral plane form `artifact://{sha}` and any server-presented form
/// `{scheme}://artifact/{sha}` (e.g. `media://artifact/{sha}`,
/// `duckdb://artifact/{sha}`), so a URI a client received from any server can be
/// pasted back as cross-server input without translation.
pub fn parse_artifact_plane_uri(uri: &str) -> Option<ArtifactSha256> {
    if let Some(rest) = uri.strip_prefix(&format!("{ARTIFACT_PLANE_SCHEME}://"))
        && !rest.contains('/')
    {
        return ArtifactSha256::new(rest).ok();
    }
    // `{scheme}://artifact/{sha}` — take the segment after the last `/artifact/`.
    let rest = uri.rsplit_once("://artifact/").map(|(_, sha)| sha)?;
    ArtifactSha256::new(rest).ok()
}

/// One entry in an artifact's access control list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Grant {
    pub artifact: ArtifactSha256,
    pub subject: Subject,
    pub level: AccessLevel,
    /// Tenant the artifact (and therefore this grant) lives in. Isolation is a
    /// hard partition: a grant never bridges tenants.
    pub tenant: TenantId,
    /// Labels the artifact carries. MAC is checked against these independently
    /// of the grant; no grant can widen clearance.
    #[serde(default)]
    pub data_labels: BTreeSet<DataLabelId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retention_expires_at: Option<DateTime<Utc>>,
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
pub fn grant_level_for_caller(
    grant: &Grant,
    caller_id: &PrincipalId,
    memberships: &BTreeSet<GroupMembership>,
) -> Option<AccessLevel> {
    match &grant.subject {
        Subject::User(user) if user == caller_id => Some(grant.level),
        Subject::User(_) => None,
        Subject::Group(group) => {
            role_in_group(memberships, group).map(|role| role.min(grant.level))
        }
    }
}

/// MAC: the caller's clearance dominates the artifact's labels iff every label
/// on the artifact is also held by the caller.
pub fn mac_satisfied(
    artifact_labels: &BTreeSet<DataLabelId>,
    caller_labels: &BTreeSet<DataLabelId>,
) -> bool {
    artifact_labels.is_subset(caller_labels)
}

/// Everything an access decision needs. Borrowed so callers assemble it from
/// the ledger and the signed identity without cloning.
pub struct AccessRequest<'a> {
    pub caller_id: &'a PrincipalId,
    pub caller_tenant: Option<&'a TenantId>,
    pub caller_labels: &'a BTreeSet<DataLabelId>,
    pub memberships: &'a BTreeSet<GroupMembership>,
    /// Tenant the artifact lives in.
    pub artifact_tenant: &'a TenantId,
    /// Labels the artifact carries (classification unioned into data labels).
    pub artifact_labels: &'a BTreeSet<DataLabelId>,
    /// Grants recorded for this artifact.
    pub grants: &'a [Grant],
    pub requested: AccessLevel,
}

/// Compose tenancy, DAC, and MAC into one decision.
///
/// Order encodes the invariants: tenant isolation is the hard boundary and is
/// checked first; then both need-to-know (DAC) *and* clearance (MAC) must hold,
/// with a MAC failure reported as `DenyClearance` even if a grant would suffice.
pub fn decide(req: &AccessRequest<'_>) -> AccessDecision {
    if req.caller_tenant != Some(req.artifact_tenant) {
        return AccessDecision::DenyTenant;
    }

    let best_dac = req
        .grants
        .iter()
        .filter_map(|grant| grant_level_for_caller(grant, req.caller_id, req.memberships))
        .max();

    let mac = mac_satisfied(req.artifact_labels, req.caller_labels);

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
        PrincipalId::new(s).unwrap()
    }
    fn gid(s: &str) -> GroupId {
        GroupId::new(s).unwrap()
    }
    fn tid(s: &str) -> TenantId {
        TenantId::new(s).unwrap()
    }
    fn lid(s: &str) -> DataLabelId {
        DataLabelId::new(s).unwrap()
    }
    fn sha() -> ArtifactSha256 {
        ArtifactSha256::new("a".repeat(64)).unwrap()
    }

    fn member(group: &str, role: AccessLevel) -> BTreeSet<GroupMembership> {
        let mut s = BTreeSet::new();
        s.insert(GroupMembership {
            group: gid(group),
            role,
        });
        s
    }

    fn user_grant(user: &str, level: AccessLevel) -> Grant {
        Grant {
            artifact: sha(),
            subject: Subject::User(pid(user)),
            level,
            tenant: tid("acme"),
            data_labels: BTreeSet::new(),
            retention_expires_at: None,
        }
    }

    fn group_grant(group: &str, level: AccessLevel, labels: BTreeSet<DataLabelId>) -> Grant {
        Grant {
            artifact: sha(),
            subject: Subject::Group(gid(group)),
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
        artifact_tenant: &'a TenantId,
        artifact_labels: &'a BTreeSet<DataLabelId>,
        grants: &'a [Grant],
        requested: AccessLevel,
    ) -> AccessRequest<'a> {
        AccessRequest {
            caller_id: caller,
            caller_tenant: tenant,
            caller_labels: labels,
            memberships,
            artifact_tenant,
            artifact_labels,
            grants,
            requested,
        }
    }

    #[test]
    fn access_level_is_ordered_read_write_admin() {
        assert!(AccessLevel::Read < AccessLevel::Write);
        assert!(AccessLevel::Write < AccessLevel::Admin);
        assert_eq!(AccessLevel::Read.min(AccessLevel::Admin), AccessLevel::Read);
    }

    #[test]
    fn parse_plane_uri_accepts_neutral_and_server_forms() {
        let sha = "a".repeat(64);
        let expected = ArtifactSha256::new(sha.clone()).unwrap();
        // Neutral plane form.
        assert_eq!(
            parse_artifact_plane_uri(&format!("artifact://{sha}")),
            Some(expected.clone())
        );
        // Any server-presented `{scheme}://artifact/{sha}` form round-trips.
        for scheme in ["media", "duckdb", "timeseries", "optimization"] {
            assert_eq!(
                parse_artifact_plane_uri(&format!("{scheme}://artifact/{sha}")),
                Some(expected.clone()),
                "scheme {scheme}"
            );
        }
        // Junk and bad shas are rejected.
        assert_eq!(parse_artifact_plane_uri("artifact://not-a-sha"), None);
        assert_eq!(parse_artifact_plane_uri("media://artifact/xyz"), None);
        assert_eq!(parse_artifact_plane_uri("media://models"), None);
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
        let memberships = member("eng", AccessLevel::Write);
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
        let memberships = member("eng", AccessLevel::Read);
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
        let artifact_labels: BTreeSet<_> = [lid("cui")].into_iter().collect();
        let memberships = member("eng", AccessLevel::Admin);
        let grants = [group_grant(
            "eng",
            AccessLevel::Admin,
            artifact_labels.clone(),
        )];
        let req = request(
            &caller,
            Some(&tenant),
            &caller_labels,
            &memberships,
            &tenant,
            &artifact_labels,
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
        let artifact_labels: BTreeSet<_> = [lid("cui")].into_iter().collect();
        let memberships = member("eng", AccessLevel::Write);
        let grants = [group_grant(
            "eng",
            AccessLevel::Write,
            artifact_labels.clone(),
        )];
        let req = request(
            &caller,
            Some(&tenant),
            &caller_labels,
            &memberships,
            &tenant,
            &artifact_labels,
            &grants,
            AccessLevel::Write,
        );
        assert_eq!(decide(&req), AccessDecision::Allow);
    }

    #[test]
    fn different_tenant_is_denied_before_anything_else() {
        let caller = pid("alice");
        let caller_tenant = tid("evil");
        let artifact_tenant = tid("acme");
        let nl = no_labels();
        let nm = no_members();
        // A grant that would otherwise allow, plus full clearance.
        let grants = [user_grant("alice", AccessLevel::Admin)];
        let req = request(
            &caller,
            Some(&caller_tenant),
            &nl,
            &nm,
            &artifact_tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&req), AccessDecision::DenyTenant);
    }

    #[test]
    fn tenantless_caller_is_denied() {
        let caller = pid("alice");
        let artifact_tenant = tid("acme");
        let nl = no_labels();
        let nm = no_members();
        let grants = [user_grant("alice", AccessLevel::Admin)];
        let req = request(
            &caller,
            None,
            &nl,
            &nm,
            &artifact_tenant,
            &nl,
            &grants,
            AccessLevel::Read,
        );
        assert_eq!(decide(&req), AccessDecision::DenyTenant);
    }

    #[test]
    fn best_grant_wins_across_multiple() {
        let caller = pid("alice");
        let tenant = tid("acme");
        let nl = no_labels();
        let memberships = member("eng", AccessLevel::Admin);
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

    #[test]
    fn artifact_sha_validation_and_plane_uri() {
        assert!(ArtifactSha256::new("bad").is_err());
        let s = ArtifactSha256::new("a".repeat(64)).unwrap();
        assert_eq!(s.plane_uri(), format!("artifact://{}", "a".repeat(64)));
        let parsed = parse_artifact_plane_uri(&s.plane_uri()).unwrap();
        assert_eq!(parsed, s);
        assert!(parse_artifact_plane_uri("artifact://bad").is_none());
        assert!(parse_artifact_plane_uri("media://artifact/x").is_none());
    }
}
