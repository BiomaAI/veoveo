use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use duckdb::{Connection, params};
use re_sdk::RecordingStreamBuilder;
use re_sdk_types::archetypes::{Scalars, TextDocument};
use serde_json::{Value, json};

use crate::contract::GovernedPlan;

pub const PLAN_JSON_MIME_TYPE: &str = "application/vnd.veoveo.optimization-plan+json";
pub const PLAN_JSON_FILENAME: &str = "plan.json";
pub const RRD_MIME_TYPE: &str = "application/vnd.veoveo.rerun-rrd";
pub const RRD_FILENAME: &str = "plan.rrd";
pub const DUCKDB_MIME_TYPE: &str = "application/vnd.duckdb";
pub const DUCKDB_FILENAME: &str = "plan.duckdb";

#[derive(Debug, Clone)]
pub struct PlanArtifactBytes {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub filename: &'static str,
    pub metadata: Value,
}

pub fn encode_plan_json(plan: &GovernedPlan) -> Result<PlanArtifactBytes> {
    Ok(PlanArtifactBytes {
        bytes: serde_json::to_vec(plan).context("serializing canonical governed plan")?,
        mime_type: PLAN_JSON_MIME_TYPE,
        filename: PLAN_JSON_FILENAME,
        metadata: artifact_metadata(plan, "veoveo_optimization_plan_json"),
    })
}

pub fn encode_duckdb(plan: &GovernedPlan) -> Result<PlanArtifactBytes> {
    let path = temporary_file("veoveo-optimization-plan", "duckdb");
    let result = (|| -> Result<Vec<u8>> {
        let connection =
            Connection::open(&path).with_context(|| format!("opening {}", path.display()))?;
        connection.execute_batch(
            r#"
            CREATE TABLE plan_assignment (
                assignment_id VARCHAR NOT NULL,
                task_id VARCHAR NOT NULL,
                ordinal UBIGINT NOT NULL,
                agent_ids_json JSON NOT NULL,
                group_id VARCHAR,
                mobility_profiles_json JSON NOT NULL,
                target_json JSON NOT NULL,
                execution_json JSON NOT NULL,
                lane_id VARCHAR,
                resource_band_id VARCHAR,
                timing_json JSON NOT NULL,
                recurrence_json JSON NOT NULL,
                shared_resources_json JSON NOT NULL,
                cost DOUBLE NOT NULL,
                risk DOUBLE NOT NULL,
                confidence DOUBLE NOT NULL
            );
            CREATE TABLE plan_requirement (
                task_id VARCHAR NOT NULL,
                minimum_quantity UBIGINT NOT NULL,
                desired_quantity UBIGINT NOT NULL,
                assigned_quantity UBIGINT NOT NULL,
                satisfaction VARCHAR NOT NULL
            );
            CREATE TABLE governed_plan (
                plan_id VARCHAR NOT NULL,
                resource_uri VARCHAR NOT NULL,
                status VARCHAR NOT NULL,
                plan_digest_sha256 VARCHAR NOT NULL,
                request_digest_sha256 VARCHAR NOT NULL,
                algorithm_revision VARCHAR NOT NULL,
                canonical_json JSON NOT NULL
            );
            "#,
        )?;
        for assignment in &plan.assignments {
            connection.execute(
                r#"
                INSERT INTO plan_assignment VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
                    ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16
                )
                "#,
                params![
                    assignment.assignment_id.as_str(),
                    assignment.task_id.as_str(),
                    u64::from(assignment.ordinal),
                    serde_json::to_string(&assignment.agent_ids)?,
                    assignment.group_id.as_ref().map(ToString::to_string),
                    serde_json::to_string(&assignment.mobility_profiles)?,
                    serde_json::to_string(&assignment.target)?,
                    serde_json::to_string(&assignment.execution)?,
                    assignment.lane_id.as_ref().map(ToString::to_string),
                    assignment
                        .resource_band_id
                        .as_ref()
                        .map(ToString::to_string),
                    serde_json::to_string(&assignment.timing)?,
                    serde_json::to_string(&assignment.recurrence)?,
                    serde_json::to_string(&assignment.shared_resources)?,
                    assignment.cost,
                    assignment.risk,
                    assignment.confidence,
                ],
            )?;
        }
        for requirement in &plan.requirements {
            connection.execute(
                "INSERT INTO plan_requirement VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    requirement.task_id.as_str(),
                    u64::from(requirement.minimum_quantity),
                    u64::from(requirement.desired_quantity),
                    u64::from(requirement.assigned_quantity),
                    enum_wire(requirement.satisfaction)?,
                ],
            )?;
        }
        connection.execute(
            "INSERT INTO governed_plan VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                plan.plan_id.as_str(),
                plan.resource_uri,
                enum_wire(plan.status)?,
                plan.plan_digest_sha256,
                plan.request_digest_sha256,
                plan.algorithm_revision,
                serde_json::to_string(plan)?,
            ],
        )?;
        connection
            .execute_batch("CHECKPOINT;")
            .context("checkpointing governed plan DuckDB")?;
        fs::read(&path).with_context(|| format!("reading {}", path.display()))
    })();
    remove_duckdb_files(&path);
    Ok(PlanArtifactBytes {
        bytes: result?,
        mime_type: DUCKDB_MIME_TYPE,
        filename: DUCKDB_FILENAME,
        metadata: artifact_metadata(plan, "duckdb"),
    })
}

pub fn encode_rrd(plan: &GovernedPlan) -> Result<PlanArtifactBytes> {
    let path = temporary_file("veoveo-optimization-plan", "rrd");
    let result = (|| -> Result<Vec<u8>> {
        let recording = RecordingStreamBuilder::new("veoveo_optimization_plan")
            .recording_id(plan.plan_id.to_string())
            .recording_name(format!("plan {}", plan.plan_id))
            .save(&path)
            .context("opening governed plan Rerun sink")?;
        recording.log(
            "/optimization/plan",
            &TextDocument::new(serde_json::to_string_pretty(plan)?)
                .with_media_type(PLAN_JSON_MIME_TYPE),
        )?;
        recording.log(
            "/optimization/metrics/assignments",
            &Scalars::single(plan.metrics.assignments as f64),
        )?;
        recording.log(
            "/optimization/metrics/total_cost",
            &Scalars::single(plan.metrics.total_cost),
        )?;
        recording.log(
            "/optimization/metrics/total_risk",
            &Scalars::single(plan.metrics.total_risk),
        )?;
        for (index, assignment) in plan.assignments.iter().enumerate() {
            recording.set_time_sequence("plan_assignment", index as i64);
            let segment = entity_segment(assignment.assignment_id.as_str());
            recording.log(
                format!("/plans/{}/assignments/{segment}", plan.plan_id),
                &TextDocument::new(serde_json::to_string_pretty(assignment)?)
                    .with_media_type("application/json"),
            )?;
        }
        recording.flush_blocking().context("flushing plan RRD")?;
        drop(recording);
        fs::read(&path).with_context(|| format!("reading {}", path.display()))
    })();
    let _ = fs::remove_file(&path);
    Ok(PlanArtifactBytes {
        bytes: result?,
        mime_type: RRD_MIME_TYPE,
        filename: RRD_FILENAME,
        metadata: artifact_metadata(plan, "rerun_rrd"),
    })
}

fn artifact_metadata(plan: &GovernedPlan, format: &str) -> Value {
    json!({
        "artifact_format": format,
        "plan_id": plan.plan_id,
        "plan_uri": plan.resource_uri,
        "plan_digest_sha256": plan.plan_digest_sha256,
        "request_digest_sha256": plan.request_digest_sha256,
        "algorithm_revision": plan.algorithm_revision,
        "source_map_releases": plan.source_map_releases,
        "frame_world_revision": plan.frame_world_revision,
        "mobility_profiles": plan.mobility_profiles,
    })
}

fn enum_wire(value: impl serde::Serialize) -> Result<String> {
    serde_json::to_value(value)?
        .as_str()
        .map(ToOwned::to_owned)
        .context("enum has no string wire value")
}

fn temporary_file(prefix: &str, extension: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}.{}",
        std::process::id(),
        uuid::Uuid::now_v7(),
        extension
    ))
}

fn remove_duckdb_files(path: &PathBuf) {
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(PathBuf::from(format!("{}.wal", path.to_string_lossy())));
}

fn entity_segment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
