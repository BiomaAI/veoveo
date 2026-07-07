use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{ArtifactMetadata, duckdb::DuckDbSource};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanRequest {
    pub input: PlanInput,
    #[serde(default)]
    pub objective: PlanningObjective,
    #[serde(default)]
    pub artifacts: PlanArtifactOptions,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanInput {
    Inline {
        agents: Vec<PlanningAgent>,
        tasks: Vec<PlanningTask>,
        options: Vec<PlanningOption>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<PlanningConstraint>,
    },
    DuckDbOptions {
        source: DuckDbSource,
        #[serde(default)]
        mapping: PlanningTableMapping,
        agents: Vec<PlanningAgent>,
        tasks: Vec<PlanningTask>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<PlanningConstraint>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningAgent {
    pub id: String,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_limits: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_options: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningTask {
    pub id: String,
    #[serde(default = "default_required_count")]
    pub required_count: u32,
    #[serde(default = "default_task_priority")]
    pub priority: f64,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub required_capabilities: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningOption {
    pub id: String,
    pub task_id: String,
    pub agent_ids: Vec<String>,
    #[serde(default)]
    pub cost: f64,
    #[serde(default)]
    pub risk: f64,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub resource_usage: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub excludes: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub tags: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlanningConstraint {
    ResourceLimit {
        resource: String,
        limit: f64,
    },
    MutualExclusion {
        option_ids: Vec<String>,
    },
    Dependency {
        option_id: String,
        depends_on: String,
    },
    MaxSelected {
        option_ids: Vec<String>,
        max: u32,
    },
    MinSelected {
        option_ids: Vec<String>,
        min: u32,
    },
    AgentMaxOptions {
        agent_id: String,
        max: u32,
    },
    TaskRequirement {
        task_id: String,
        min: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningObjective {
    #[serde(default = "default_cost_weight")]
    pub cost_weight: f64,
    #[serde(default = "default_risk_weight")]
    pub risk_weight: f64,
    #[serde(default)]
    pub duration_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub priority_weight: f64,
    #[serde(default = "default_confidence_weight")]
    pub confidence_weight: f64,
    #[serde(default)]
    pub resource_weight: f64,
}

impl Default for PlanningObjective {
    fn default() -> Self {
        Self {
            cost_weight: default_cost_weight(),
            risk_weight: default_risk_weight(),
            duration_weight: 0.0,
            priority_weight: default_priority_weight(),
            confidence_weight: default_confidence_weight(),
            resource_weight: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanArtifactOptions {
    #[serde(default = "default_true")]
    pub duckdb: bool,
    #[serde(default = "default_true")]
    pub rerun_rrd: bool,
}

impl Default for PlanArtifactOptions {
    fn default() -> Self {
        Self {
            duckdb: true,
            rerun_rrd: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PlanningTableMapping {
    #[serde(default = "default_option_id_column")]
    pub option_id_column: String,
    #[serde(default = "default_task_id_column")]
    pub task_id_column: String,
    #[serde(
        default = "default_agent_id_column",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent_id_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_ids_column: Option<String>,
    #[serde(
        default = "default_cost_column",
        skip_serializing_if = "Option::is_none"
    )]
    pub cost_column: Option<String>,
    #[serde(
        default = "default_risk_column",
        skip_serializing_if = "Option::is_none"
    )]
    pub risk_column: Option<String>,
    #[serde(
        default = "default_confidence_column",
        skip_serializing_if = "Option::is_none"
    )]
    pub confidence_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_usage_json_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_json_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub excludes_json_column: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags_json_column: Option<String>,
}

impl Default for PlanningTableMapping {
    fn default() -> Self {
        Self {
            option_id_column: default_option_id_column(),
            task_id_column: default_task_id_column(),
            agent_id_column: default_agent_id_column(),
            agent_ids_column: None,
            cost_column: default_cost_column(),
            risk_column: default_risk_column(),
            confidence_column: default_confidence_column(),
            duration_column: None,
            start_column: None,
            end_column: None,
            resource_usage_json_column: None,
            requires_json_column: None,
            excludes_json_column: None,
            tags_json_column: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Optimal,
    Infeasible,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanOutput {
    pub status: PlanStatus,
    pub summary: PlanSummary,
    pub selected_options: Vec<SelectedOption>,
    pub task_results: Vec<TaskPlanResult>,
    pub agent_results: Vec<AgentPlanResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub objective_value: Option<f64>,
    pub solver: PlanSolverSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duckdb_artifact: Option<ArtifactMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rrd_artifact: Option<ArtifactMetadata>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanSummary {
    pub agents: u64,
    pub tasks: u64,
    pub options: u64,
    pub selected: u64,
    pub completed_tasks: u64,
    pub total_cost: f64,
    pub total_risk: f64,
    pub total_confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SelectedOption {
    pub option_id: String,
    pub task_id: String,
    pub agent_ids: Vec<String>,
    pub score: f64,
    pub cost: f64,
    pub risk: f64,
    pub confidence: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TaskPlanResult {
    pub task_id: String,
    pub required_count: u32,
    pub selected_count: u32,
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgentPlanResult {
    pub agent_id: String,
    pub selected_count: u32,
    pub resource_usage: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanSolverSummary {
    pub backend: String,
    pub variables: u64,
    pub constraints: u64,
    pub message: String,
}

fn default_required_count() -> u32 {
    1
}

fn default_task_priority() -> f64 {
    1.0
}

fn default_confidence() -> f64 {
    1.0
}

fn default_cost_weight() -> f64 {
    1.0
}

fn default_risk_weight() -> f64 {
    1.0
}

fn default_priority_weight() -> f64 {
    1.0
}

fn default_confidence_weight() -> f64 {
    0.25
}

fn default_true() -> bool {
    true
}

fn default_option_id_column() -> String {
    "option_id".to_string()
}

fn default_task_id_column() -> String {
    "task_id".to_string()
}

fn default_agent_id_column() -> Option<String> {
    Some("agent_id".to_string())
}

fn default_cost_column() -> Option<String> {
    Some("cost".to_string())
}

fn default_risk_column() -> Option<String> {
    Some("risk".to_string())
}

fn default_confidence_column() -> Option<String> {
    Some("confidence".to_string())
}
