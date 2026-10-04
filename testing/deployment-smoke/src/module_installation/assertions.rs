//! Installed observations, rather than mirrored render assertions.
use super::fixture::{Fixture, Render};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Lane {
    module: String,
    initialized: bool,
    current: Option<u64>,
    latest: Option<u64>,
    selected: bool,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Status {
    status: String,
    generation: String,
    lanes: Vec<Lane>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Publication {
    status: String,
    revision_id: String,
    sha256: String,
    servers: usize,
    profiles: usize,
    revisions: u64,
    active_objects: u64,
    runtime_auth_verified: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Case {
    name: String,
    generation: String,
    lanes: Vec<Lane>,
    jobs: BTreeMap<String, String>,
    runtime_auth_verified: bool,
    publication_revision: String,
    publication_sha256: String,
    no_op_reused: bool,
    stale_commands_rejected: Vec<String>,
    observed_image_ids: Vec<String>,
}

fn job<'a>(render: &'a Render, component: &str) -> Result<&'a Value> {
    render
        .objects
        .iter()
        .find(|o| {
            o["kind"] == "Job"
                && o["metadata"]["labels"]["app.kubernetes.io/component"] == component
        })
        .context("rendered installation Job missing")
}
fn lane<'a>(lanes: &'a [Lane], name: &str) -> Result<&'a Lane> {
    lanes
        .iter()
        .find(|l| l.module == name)
        .context("expected catalog lane missing")
}
fn status(fixture: &mut Fixture, render: &Render, generation: u64) -> Result<Status> {
    let bytes = fixture.probe(
        job(render, "module-migration")?,
        &format!("status-{generation}"),
        &["module-status", "--wait-seconds", "60"],
        true,
    )?;
    let status: Status =
        serde_json::from_slice(&bytes).context("decode installed module status")?;
    ensure!(
        status.status == "current" && status.generation == render.plan.generation().to_string(),
        "installed status does not match requested preparation"
    );
    let selected: Vec<_> = status
        .lanes
        .iter()
        .filter(|l| l.selected)
        .map(|l| l.module.as_str())
        .collect();
    ensure!(
        selected.len() == render.plan.lanes().len(),
        "installed status lost selected lane"
    );
    for l in status.lanes.iter().filter(|l| l.selected) {
        ensure!(
            l.initialized && l.current == l.latest,
            "selected installed lane is incomplete"
        );
    }
    Ok(status)
}
fn publication(fixture: &Fixture, render: &Render) -> Result<Publication> {
    let name = job(render, "control-plane-publication")?["metadata"]["name"]
        .as_str()
        .context("publication name")?;
    let published: Publication = serde_json::from_slice(&fixture.logs(name)?)?;
    ensure!(
        published.status == "published" || published.status == "unchanged",
        "publication did not settle"
    );
    ensure!(
        published.runtime_auth_verified
            && !published.revision_id.is_empty()
            && !published.sha256.is_empty(),
        "publication lacks authenticated persisted outcome"
    );
    Ok(published)
}
fn credential_confinement(render: &Render) -> Result<()> {
    let mut migrations = 0;
    for o in render.objects.iter().filter(|o| o["kind"] == "Job") {
        let component = o["metadata"]["labels"]["app.kubernetes.io/component"]
            .as_str()
            .context("Job component")?;
        for container in o["spec"]["template"]["spec"]["containers"]
            .as_array()
            .context("Job containers")?
        {
            for env in container["env"].as_array().context("Job env")? {
                if env["valueFrom"]["secretKeyRef"]["name"] == "veoveo-surreal-admin" {
                    ensure!(
                        matches!(component, "installation-prepare" | "module-migration"),
                        "root credential escaped migration Jobs"
                    );
                    migrations += 1;
                }
                if env["name"] == "VEOVEO_SURREAL_AUTH_LEVEL"
                    && component == "control-plane-publication"
                {
                    ensure!(
                        env["value"] == "database",
                        "publication did not use runtime authentication"
                    );
                }
            }
        }
    }
    ensure!(
        migrations > 0,
        "fixture rendered no real migration credential references"
    );
    Ok(())
}

pub(super) fn lifecycle(
    fixture: &mut Fixture,
    cases: &mut Vec<Case>,
    recovery: &mut Option<super::managed::Recovery>,
) -> Result<()> {
    let first = fixture.generate(1, &["time"])?;
    let mut prior: Option<Vec<Lane>> = None;
    for (generation, enabled, name) in [
        (1, &["time"][..], "fresh"),
        (2, &[][..], "rotate-disable"),
        (3, &["time", "media"][..], "later-enable"),
    ] {
        let render = if generation == 1 {
            None
        } else {
            fixture.rotate_credentials()?;
            Some(fixture.generate(generation, enabled)?)
        };
        let render = render.as_ref().unwrap_or(&first);
        credential_confinement(render)?;
        fixture.install(render)?;
        if generation == 2 {
            super::managed::recover(fixture)?;
        }
        let installed = status(fixture, render, generation)?;
        if generation == 1 {
            ensure!(
                lane(&installed.lanes, "time")?.initialized,
                "fresh Time lane missing"
            );
            ensure!(
                !lane(&installed.lanes, "media")?.initialized,
                "disabled Media lane was initialized"
            );
        } else {
            let previous = prior.as_ref().context("previous installed status")?;
            let before = lane(previous, "time")?;
            let after = lane(&installed.lanes, "time")?;
            ensure!(
                before.initialized == after.initialized
                    && before.current == after.current
                    && before.latest == after.latest,
                "disabled/re-enabled Time history changed"
            );
            if generation == 2 {
                ensure!(
                    !after.selected && !lane(&installed.lanes, "media")?.initialized,
                    "disabled optional lane changed"
                );
            }
            if generation == 3 {
                ensure!(
                    lane(&installed.lanes, "media")?.initialized
                        && lane(&installed.lanes, "media")?.selected,
                    "later-enabled Media lane not initialized"
                );
            }
        }
        let published = publication(fixture, render)?;
        let before = fixture.snapshot()?;
        fixture.install(render)?;
        if generation == 2 {
            *recovery = Some(super::managed::replay_and_stop(fixture)?);
        }
        ensure!(
            before == fixture.snapshot()?,
            "unchanged installation recreated live Job/ConfigMap/database objects"
        );
        let replay_name = format!("publication-replay-{generation}");
        let replay: Publication = serde_json::from_slice(&fixture.probe(
            job(render, "control-plane-publication")?,
            &replay_name,
            &[
                "control-plane-publish",
                "--control-plane",
                "/etc/veoveo/gateway/gateway.json",
                "--applied-by",
                "installation-bootstrap",
                "--wait-seconds",
                "60",
            ],
            true,
        )?)?;
        ensure!(
            replay.status == "unchanged"
                && replay.revision_id == published.revision_id
                && replay.runtime_auth_verified,
            "no-op publication replaced its persisted revision"
        );
        let mut rejected = Vec::new();
        if generation == 2 {
            for (kind, args) in [
                (
                    "installation-prepare",
                    vec!["installation-prepare", "--wait-seconds", "5"],
                ),
                (
                    "module-migration",
                    vec!["module-migrate", "--module", "time", "--wait-seconds", "5"],
                ),
                (
                    "control-plane-publication",
                    vec![
                        "control-plane-publish",
                        "--control-plane",
                        "/etc/veoveo/gateway/gateway.json",
                        "--applied-by",
                        "installation-bootstrap",
                        "--wait-seconds",
                        "5",
                    ],
                ),
            ] {
                let output =
                    fixture.probe(job(&first, kind)?, &format!("stale-{kind}"), &args, false)?;
                let text = String::from_utf8(output)?;
                // Logs are not copied to evidence. The error must identify generation fencing,
                // rather than an unrelated crash, image pull or timeout.
                ensure!(
                    text.contains("generation")
                        && (text.contains("stale") || text.contains("newer")),
                    "stale command failed outside generation fencing"
                );
                rejected.push(kind.into());
            }
            let mut old_auth = job(render, "control-plane-publication")?.clone();
            for env in old_auth["spec"]["template"]["spec"]["containers"][0]["env"]
                .as_array_mut()
                .context("runtime probe env")?
            {
                if env["name"] == "VEOVEO_SURREAL_PASSWORD" {
                    env["valueFrom"]["secretKeyRef"]["name"] = "fixture-old-runtime".into();
                }
            }
            let failed = fixture.probe(
                &old_auth,
                "old-runtime-rejected",
                &["control-plane-validate"],
                false,
            )?;
            let diagnostic = String::from_utf8(failed)?.to_ascii_lowercase();
            ensure!(
                diagnostic.contains("authentication") || diagnostic.contains("authenticate"),
                "old runtime account failed outside authentication admission"
            );
            rejected.push("old-runtime-authentication".into());
            let after = status(fixture, render, 20)?; // Probe identity is independent of requested plan generation.
            ensure!(
                after.lanes == installed.lanes,
                "stale Jobs altered current lane history"
            );
            let after_pub: Publication = serde_json::from_slice(&fixture.probe(
                job(render, "control-plane-publication")?,
                "publication-after-stale",
                &[
                    "control-plane-publish",
                    "--control-plane",
                    "/etc/veoveo/gateway/gateway.json",
                    "--applied-by",
                    "installation-bootstrap",
                    "--wait-seconds",
                    "60",
                ],
                true,
            )?)?;
            ensure!(
                after_pub.status == "unchanged"
                    && after_pub.revision_id == published.revision_id
                    && after_pub.sha256 == published.sha256
                    && after_pub.revisions == published.revisions
                    && after_pub.active_objects == published.active_objects,
                "stale Jobs altered current persisted publication"
            );
        }
        cases.push(Case {
            name: name.into(),
            generation: generation.to_string(),
            lanes: installed.lanes.clone(),
            jobs: before,
            runtime_auth_verified: published.runtime_auth_verified,
            publication_revision: published.revision_id,
            publication_sha256: published.sha256,
            no_op_reused: true,
            stale_commands_rejected: rejected,
            observed_image_ids: fixture.image_ids()?,
        });
        prior = Some(installed.lanes);
        if generation == 1 {
            super::managed::start(fixture)?;
        }
    }
    Ok(())
}
