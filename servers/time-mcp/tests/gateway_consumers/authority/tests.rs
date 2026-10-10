use super::*;
use serde_json::json;
pub(super) fn fixture() -> Result<(Input, veoveo_deploy_contract::InstallationTarget)> {
    let base = super::super::fixture_value();
    let source = |family: &str, suffix: &str| {
        json!({
            "create":{"source":{"sourceId":format!("time-source-{}",uuid::Uuid::now_v7()),
                "name":"Isolated fixture","datasetKind":family,"url":"https://authority.example/product",
                "expectedContentType":"application/octet-stream","enabled":true,"recordVersion":0},"idempotencyKey":format!("create-{suffix}")},
            "digest":"a".repeat(64),"expectedVersionLabel":"fixture-release",
            "acquisitionKeys":[format!("{suffix}-a"),format!("{suffix}-b"),format!("{suffix}-c")]
        })
    };
    let input = serde_json::from_value(json!({
        "installation":base["installation"],"administrator":base["administrator"],
        "isolation":{"namespace":"isolated","tenantId":"isolated","workContext":"operations",
            "readerPrincipalId":"fixture-reader","administratorPrincipalId":"fixture-administrator"},
        "initialAuthority":base["authority"],"tzdb":source("tzdb","tz"),"leaps":source("leap_seconds","leap"),
        "epoch":base["epoch"],"epochIdempotencyKeys":["epoch-one","epoch-two"],
        "relativeOffsetNanoseconds":0,"expectedRelativeTaiNanoseconds":0,"expectedUtc":"1970-01-01T00:00:00Z"
    }))?;
    let target = serde_json::from_value(json!({"schema":"veoveo.ai/installation-target/v1",
        "kubernetes":{"context":"fixture","namespace":"isolated"},"localBaseUrl":"http://127.0.0.1:8080",
        "publicBaseUrl":"https://installation.example","controlPlane":"gateway.json","expectedDeployments":["time-mcp"],
        "minimumGpuShares":0,"operator":{"clientId":"fixture","profile":"operator","scopes":["time:read"],"workContext":"operations"}}))?;
    Ok((input, target))
}
#[test]
fn authority_fixture_rejects_wrong_isolation_family_epoch_and_expectation() -> Result<()> {
    let (mut input, target) = fixture()?;
    input.admit(&target)?;
    input.isolation.namespace = "another".into();
    ensure!(input.admit(&target).is_err());
    input.isolation.namespace = "isolated".into();
    input.expected_relative_tai_nanoseconds += 1;
    ensure!(input.admit(&target).is_err());
    input.expected_relative_tai_nanoseconds -= 1;
    input.epoch.version = veoveo_time_mcp::TimeVersion::new(2)?;
    ensure!(input.admit(&target).is_err());
    let (mut input, target) = fixture()?;
    input.leaps = input.tzdb.clone();
    ensure!(input.admit(&target).is_err());
    Ok(())
}
#[test]
fn activation_guard_uses_pointer_and_conflict_requires_typed_refusal() -> Result<()> {
    let (input, _) = fixture()?;
    let release: AuthorityRelease = serde_json::from_value(json!({
        "releaseId":format!("time-release-{}",uuid::Uuid::now_v7()),"sourceId":input.leaps.create.source.source_id,
        "datasetKind":"leap_seconds","versionLabel":"fixture-release","sourceUrl":"https://authority.example/product",
        "sourceDigestSha256":"a".repeat(64),"artifactPath":"/retained/leaps","state":"active",
        "retrievedAt":"2026-10-10T00:00:00Z","validatedAt":"2026-10-10T00:00:01Z","recordVersion":2
    }))?;
    let selected =
        ActiveAuthoritySelection::try_from(veoveo_time_mcp::ActiveAuthoritySelectionValue {
            pointer_version: veoveo_time_mcp::TimeVersion::new(7)?,
            release: release.clone(),
        })?;
    let request = workflow::activation(&release, &[selected]);
    ensure!(
        request.expected_active_pointer_version.expected_version() == 7
            && request.expected_release_record_version.get() == 2
    );
    ensure!(
        workflow::activation(&release, &[]).expected_active_pointer_version
            == TimeWriteGuard::Absent
    );
    workflow::refused(409, Some(AdminErrorCode::VersionConflict))?;
    ensure!(workflow::refused(500, Some(AdminErrorCode::InternalError)).is_err());
    ensure!(workflow::refused(409, None).is_err());
    Ok(())
}
#[test]
fn authority_delivery_rejects_foreign_identity_narrow_ack_and_unchanged_snapshot() -> Result<()> {
    let uri = TimeResource::AuthoritiesCurrent.to_uri()?;
    let filter = SubscriptionFilter::builder()
        .resource_subscriptions([uri.to_string()])
        .build();
    let id = rmcp::model::RequestId::Number(1);
    let notification = |uri: &str, id: rmcp::model::RequestId| {
        let mut value = ServerNotification::ResourceUpdatedNotification(
            rmcp::model::ResourceUpdatedNotification::new(
                rmcp::model::ResourceUpdatedNotificationParam::new(uri),
            ),
        );
        value.get_meta_mut().set_subscription_id(id);
        value
    };
    workflow::delivered(
        &notification(uri.as_str(), id.clone()),
        &id,
        &filter,
        &filter,
    )?;
    ensure!(
        workflow::delivered(
            &notification("time://clock/current", id.clone()),
            &id,
            &filter,
            &filter
        )
        .is_err()
    );
    ensure!(
        workflow::delivered(
            &notification("not a URI", id.clone()),
            &id,
            &filter,
            &filter
        )
        .is_err()
    );
    ensure!(
        workflow::delivered(
            &notification(uri.as_str(), rmcp::model::RequestId::Number(2)),
            &id,
            &filter,
            &filter
        )
        .is_err()
    );
    ensure!(
        workflow::delivered(
            &notification(uri.as_str(), id.clone()),
            &id,
            &filter,
            &SubscriptionFilter::builder().build()
        )
        .is_err()
    );
    let (input, _) = fixture()?;
    ensure!(workflow::changed_binding(&input.initial_authority, &input.initial_authority).is_err());
    Ok(())
}
