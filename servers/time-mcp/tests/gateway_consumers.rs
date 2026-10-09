//! Installed read-only Time consumers. No activation, Task recovery or delivery claims.
//! Input: VEOVEO_TIME_CONSUMERS_INPUT, a closed JSON fixture containing installation,
//! authority, calendar, epoch, resolutions, zoneIds, scales, source and administrator.
//! Calendar and epoch fixtures must already exist. The selected authority must
//! remain unchanged throughout the run.
use anyhow::{Result, ensure};
use rmcp::{Peer, RoleClient};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
use veoveo_testing_support::installed::knowledge as installed;
#[path = "gateway_consumers/trace.rs"]
mod trace;
use veoveo_time_mcp::{
    AssessClockRequest, ClockAssessment, ClockCurrent, ConvertTimeOutput, ConvertTimeRequest,
    EffectiveTimeAuthority, EvaluateWindowsOutput, EvaluateWindowsRequest, MissionEpoch,
    OperationalCalendar, ResolveTimeOutput, ResolveTimeRequest, SubsecondNanoseconds,
    TimeExpressionValue, TimeInstant, TimeResource, TimeScale, TimeSource, TimeWindow,
    WindowOperation,
};
use veoveo_types::GatewayProfileId;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    authority: EffectiveTimeAuthority,
    calendar: OperationalCalendar,
    epoch: MissionEpoch,
    resolutions: Vec<ResolutionCase>,
    zone_ids: Vec<String>,
    scales: Vec<TimeScale>,
    source: TimeSource,
    administrator: Administrator,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Administrator {
    profile: GatewayProfileId,
    token_file: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ResolutionCase {
    request: ResolveTimeRequest,
    expected: TimeInstant,
    expected_utc: String,
}

impl Input {
    fn admit(&self) -> Result<()> {
        ensure!(
            self.installation.deployment == "time-mcp",
            "fixture must select Time MCP"
        );
        ensure!(
            (2..=16).contains(&self.resolutions.len()),
            "fixture requires two to sixteen resolution cases"
        );
        ensure!(
            !self.calendar.windows.is_empty(),
            "calendar fixture must contain recurrence windows"
        );
        ensure!(
            self.epoch.instant.authority == self.authority.binding(),
            "epoch fixture must bind the selected current authority"
        );
        ensure!(
            self.resolutions
                .iter()
                .all(|case| case.expected.authority == self.authority.binding()),
            "resolution expectations must bind the selected authority"
        );
        ensure!(
            (1..=8).contains(&self.zone_ids.len()) && (1..=6).contains(&self.scales.len()),
            "fixture requires bounded nonempty zone and scale selections"
        );
        ensure!(
            self.zone_ids
                .iter()
                .all(|zone| veoveo_time_mcp::TimeZoneId::parse(zone).is_ok() && zone.len() <= 128),
            "conversion zones must satisfy expression and resource profiles"
        );
        ensure!(
            self.zone_ids
                .iter()
                .enumerate()
                .all(|(index, zone)| !self.zone_ids[..index].contains(zone))
                && self
                    .scales
                    .iter()
                    .enumerate()
                    .all(|(index, scale)| !self.scales[..index].contains(scale)),
            "conversion selections must be distinct"
        );
        let boundary = |fraction| {
            self.resolutions
                .iter()
                .any(|case| match &*case.request.expression {
                    TimeExpressionValue::Unix { nanosecond, .. }
                    | TimeExpressionValue::Tai { nanosecond, .. } => {
                        *nanosecond == fraction && case.expected.nanosecond == fraction
                    }
                    _ => false,
                })
        };
        ensure!(
            boundary(SubsecondNanoseconds::ZERO) && boundary(SubsecondNanoseconds::MAX),
            "fixture must resolve both admitted subsecond endpoints through Unix or TAI expressions"
        );
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Check {
    CurrentAuthority,
    HttpsSourceMetadata,
    CalendarAndEpoch,
    ClockConsumers,
    ResolveAndConvert,
    EpochRelative,
    IntervalIntersection,
    StableAuthority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, veoveo_types::Vocabulary)]
enum Failure {
    Connection,
    ConsumerChecks,
    Deadline,
    ConnectionCleanup,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Receipt<'a> {
    schema_version: &'static str,
    authority: &'a EffectiveTimeAuthority,
    calendar: &'a OperationalCalendar,
    epoch: &'a MissionEpoch,
    source: &'a TimeSource,
    checks: &'a [Check],
    failures: &'a [Failure],
    trace: &'a trace::Trace,
    remaining_gates: [&'static str; 6],
}

fn open_receipt(path: &Path) -> Result<fs::File> {
    ensure!(path.is_absolute(), "receipt path must be absolute");
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    Ok(options.open(path)?)
}

fn write_receipt(
    file: &mut fs::File,
    input: &Input,
    checks: &[Check],
    failures: &[Failure],
    trace: &trace::Trace,
) -> Result<()> {
    serde_json::to_writer_pretty(
        &mut *file,
        &Receipt {
            schema_version: "veoveo.ai/time-consumers-acceptance/v1",
            authority: &input.authority,
            calendar: &input.calendar,
            epoch: &input.epoch,
            source: &input.source,
            checks,
            failures,
            trace,
            remaining_gates: [
                "authority_activation_and_conflict_rollback_f17_f21",
                "post_activation_epoch_behavior_f20",
                "restart_replica_and_startup_f18_f19",
                "schedule_task_lifecycle_and_recovery_f16",
                "delivered_resource_subscription",
                "acquisition_lifecycle_f15",
            ],
        },
    )?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

#[tokio::test]
#[ignore = "requires installed Time, private caller and admin-read tokens, selected HTTPS source, stable authority and populated calendar/epoch fixtures"]
async fn typed_time_consumers_through_public_gateway() -> Result<()> {
    // Admission diagnostics can include supplied values. Keep them out of test logs.
    let input: Input = installed::input_from("VEOVEO_TIME_CONSUMERS_INPUT")
        .map_err(|_| anyhow::anyhow!("Time fixture input admission failed"))?;
    input
        .admit()
        .map_err(|_| anyhow::anyhow!("Time fixture preconditions failed"))?;
    input
        .installation
        .validate()
        .map_err(|_| anyhow::anyhow!("Time installation admission failed"))?;
    let mut output = open_receipt(&input.installation.output)
        .map_err(|_| anyhow::anyhow!("Time receipt admission failed"))?;
    let mut checks = Vec::new();
    let mut failures = Vec::new();
    let mut trace = trace::Trace::default();
    match tokio::time::timeout(Duration::from_secs(75), input.installation.caller()).await {
        Ok(Ok(caller)) => {
            match tokio::time::timeout(
                Duration::from_secs(240),
                exercise(caller.peer(), &input, &mut checks, &mut trace),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    trace.finish(&Err::<(), _>(error));
                    failures.push(Failure::ConsumerChecks);
                }
                Err(_) => failures.push(Failure::Deadline),
            }
            // Shutdown is attempted separately even after the exercise deadline.
            if installed::close(caller).await.is_err() {
                failures.push(Failure::ConnectionCleanup);
            }
        }
        _ => failures.push(Failure::Connection),
    }
    // Only closed failure categories enter the receipt; raw SDK/provider errors
    // can contain private resource payloads or transport credentials.
    write_receipt(&mut output, &input, &checks, &failures, &trace)
        .map_err(|_| anyhow::anyhow!("Time receipt persistence failed"))?;
    ensure!(
        failures.is_empty(),
        "Time installed consumer checks failed; inspect the private receipt"
    );
    Ok(())
}

async fn exercise(
    peer: &Peer<RoleClient>,
    input: &Input,
    checks: &mut Vec<Check>,
    trace: &mut trace::Trace,
) -> Result<()> {
    let authority: EffectiveTimeAuthority =
        trace.read(peer, TimeResource::AuthoritiesCurrent).await?;
    ensure!(
        authority == input.authority,
        "selected current authority differs"
    );
    for reference in [authority.tzdb(), authority.leap_seconds()] {
        match reference.source() {
            veoveo_time_mcp::TimeAuthoritySource::Bootstrap {} => {
                let retained: veoveo_time_mcp::TimeAuthorityReference = trace
                    .read(
                        peer,
                        TimeResource::BootstrapAuthority(reference.release_id().clone()),
                    )
                    .await?;
                ensure!(
                    &retained == reference,
                    "bootstrap metadata disagrees with effective authority"
                );
            }
            veoveo_time_mcp::TimeAuthoritySource::Acquisition { source_id, .. } => {
                let retained: veoveo_time_mcp::AuthorityRelease = trace
                    .read(
                        peer,
                        TimeResource::AuthorityRelease(reference.release_id().clone()),
                    )
                    .await?;
                ensure!(
                    &retained.release_id == reference.release_id()
                        && &retained.source_id == source_id
                        && retained.dataset_kind == reference.dataset_kind()
                        && retained.version_label == reference.version_label()
                        && retained.source_digest_sha256.canonical() == reference.source_digest()
                        && retained.state == veoveo_time_mcp::AuthorityReleaseState::Active,
                    "acquired release metadata disagrees with effective authority"
                );
            }
        }
    }
    checks.push(Check::CurrentAuthority);
    read_source(input, trace).await?;
    checks.push(Check::HttpsSourceMetadata);
    let calendar: OperationalCalendar = trace
        .read(
            peer,
            TimeResource::Calendar {
                id: input.calendar.calendar_id.clone(),
                version: input.calendar.version,
            },
        )
        .await?;
    let epoch: MissionEpoch = trace
        .read(
            peer,
            TimeResource::EpochVersion {
                id: input.epoch.epoch_id.clone(),
                version: input.epoch.version,
            },
        )
        .await?;
    let latest_epoch: MissionEpoch = trace
        .read(peer, TimeResource::Epoch(input.epoch.epoch_id.clone()))
        .await?;
    ensure!(
        calendar == input.calendar && epoch == input.epoch && latest_epoch == epoch,
        "selected calendar or latest epoch differs"
    );
    checks.push(Check::CalendarAndEpoch);
    let clock: ClockCurrent = trace.read(peer, TimeResource::ClockCurrent).await?;
    ensure!(
        clock.time.effective_authority() == &authority
            && clock.time.instant().uncertainty_nanoseconds
                == clock.clock_quality.error_bound_nanoseconds,
        "clock authority or uncertainty disagrees"
    );
    let assessment: ClockAssessment = trace
        .call(
            peer,
            &AssessClockRequest {
                policy: Some(clock.effective_policy.clone()),
            },
        )
        .await?;
    ensure!(
        assessment.policy == clock.effective_policy,
        "clock assessment changed requested policy"
    );
    check_clock_assessment(&assessment)?;
    checks.push(Check::ClockConsumers);
    for (index, case) in input.resolutions.iter().enumerate() {
        trace.vector_index = Some(index);
        let resolved: ResolveTimeOutput = trace.call(peer, &case.request).await?;
        ensure!(
            resolved.instant() == &case.expected
                && resolved.effective_authority() == &authority
                && resolved.projection().utc_rfc3339 == case.expected_utc,
            "resolution differs from selected expectation"
        );
        let converted: ConvertTimeOutput = trace
            .call(
                peer,
                &ConvertTimeRequest {
                    instant: resolved.instant().clone(),
                    zone_ids: input.zone_ids.clone(),
                    scales: input.scales.clone(),
                },
            )
            .await?;
        ensure!(
            converted.canonical == resolved,
            "conversion changed the canonical resolution"
        );
        ensure!(
            converted
                .zoned
                .iter()
                .map(|zone| &zone.zone_id)
                .collect::<Vec<_>>()
                == input.zone_ids.iter().collect::<Vec<_>>()
                && converted
                    .scales
                    .iter()
                    .map(|scale| scale.scale)
                    .collect::<Vec<_>>()
                    == input.scales,
            "conversion selections disagree"
        );
        // Feed each advertised RFC9557 representation back into the public resolver.
        for zone in &converted.zoned {
            let roundtrip: ResolveTimeOutput = trace
                .call(
                    peer,
                    &ResolveTimeRequest {
                        expression: TimeExpressionValue::Rfc9557 {
                            value: zone.rfc9557.clone(),
                            disambiguation: veoveo_time_mcp::Disambiguation::Reject,
                        }
                        .build()?,
                        additional_uncertainty_nanoseconds: resolved
                            .instant()
                            .uncertainty_nanoseconds,
                    },
                )
                .await?;
            ensure!(
                roundtrip.instant() == resolved.instant(),
                "zoned representation changed the instant"
            );
        }
    }
    trace.vector_index = None;
    checks.push(Check::ResolveAndConvert);
    for offset_nanoseconds in [-1_i64, 0, 1] {
        let relative: ResolveTimeOutput = trace
            .call(
                peer,
                &ResolveTimeRequest {
                    expression: TimeExpressionValue::EpochRelative {
                        epoch_id: epoch.epoch_id.clone(),
                        offset_nanoseconds,
                    }
                    .build()?,
                    additional_uncertainty_nanoseconds: 7,
                },
            )
            .await?;
        let expected = TimeInstant::from_total_nanoseconds(
            epoch.instant.total_nanoseconds() + i128::from(offset_nanoseconds),
            epoch
                .instant
                .uncertainty_nanoseconds
                .checked_add(7)
                .ok_or_else(|| anyhow::anyhow!("epoch uncertainty overflow"))?,
            authority.binding(),
        )?;
        ensure!(
            relative.instant() == &expected && relative.effective_authority() == &authority,
            "epoch arithmetic or metadata disagrees"
        );
    }
    checks.push(Check::EpochRelative);
    let base = epoch.instant.total_nanoseconds();
    let at = |delta| {
        TimeInstant::from_total_nanoseconds(
            base + delta,
            epoch.instant.uncertainty_nanoseconds,
            authority.binding(),
        )
    };
    let left = TimeWindow::new(at(0)?, at(20)?)?;
    let right = TimeWindow::new(at(10)?, at(30)?)?;
    let evaluated: EvaluateWindowsOutput = trace
        .call(
            peer,
            &EvaluateWindowsRequest {
                operation: WindowOperation::Intersection,
                left: vec![left],
                right: vec![right],
            },
        )
        .await?;
    ensure!(
        evaluated.windows == vec![TimeWindow::new(at(10)?, at(20)?)?],
        "half-open interval intersection disagrees"
    );
    // A touching endpoint contributes no instant to a half-open intersection.
    let touching: EvaluateWindowsOutput = trace
        .call(
            peer,
            &EvaluateWindowsRequest {
                operation: WindowOperation::Intersection,
                left: vec![TimeWindow::new(at(0)?, at(10)?)?],
                right: vec![TimeWindow::new(at(10)?, at(20)?)?],
            },
        )
        .await?;
    check_intersection(&touching, &[])?;
    // Move the lower bound back one nanosecond: precisely one instant overlaps.
    let discrete: EvaluateWindowsOutput = trace
        .call(
            peer,
            &EvaluateWindowsRequest {
                operation: WindowOperation::Intersection,
                left: vec![TimeWindow::new(at(0)?, at(10)?)?],
                right: vec![TimeWindow::new(at(9)?, at(20)?)?],
            },
        )
        .await?;
    check_intersection(&discrete, &[TimeWindow::new(at(9)?, at(10)?)?])?;
    checks.push(Check::IntervalIntersection);
    let final_authority: EffectiveTimeAuthority =
        trace.read(peer, TimeResource::AuthoritiesCurrent).await?;
    ensure!(
        final_authority == authority,
        "authority changed during read-only consumer qualification"
    );
    checks.push(Check::StableAuthority);
    Ok(())
}

#[test]
fn receipt_refuses_relative_existing_and_symlink_paths() -> Result<()> {
    let directory = tempfile::tempdir()?;
    ensure!(open_receipt(Path::new("receipt.json")).is_err());
    let path = directory.path().join("receipt.json");
    let file = open_receipt(&path)?;
    drop(file);
    ensure!(open_receipt(&path).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        ensure!(fs::metadata(&path)?.permissions().mode() & 0o077 == 0);
        let link = directory.path().join("receipt-link.json");
        symlink(&path, &link)?;
        ensure!(open_receipt(&link).is_err());
    }
    Ok(())
}

fn source_url(input: &Input) -> Result<reqwest::Url> {
    let mut url = reqwest::Url::parse(input.installation.endpoint.as_str())?;
    url.set_path("/");
    url.set_query(None);
    url.set_fragment(None);
    url.path_segments_mut()
        .map_err(|_| anyhow::anyhow!("source route requires an HTTPS origin"))?
        .clear()
        .extend([
            "admin",
            input.administrator.profile.as_str(),
            "servers",
            "time",
            "sources",
            input.source.source_id.as_str(),
        ]);
    Ok(url)
}

async fn read_source(input: &Input, trace: &mut trace::Trace) -> Result<()> {
    trace.begin(trace::Request::SourceRead(input.source.source_id.clone()));
    let result = source_response(input, trace).await;
    trace.finish(&result);
    result
}

async fn source_response(input: &Input, trace: &mut trace::Trace) -> Result<()> {
    let authorization = installed::bearer_header(&input.administrator.token_file)?;
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(65))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let mut response = client
        .get(source_url(input)?)
        .header(reqwest::header::AUTHORIZATION, authorization)
        .send()
        .await?;
    let status = response.status();
    trace.http_status(status);
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        ensure!(
            bytes.len() + chunk.len() <= 64 * 1024,
            "source metadata exceeds 64 KiB"
        );
        bytes.extend_from_slice(&chunk);
    }
    trace.http(status, &bytes);
    ensure!(
        status == reqwest::StatusCode::OK,
        "source metadata read failed"
    );
    let source: TimeSource = serde_json::from_slice(&bytes)?;
    ensure!(
        source == input.source,
        "selected HTTPS source metadata differs"
    );
    Ok(())
}

#[cfg(test)]
fn fixture_value() -> serde_json::Value {
    use serde_json::json;
    use veoveo_time_mcp::{
        AuthorityDatasetKind, AuthorityReleaseId, TimeAuthorityReference, TimeAuthorityReleaseUri,
        TimeAuthoritySource,
    };
    let reference = |id, kind| {
        TimeAuthorityReference::new(
            TimeAuthorityReleaseUri::bootstrap(&AuthorityReleaseId::parse(id).unwrap()),
            kind,
            TimeAuthoritySource::Bootstrap {},
            veoveo_types::Sha256Digest::from_hex("a".repeat(64)).unwrap(),
            "fixture".into(),
        )
        .unwrap()
    };
    let authority = EffectiveTimeAuthority::new(
        reference("time-release-tzdb", AuthorityDatasetKind::Tzdb),
        reference("time-release-leap", AuthorityDatasetKind::LeapSeconds),
    )
    .unwrap();
    let instant = TimeInstant::from_total_nanoseconds(0, 0, authority.binding()).unwrap();
    let maximum = TimeInstant::from_total_nanoseconds(999_999_999, 0, authority.binding()).unwrap();
    json!({
        "installation": {"installationTarget":"/private/target.json", "endpoint":"https://installation.example/mcp/operator", "callerTokenFile":"/private/caller", "deployment":"time-mcp", "output":"/private/receipt.json"},
        "authority":authority,
        "calendar":{"calendarId":"calendar-fixture", "version":1, "name":"Fixture", "zoneId":"UTC", "windows":[{"startLocal":"2026-10-01T09:00:00", "endLocal":"2026-10-01T10:00:00", "recurrence":{"frequency":"daily", "interval":1,"count":1,"until":null}}]},
        "epoch":{"epochId":"epoch-fixture", "name":"Fixture", "instant":instant,"version":1},
        "resolutions":[
            {"request":{"expression":{"format":"tai","secondsSince1970":0,"nanosecond":0}}, "expected":instant,"expectedUtc":"1969-12-31T23:59:50Z"},
            {"request":{"expression":{"format":"tai","secondsSince1970":0,"nanosecond":999999999}}, "expected":maximum,"expectedUtc":"1969-12-31T23:59:50.999999999Z"}
        ],
        "zoneIds":["UTC"], "scales":["tai"],
        "source":{"sourceId":"time-source-fixture","name":"Fixture", "datasetKind":"tzdb","url":"https://data.iana.org/time-zones/releases/tzdata2026b.tar.gz","expectedContentType":"application/gzip","enabled":true,"recordVersion":1},
        "administrator":{"profile":"operator", "tokenFile":"/private/admin"}
    })
}

#[test]
fn fixture_rejects_unknown_fields_and_inconsistent_selections() -> Result<()> {
    let original = fixture_value();
    let admitted: Input = serde_json::from_value(original.clone())?;
    admitted.admit()?;
    for mutation in [
        "unknown",
        "wrongAuthority",
        "emptyCalendar",
        "missingBoundary",
        "duplicateZone",
        "invalidSource",
        "invalidIdentity",
    ] {
        let mut value = original.clone();
        match mutation {
            "unknown" => value["unexpected"] = true.into(),
            "wrongAuthority" => {
                value["epoch"]["instant"]["authority"]["tzdbReleaseId"] =
                    "time-release-other".into()
            }
            "emptyCalendar" => value["calendar"]["windows"] = serde_json::json!([]),
            "missingBoundary" => {
                value["resolutions"][1]["request"]["expression"]["nanosecond"] = 1.into()
            }
            "duplicateZone" => value["zoneIds"] = serde_json::json!(["UTC", "UTC"]),
            "invalidSource" => value["source"]["url"] = "http://data.iana.org/fixture".into(),
            "invalidIdentity" => value["epoch"]["epochId"] = "calendar-wrong-family".into(),
            _ => unreachable!(),
        }
        ensure!(
            serde_json::from_value::<Input>(value).map_or(true, |input| input.admit().is_err()),
            "invalid fixture was admitted"
        );
    }
    let route = source_url(&admitted)?;
    ensure!(
        route.as_str()
            == "https://installation.example/admin/operator/servers/time/sources/time-source-fixture"
    );
    Ok(())
}

#[test]
fn receipt_records_partial_checks_without_credentials() -> Result<()> {
    let input: Input = serde_json::from_value(fixture_value())?;
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("receipt.json");
    let mut file = open_receipt(&path)?;
    write_receipt(
        &mut file,
        &input,
        &[Check::CurrentAuthority],
        &[Failure::ConsumerChecks],
        &trace::Trace::default(),
    )?;
    let bytes = fs::read(&path)?;
    let report: serde_json::Value = serde_json::from_slice(&bytes)?;
    ensure!(report["schemaVersion"] == "veoveo.ai/time-consumers-acceptance/v1");
    ensure!(report["checks"] == serde_json::json!(["current_authority"]));
    ensure!(report["failures"] == serde_json::json!(["consumer_checks"]));
    ensure!(
        report["remainingGates"]
            .as_array()
            .is_some_and(|gates| gates.len() == 6)
    );
    let text = String::from_utf8(bytes)?;
    for private in [
        "/private/caller",
        "/private/admin",
        "callerTokenFile",
        "tokenFile",
        "administrator",
    ] {
        ensure!(!text.contains(private), "receipt exposed credential input");
    }
    Ok(())
}

fn check_intersection(actual: &EvaluateWindowsOutput, expected: &[TimeWindow]) -> Result<()> {
    ensure!(
        actual.windows == expected,
        "half-open intersection differs from expected nanosecond bounds"
    );
    Ok(())
}

#[test]
fn intersection_assertions_reject_touching_and_shifted_results() -> Result<()> {
    let input: Input = serde_json::from_value(fixture_value())?;
    let at = |delta| TimeInstant::from_total_nanoseconds(delta, 0, input.authority.binding());
    let one = TimeWindow::new(at(9)?, at(10)?)?;
    check_intersection(&EvaluateWindowsOutput { windows: vec![] }, &[])?;
    ensure!(
        check_intersection(
            &EvaluateWindowsOutput {
                windows: vec![one.clone()]
            },
            &[]
        )
        .is_err()
    );
    check_intersection(
        &EvaluateWindowsOutput {
            windows: vec![one.clone()],
        },
        std::slice::from_ref(&one),
    )?;
    ensure!(
        check_intersection(
            &EvaluateWindowsOutput {
                windows: vec![TimeWindow::new(at(9)?, at(11)?)?]
            },
            &[one]
        )
        .is_err()
    );
    Ok(())
}

fn check_clock_assessment(assessment: &ClockAssessment) -> Result<()> {
    let expected = veoveo_time_mcp::clock::assess_clock(
        assessment.quality.clone(),
        assessment.policy.clone(),
    )?;
    ensure!(
        assessment == &expected,
        "clock assessment classification disagrees with returned quality and policy"
    );
    Ok(())
}

#[test]
fn clock_assertion_rejects_misclassified_quality() -> Result<()> {
    let policy = veoveo_time_mcp::ClockQualityPolicy::builder()
        .maximum_error_nanoseconds(100)
        .maximum_stratum(4)
        .minimum_source_diversity(1)
        .maximum_holdover_seconds(10)
        .build()?;
    let quality = veoveo_time_mcp::ClockQualityValue {
        synchronized: false,
        estimated_offset_nanoseconds: 0,
        error_bound_nanoseconds: 101,
        stratum: 5,
        holdover_age_seconds: None,
        source_diversity: 0,
        traceability: vec![],
        observed_at: "2026-10-01T00:00:00Z".into(),
    }
    .build()?;
    let correct = veoveo_time_mcp::clock::assess_clock(quality.clone(), policy.clone())?;
    check_clock_assessment(&correct)?;
    let misclassified = veoveo_time_mcp::ClockAssessmentValue {
        quality,
        policy,
        acceptable: true,
        violations: vec![],
    }
    .build()?;
    ensure!(check_clock_assessment(&misclassified).is_err());
    Ok(())
}
