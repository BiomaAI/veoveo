//! Installed administrator health and public audit correlation.
use anyhow::{Result, ensure};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use veoveo_gateway_contract::{GatewayServerHealthState, ServerHealthReport};
use veoveo_mcp_contract::audit::*;
use veoveo_types::{
    GatewayProfileId, OAuthClientId, PrincipalId, ServerSlug, Sha256Digest, WorkContextId,
};

const EXPORT_BYTES: usize = 2 * 1024 * 1024;
const EXPORT_RECORDS: usize = 4096;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AuditReceipt {
    pub partition: AuditPartition,
    pub scanned_records: usize,
    pub admission_records: Vec<AuditRecordId>,
    pub completion_records: Vec<AuditRecordId>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AdminReceipt {
    pub profile: GatewayProfileId,
    pub actor: PrincipalId,
    pub client: OAuthClientId,
    pub started_at: DateTime<Utc>,
    pub active_check: Option<AdminCheck>,
    pub health_status: Option<u16>,
    pub health: Option<ServerHealthReport>,
    pub non_administrator_profile: GatewayProfileId,
    pub non_administrator_status: Option<u16>,
    pub non_administrator_body_digest: Option<Sha256Digest>,
    pub audit_partitions_status: Option<u16>,
    pub audit_partitions: Option<Vec<AuditPartition>>,
    pub audit_export_status: Option<u16>,
    pub audit: Option<AuditReceipt>,
    pub failure: Option<AdminFailure>,
}
impl AdminReceipt {
    /// The aggregate owner invokes this after dropping a timed-out operation.
    pub(super) fn timed_out(&mut self) {
        if let Some(check) = self.active_check {
            self.failure = Some(AdminFailure {
                check,
                reason: AdminFailureReason::Deadline,
            });
        }
    }

    pub(super) fn admit(
        installation: &veoveo_gateway_composition::smoke_support::InstalledTarget,
    ) -> Result<Self> {
        let identity = installation.administrator()?;
        Ok(Self {
            profile: identity.profile.clone(),
            actor: identity.principal.clone(),
            client: identity.client_id.clone(),
            started_at: Utc::now(),
            active_check: None,
            health_status: None,
            health: None,
            non_administrator_profile: installation.operator.profile.clone(),
            non_administrator_status: None,
            non_administrator_body_digest: None,
            audit_partitions_status: None,
            audit_partitions: None,
            audit_export_status: None,
            audit: None,
            failure: None,
        })
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
pub(super) enum AdminCheck {
    #[vocabulary(rename = "health")]
    Health,
    #[vocabulary(rename = "non_administrator")]
    NonAdministrator,
    #[vocabulary(rename = "audit_partitions")]
    AuditPartitions,
    #[vocabulary(rename = "audit_export")]
    AuditExport,
}
#[derive(Debug, Clone, Copy, Eq, PartialEq, veoveo_types::Vocabulary)]
pub(super) enum AdminFailureReason {
    #[vocabulary(rename = "deadline")]
    Deadline,
    #[vocabulary(rename = "transport")]
    Transport,
    #[vocabulary(rename = "body_too_large")]
    BodyTooLarge,
    #[vocabulary(rename = "unexpected_status")]
    UnexpectedStatus,
    #[vocabulary(rename = "typed_admission")]
    TypedAdmission,
    #[vocabulary(rename = "inventory")]
    Inventory,
    #[vocabulary(rename = "correlation")]
    Correlation,
    #[vocabulary(rename = "persistence")]
    Persistence,
}
#[derive(Debug, Clone, Serialize)]
pub(super) struct AdminFailure {
    pub check: AdminCheck,
    pub reason: AdminFailureReason,
}

pub(super) struct AdminInput<'a> {
    pub installation: &'a veoveo_gateway_composition::smoke_support::InstalledTarget,
    pub client: &'a reqwest::Client,
    pub operator_token: &'a str,
    pub admin_token: &'a str,
    pub expected_servers: &'a BTreeSet<ServerSlug>,
    pub healthy_servers: &'a BTreeSet<ServerSlug>,
    pub deadline: tokio::time::Instant,
}

/// The caller owns this receipt outside its aggregate cancellation future.
/// Each observed response and failure is persisted before another await/assertion.
pub(super) async fn run(
    input: AdminInput<'_>,
    receipt: &mut AdminReceipt,
    mut persist: impl FnMut(&AdminReceipt) -> Result<()>,
) -> Result<()> {
    let identity = input.installation.administrator()?;
    let health_url =
        input
            .installation
            .public_url(&["admin", identity.profile.as_str(), "server-health"])?;
    let body = request(
        input.client.get(health_url).bearer_auth(input.admin_token),
        &input,
        receipt,
        AdminCheck::Health,
        &mut persist,
    )
    .await?;
    if receipt.health_status != Some(200) {
        return fail(
            receipt,
            AdminCheck::Health,
            AdminFailureReason::UnexpectedStatus,
            &mut persist,
        );
    }
    let report = match serde_json::from_slice(&body) {
        Ok(report) => report,
        Err(_) => {
            return fail(
                receipt,
                AdminCheck::Health,
                AdminFailureReason::TypedAdmission,
                &mut persist,
            );
        }
    };
    receipt.health = Some(report);
    persist(receipt)?;
    if validate_health(
        receipt.health.as_ref().expect("admitted health"),
        input.expected_servers,
        input.healthy_servers,
    )
    .is_err()
    {
        return fail(
            receipt,
            AdminCheck::Health,
            AdminFailureReason::Inventory,
            &mut persist,
        );
    }
    let health_until = Utc::now() + chrono::TimeDelta::milliseconds(1);
    let operator_url = input.installation.public_url(&[
        "admin",
        input.installation.operator.profile.as_str(),
        "server-health",
    ])?;
    let body = request(
        input
            .client
            .get(operator_url)
            .bearer_auth(input.operator_token),
        &input,
        receipt,
        AdminCheck::NonAdministrator,
        &mut persist,
    )
    .await?;
    receipt.non_administrator_body_digest =
        Some(Sha256Digest::from_bytes(Sha256::digest(&body).into()));
    persist(receipt)?;
    if receipt
        .non_administrator_status
        .and_then(|status| reqwest::StatusCode::from_u16(status).ok())
        .is_none_or(|status| admission_denied(status).is_err())
    {
        return fail(
            receipt,
            AdminCheck::NonAdministrator,
            AdminFailureReason::UnexpectedStatus,
            &mut persist,
        );
    }
    let partition = AuditPartition::Tenant(identity.tenant.clone());
    let url = input.installation.public_url(&[
        "admin",
        identity.profile.as_str(),
        "console",
        "audit",
        "partitions",
    ])?;
    let body = request(
        input.client.get(url).bearer_auth(input.admin_token),
        &input,
        receipt,
        AdminCheck::AuditPartitions,
        &mut persist,
    )
    .await?;
    if receipt.audit_partitions_status != Some(200) {
        return fail(
            receipt,
            AdminCheck::AuditPartitions,
            AdminFailureReason::UnexpectedStatus,
            &mut persist,
        );
    }
    receipt.audit_partitions = match serde_json::from_slice(&body) {
        Ok(partitions) => Some(partitions),
        Err(_) => {
            return fail(
                receipt,
                AdminCheck::AuditPartitions,
                AdminFailureReason::TypedAdmission,
                &mut persist,
            );
        }
    };
    persist(receipt)?;
    if !receipt
        .audit_partitions
        .as_ref()
        .expect("admitted partitions")
        .contains(&partition)
    {
        return fail(
            receipt,
            AdminCheck::AuditPartitions,
            AdminFailureReason::Inventory,
            &mut persist,
        );
    }
    let mut query = AuditQuery::new(partition);
    query.actor = Some(identity.principal.clone());
    query.class = Some(AuditClass::ApiActivity);
    query.target = Some(AuditTarget::Installation);
    query.from = Some(receipt.started_at);
    query.until = Some(health_until);
    query.validate()?;
    let mut url = input.installation.public_url(&[
        "admin",
        identity.profile.as_str(),
        "console",
        "audit",
        "export",
    ])?;
    url.query_pairs_mut()
        .append_pair("query", &serde_json::to_string(&query)?);
    let body = request(
        input.client.get(url).bearer_auth(input.admin_token),
        &input,
        receipt,
        AdminCheck::AuditExport,
        &mut persist,
    )
    .await?;
    if receipt.audit_export_status != Some(200) {
        return fail(
            receipt,
            AdminCheck::AuditExport,
            AdminFailureReason::UnexpectedStatus,
            &mut persist,
        );
    }
    let registry = veoveo_gateway_catalog::audit_target_registry()?;
    receipt.audit = match correlate_export(
        &body,
        &registry,
        AuditCorrelation {
            principal: &identity.principal,
            client: &identity.client_id,
            profile: &identity.profile,
            context: &identity.work_context.id,
            query: &query,
        },
    ) {
        Ok(audit) => Some(audit),
        Err(_) => {
            return fail(
                receipt,
                AdminCheck::AuditExport,
                AdminFailureReason::Correlation,
                &mut persist,
            );
        }
    };
    persist(receipt)?;
    receipt.active_check = None;
    persist(receipt)?;
    Ok(())
}

fn fail<T>(
    receipt: &mut AdminReceipt,
    check: AdminCheck,
    reason: AdminFailureReason,
    persist: &mut impl FnMut(&AdminReceipt) -> Result<()>,
) -> Result<T> {
    receipt.failure = Some(AdminFailure { check, reason });
    persist(receipt)?;
    anyhow::bail!("installed administrator check failed; see typed partial receipt")
}

async fn request(
    request: reqwest::RequestBuilder,
    input: &AdminInput<'_>,
    receipt: &mut AdminReceipt,
    check: AdminCheck,
    persist: &mut impl FnMut(&AdminReceipt) -> Result<()>,
) -> Result<Vec<u8>> {
    bounded_response(request.send(), input.deadline, receipt, check, persist).await
}

/// Production and cancellation controls share this future seam. It replaces only
/// HTTP dispatch in controls; observations, persistence and bounds are unchanged.
async fn bounded_response(
    send: impl std::future::Future<Output = reqwest::Result<reqwest::Response>>,
    deadline: tokio::time::Instant,
    receipt: &mut AdminReceipt,
    check: AdminCheck,
    persist: &mut impl FnMut(&AdminReceipt) -> Result<()>,
) -> Result<Vec<u8>> {
    receipt.active_check = Some(check);
    persist(receipt)?;
    let result = tokio::time::timeout_at(
        deadline.min(tokio::time::Instant::now() + std::time::Duration::from_secs(15)),
        async {
            let mut response = send.await.map_err(|_| AdminFailureReason::Transport)?;
            let status = response.status().as_u16();
            match check {
                AdminCheck::Health => receipt.health_status = Some(status),
                AdminCheck::NonAdministrator => receipt.non_administrator_status = Some(status),
                AdminCheck::AuditPartitions => receipt.audit_partitions_status = Some(status),
                AdminCheck::AuditExport => receipt.audit_export_status = Some(status),
            }
            persist(receipt).map_err(|_| AdminFailureReason::Persistence)?;
            if response
                .content_length()
                .is_some_and(|length| length > EXPORT_BYTES as u64)
            {
                return Err(AdminFailureReason::BodyTooLarge);
            }
            let mut bytes = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| AdminFailureReason::Transport)?
            {
                if bytes.len().saturating_add(chunk.len()) > EXPORT_BYTES {
                    return Err(AdminFailureReason::BodyTooLarge);
                }
                bytes.extend_from_slice(&chunk);
            }
            Ok(bytes)
        },
    )
    .await;
    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(reason)) => fail(receipt, check, reason, persist),
        Err(_) => fail(receipt, check, AdminFailureReason::Deadline, persist),
    }
}

struct AuditCorrelation<'a> {
    principal: &'a PrincipalId,
    client: &'a OAuthClientId,
    profile: &'a GatewayProfileId,
    context: &'a WorkContextId,
    query: &'a AuditQuery,
}

fn validate_health(
    report: &ServerHealthReport,
    expected: &BTreeSet<ServerSlug>,
    healthy: &BTreeSet<ServerSlug>,
) -> Result<()> {
    let observed = report
        .servers
        .iter()
        .map(|entry| entry.server.clone())
        .collect::<BTreeSet<_>>();
    ensure!(
        observed.len() == report.servers.len() && &observed == expected,
        "administrator health registered-server set differs from the checked control plane"
    );
    ensure!(
        healthy.is_subset(expected),
        "selected healthy servers are not registered"
    );
    for entry in &report.servers {
        ensure!(
            entry.state.is_some() && entry.checked_at.is_some(),
            "administrator health contains a server without a completed probe"
        );
        if healthy.contains(&entry.server) {
            ensure!(
                entry.state == Some(GatewayServerHealthState::Healthy),
                "selected administrator health server is not healthy"
            );
        }
    }
    let mut modules = BTreeSet::new();
    ensure!(
        report
            .module_bindings
            .iter()
            .all(|binding| modules.insert(&binding.module)),
        "administrator health repeats a module binding"
    );
    Ok(())
}

fn admission_denied(status: reqwest::StatusCode) -> Result<u16> {
    ensure!(
        status == reqwest::StatusCode::FORBIDDEN,
        "authenticated non-administrator health request did not return HTTP 403"
    );
    Ok(status.as_u16())
}

fn correlate_export(
    bytes: &[u8],
    registry: &AuditTargetRegistry,
    expected: AuditCorrelation<'_>,
) -> Result<AuditReceipt> {
    expected.query.validate()?;
    ensure!(
        expected.query.actor.as_ref() == Some(expected.principal)
            && expected.query.target.as_ref() == Some(&AuditTarget::Installation)
            && expected.query.class == Some(AuditClass::ApiActivity)
            && expected.query.from.is_some()
            && expected.query.until.is_some(),
        "public audit correlation query does not identify its actor and run interval"
    );
    ensure!(
        bytes.len() <= EXPORT_BYTES,
        "public audit export exceeds 2 MiB"
    );
    let text = std::str::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("public audit export is not UTF-8"))?;
    let mut checkpoint = None;
    let mut saw_header = false;
    let mut completed = false;
    let mut scanned = 0usize;
    let mut ids = BTreeSet::new();
    let mut admissions = Vec::new();
    let mut completions = Vec::new();
    for (index, line) in text.lines().enumerate() {
        ensure!(
            index < EXPORT_RECORDS + 2 && !completed,
            "public audit export framing exceeds its bound"
        );
        let value = registry
            .decoder()
            .from_str::<AuditExportLine>(line)
            .map_err(|_| anyhow::anyhow!("public audit export failed owning typed admission"))?;
        match value {
            AuditExportLine::Header {
                query,
                first,
                checkpoint: current,
            } => {
                ensure!(
                    !saw_header && index == 0 && first.is_some() && current.is_some(),
                    "public audit export lacks its sealed header"
                );
                ensure!(
                    serde_json::to_value(&query)? == serde_json::to_value(expected.query)?,
                    "public audit export changed the admitted query"
                );
                let sealed = current.as_ref().expect("admitted sealed checkpoint");
                ensure!(
                    sealed.partition == expected.query.partition
                        && first.is_some_and(|first| first <= sealed.sequence),
                    "public audit checkpoint differs from its selected partition or range"
                );
                checkpoint = current;
                saw_header = true;
            }
            AuditExportLine::Record { record } => {
                ensure!(
                    saw_header && scanned < EXPORT_RECORDS,
                    "public audit record precedes its header or exceeds its bound"
                );
                scanned += 1;
                let draft = &record.draft;
                ensure!(
                    ids.insert(draft.id()),
                    "public audit export repeats a record"
                );
                let Some(actor) = draft.actor() else {
                    continue;
                };
                if &actor.principal != expected.principal
                    || actor.oauth_client.as_ref() != Some(expected.client)
                    || actor.kind != AuditPrincipalKind::Service
                    || draft.authority().profile.as_ref() != Some(expected.profile)
                    || draft.authority().work_context.as_ref() != Some(expected.context)
                    || draft.partition() != &expected.query.partition
                    || expected
                        .query
                        .from
                        .is_none_or(|from| draft.occurred_at() < from)
                    || expected
                        .query
                        .until
                        .is_none_or(|until| draft.occurred_at() >= until)
                    || draft.target() != &AuditTarget::Installation
                    || draft.reason() != AuditReason::Accepted
                {
                    continue;
                }
                match draft.detail() {
                    AuditDetail::AdminAdmission {
                        operation: AdministrativeOperation::ServerHealth,
                        access: AdministrativeAccess::Read,
                    } if draft.outcome() == AuditOutcome::Allowed => admissions.push(draft.id()),
                    AuditDetail::AdminCompletion {
                        operation: AdministrativeOperation::ServerHealth,
                        access: AdministrativeAccess::Read,
                        failure: None,
                    } if draft.outcome() == AuditOutcome::Succeeded => completions.push(draft.id()),
                    _ => {}
                }
            }
            AuditExportLine::Complete {
                records,
                checkpoint: current,
            } => {
                ensure!(
                    saw_header && records == u64::try_from(scanned)? && current == checkpoint,
                    "public audit completion disagrees with its sealed export"
                );
                completed = true;
            }
        }
    }
    ensure!(
        completed && !admissions.is_empty(),
        "public audit export does not prove this administrator ServerHealth admission"
    );
    Ok(AuditReceipt {
        partition: expected.query.partition.clone(),
        scanned_records: scanned,
        admission_records: admissions,
        completion_records: completions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_gateway_contract::ServerHealthEntry;
    use veoveo_types::Sha256Digest;

    #[test]
    fn health_admits_offline_unselected_servers_but_requires_exact_probed_inventory() {
        let slug = |name| ServerSlug::parse(name).unwrap();
        let mut report = ServerHealthReport {
            servers: vec![
                ServerHealthEntry {
                    server: slug("frames"),
                    state: Some(GatewayServerHealthState::Healthy),
                    checked_at: Some(Utc::now()),
                },
                ServerHealthEntry {
                    server: slug("reason"),
                    state: Some(GatewayServerHealthState::Offline),
                    checked_at: Some(Utc::now()),
                },
            ],
            module_bindings: vec![],
        };
        let expected = BTreeSet::from([slug("frames"), slug("reason")]);
        let healthy = BTreeSet::from([slug("frames")]);
        assert!(validate_health(&report, &expected, &healthy).is_ok());
        report.servers[1].checked_at = None;
        assert!(validate_health(&report, &expected, &healthy).is_err());
        report.servers[1].checked_at = Some(Utc::now());
        report.servers[0].state = Some(GatewayServerHealthState::Degraded);
        assert!(validate_health(&report, &expected, &healthy).is_err());
        report.servers[0].state = Some(GatewayServerHealthState::Healthy);
        report.servers.push(report.servers[0].clone());
        assert!(validate_health(&report, &expected, &healthy).is_err());
        report.servers.pop();
        report.servers.pop();
        assert!(validate_health(&report, &expected, &healthy).is_err());
    }

    #[test]
    fn denial_requires_an_actual_authorization_http_status() {
        assert!(admission_denied(reqwest::StatusCode::UNAUTHORIZED).is_err());
        assert_eq!(
            admission_denied(reqwest::StatusCode::FORBIDDEN).unwrap(),
            403
        );
        for status in [
            reqwest::StatusCode::OK,
            reqwest::StatusCode::NOT_FOUND,
            reqwest::StatusCode::BAD_GATEWAY,
        ] {
            assert!(admission_denied(status).is_err());
        }
    }

    struct Fixture {
        principal: PrincipalId,
        client: OAuthClientId,
        profile: GatewayProfileId,
        context: WorkContextId,
        query: AuditQuery,
        record: AuditRecord,
    }
    impl Fixture {
        fn new() -> Self {
            let principal = PrincipalId::parse("protocol-admin").unwrap();
            let client = OAuthClientId::parse("protocol-admin-client").unwrap();
            let profile = GatewayProfileId::parse("admin").unwrap();
            let context = WorkContextId::parse("operations").unwrap();
            let now = Utc::now();
            let mut query = AuditQuery::new(AuditPartition::Installation);
            query.actor = Some(principal.clone());
            query.from = Some(now - chrono::TimeDelta::seconds(1));
            query.until = Some(now + chrono::TimeDelta::seconds(1));
            query.class = Some(AuditClass::ApiActivity);
            query.target = Some(AuditTarget::Installation);
            let draft = AuditDraft::builder(
                AuditRequest::background(),
                AuditTarget::Installation,
                AuditDetail::AdminAdmission {
                    operation: AdministrativeOperation::ServerHealth,
                    access: AdministrativeAccess::Read,
                },
                AuditOutcome::Allowed,
                AuditReason::Accepted,
            )
            .actor(AuditActor {
                principal: principal.clone(),
                kind: AuditPrincipalKind::Service,
                tenant: None,
                oauth_client: Some(client.clone()),
                session_family: None,
                delegating_principal: None,
                managed_agent: None,
            })
            .authority(AuditAuthority {
                profile: Some(profile.clone()),
                work_context: Some(context.clone()),
                ..AuditAuthority::default()
            })
            .occurred_at(now)
            .build()
            .unwrap();
            Self {
                principal,
                client,
                profile,
                context,
                query,
                record: AuditRecord {
                    draft,
                    recorded_at: now,
                },
            }
        }
        fn expected(&self) -> AuditCorrelation<'_> {
            AuditCorrelation {
                principal: &self.principal,
                client: &self.client,
                profile: &self.profile,
                context: &self.context,
                query: &self.query,
            }
        }
        fn lines(&self) -> Vec<String> {
            let checkpoint = AuditCheckpoint {
                partition: self.query.partition.clone(),
                sequence: AuditBlockSequence::new(1).unwrap(),
                head_hash: Sha256Digest::from_hex("0".repeat(64)).unwrap(),
                last_versionstamp: AuditVersionstamp::new(1).unwrap(),
            };
            [
                AuditExportLine::Header {
                    query: Box::new(self.query.clone()),
                    first: Some(checkpoint.sequence),
                    checkpoint: Some(checkpoint.clone()),
                },
                AuditExportLine::Record {
                    record: Box::new(self.record.clone()),
                },
                AuditExportLine::Complete {
                    records: 1,
                    checkpoint: Some(checkpoint),
                },
            ]
            .iter()
            .map(|line| serde_json::to_string(line).unwrap())
            .collect()
        }
    }

    #[tokio::test]
    async fn cancellation_keeps_health_and_forbidden_observations_before_next_request() {
        for denied_observed in [false, true] {
            let fixture = Fixture::new();
            let mut receipt = AdminReceipt {
                profile: fixture.profile.clone(),
                actor: fixture.principal.clone(),
                client: fixture.client.clone(),
                started_at: Utc::now(),
                active_check: None,
                health_status: Some(200),
                health: Some(ServerHealthReport {
                    servers: vec![ServerHealthEntry {
                        server: ServerSlug::parse("frames").unwrap(),
                        state: Some(GatewayServerHealthState::Healthy),
                        checked_at: Some(Utc::now()),
                    }],
                    module_bindings: vec![],
                }),
                non_administrator_profile: GatewayProfileId::parse("operator").unwrap(),
                non_administrator_status: denied_observed.then_some(403),
                non_administrator_body_digest: denied_observed
                    .then(|| Sha256Digest::from_bytes(Sha256::digest(b"forbidden").into())),
                audit_partitions_status: None,
                audit_partitions: None,
                audit_export_status: None,
                audit: None,
                failure: None,
            };
            let mut snapshots = Vec::new();
            let mut persist = |receipt: &AdminReceipt| {
                snapshots.push(receipt.clone());
                Ok(())
            };
            let next = if denied_observed {
                AdminCheck::AuditPartitions
            } else {
                AdminCheck::NonAdministrator
            };
            let cancelled = tokio::time::timeout(
                std::time::Duration::from_millis(1),
                bounded_response(
                    std::future::pending::<reqwest::Result<reqwest::Response>>(),
                    tokio::time::Instant::now() + std::time::Duration::from_secs(30),
                    &mut receipt,
                    next,
                    &mut persist,
                ),
            )
            .await;
            assert!(cancelled.is_err());
            receipt.timed_out();
            persist(&receipt).unwrap();
            let saved = snapshots.last().unwrap();
            assert_eq!(saved.health_status, Some(200));
            assert_eq!(saved.health.as_ref().unwrap().servers.len(), 1);
            assert_eq!(
                saved.non_administrator_status,
                denied_observed.then_some(403)
            );
            assert_eq!(
                saved.non_administrator_body_digest.is_some(),
                denied_observed
            );
            assert_eq!(saved.failure.as_ref().unwrap().check, next);
            assert_eq!(
                saved.failure.as_ref().unwrap().reason,
                AdminFailureReason::Deadline
            );
        }
    }

    #[test]
    fn audit_export_proves_full_identity_and_requires_complete_sealed_framing() {
        let fixture = Fixture::new();
        let registry = veoveo_gateway_catalog::audit_target_registry().unwrap();
        let lines = fixture.lines();
        let read = |lines: &[String]| {
            correlate_export(lines.join("\n").as_bytes(), &registry, fixture.expected())
        };
        let receipt = read(&lines).unwrap();
        assert_eq!(receipt.admission_records, vec![fixture.record.draft.id()]);
        assert!(receipt.completion_records.is_empty());
        assert!(read(&lines[..2]).is_err());
        assert!(read(&[lines[1].clone(), lines[0].clone(), lines[2].clone()]).is_err());
        assert!(
            read(&[
                lines[0].clone(),
                lines[1].clone(),
                lines[1].clone(),
                lines[2].clone()
            ])
            .is_err()
        );
        let mut altered = lines.clone();
        altered[2] = altered[2].replace("\"records\":1", "\"records\":2");
        assert!(read(&altered).is_err());
        assert!(
            correlate_export(&vec![b' '; EXPORT_BYTES + 1], &registry, fixture.expected()).is_err()
        );
    }

    #[test]
    fn audit_export_rejects_wrong_client_profile_actor_context_time_or_outcome() {
        let fixture = Fixture::new();
        let registry = veoveo_gateway_catalog::audit_target_registry().unwrap();
        for (from, to) in [
            ("protocol-admin-client", "another-client"),
            ("\"profile\":\"admin\"", "\"profile\":\"other\""),
            (
                "\"principal\":\"protocol-admin\"",
                "\"principal\":\"another-actor\"",
            ),
            (
                "\"work_context\":\"operations\"",
                "\"work_context\":\"another-context\"",
            ),
            (
                "\"operation\":\"server_health\"",
                "\"operation\":\"control_plane\"",
            ),
        ] {
            let mut lines = fixture.lines();
            assert!(
                lines[1].contains(from),
                "fixture substitution must exercise real wire data"
            );
            lines[1] = lines[1].replace(from, to);
            assert!(
                correlate_export(lines.join("\n").as_bytes(), &registry, fixture.expected())
                    .is_err()
            );
        }
        let mut denied = fixture.lines();
        denied[1] = denied[1]
            .replace("\"outcome\":\"allowed\"", "\"outcome\":\"denied\"")
            .replace("\"reason\":\"accepted\"", "\"reason\":\"policy_denied\"");
        assert!(
            correlate_export(denied.join("\n").as_bytes(), &registry, fixture.expected()).is_err()
        );
        let mut query = fixture.query.clone();
        query.from = Some(Utc::now() + chrono::TimeDelta::seconds(10));
        query.until = Some(Utc::now() + chrono::TimeDelta::seconds(20));
        let expected = AuditCorrelation {
            query: &query,
            ..fixture.expected()
        };
        // A matching header cannot make an out-of-window record satisfy correlation.
        let mut lines = fixture.lines();
        lines[0] = serde_json::to_string(&AuditExportLine::Header {
            query: Box::new(query.clone()),
            first: Some(AuditBlockSequence::new(1).unwrap()),
            checkpoint: Some(AuditCheckpoint {
                partition: query.partition.clone(),
                sequence: AuditBlockSequence::new(1).unwrap(),
                head_hash: Sha256Digest::from_hex("0".repeat(64)).unwrap(),
                last_versionstamp: AuditVersionstamp::new(1).unwrap(),
            }),
        })
        .unwrap();
        assert!(correlate_export(lines.join("\n").as_bytes(), &registry, expected).is_err());
    }
}
