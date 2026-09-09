//! Direct governed detail projection, independent of the latest catalog window.
use super::*;
use veoveo_platform_store::{ArtifactId, PrincipalRecord, ShareLinkRecord};

pub(crate) async fn read_console_artifact(
    State(state): State<AdminState>,
    AxumPath((profile, artifact_id)): AxumPath<(String, veoveo_mcp_contract::ArtifactId)>,
    Extension(subject): Extension<AuthenticatedSubject>,
) -> Response {
    let started = Instant::now();
    let Some(profile_id) = admin_profile_id(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (_, _, subject) = match authorize_admin_request(
        &state,
        &profile_id,
        subject,
        GatewayAction::AdminRead,
        "admin/console/artifacts/read",
        BTreeMap::from([("artifact_id".into(), artifact_id.to_string())]),
        started,
    )
    .await
    {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match project(
        &state,
        &subject,
        ArtifactId::from_uuid(artifact_id.as_uuid()),
    )
    .await
    {
        Ok(Some(summary)) => (
            [(axum::http::header::CACHE_CONTROL, "no-store")],
            Json(summary),
        )
            .into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

async fn project(
    state: &AdminState,
    subject: &AuthenticatedSubject,
    artifact_id: ArtifactId,
) -> anyhow::Result<Option<ArtifactSummary>> {
    let Some(tenant_key) = subject.principal.tenant.as_ref() else {
        return Ok(None);
    };
    let tenant = deterministic_tenant_id(tenant_key.as_str())?.record_id();
    let store = state.control_store.platform_store();
    let Some(aggregate) = store.artifact_aggregate(artifact_id).await? else {
        return Ok(None);
    };
    if aggregate.occurrence.tenant != tenant || aggregate.blob.tenant != tenant {
        return Ok(None);
    }
    let access = ArtifactAccessContext::from_subject(subject, tenant_key.as_str())?;
    let mut response = store.client().query(
        "SELECT * FROM principal WHERE tenant = $tenant AND id = $owner; SELECT * FROM share_link WHERE tenant = $tenant AND artifact = $artifact LIMIT 200;"
    ).bind(("tenant", tenant)).bind(("owner", aggregate.occurrence.owner.clone()))
        .bind(("artifact", artifact_id.record_id())).await?.check()?;
    let principals: Vec<PrincipalRecord> = response.take(0)?;
    let links: Vec<ShareLinkRecord> = response.take(1)?;
    let names = principals
        .into_iter()
        .map(|row| Ok((record_key(&row.id)?, row.display_name)))
        .collect::<anyhow::Result<BTreeMap<_, _>>>()?;
    let summary = artifact_summary(
        aggregate.occurrence,
        Some(aggregate.blob.byte_len),
        aggregate
            .grants
            .iter()
            .map(artifact_grant_summary)
            .collect(),
        links
            .iter()
            .map(|row| share_link_summary(row, Utc::now()))
            .collect::<anyhow::Result<Vec<_>>>()?,
        &names,
        &access,
    )?;
    if !summary.effective_access.read {
        return Ok(None);
    }
    Ok(Some(summary))
}
