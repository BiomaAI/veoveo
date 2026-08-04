use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Command, ExitStatus, Stdio},
};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PhaseTimings {
    pub(crate) buildkit_millis: u64,
    pub(crate) compile_millis: u64,
    pub(crate) sbom_millis: u64,
    pub(crate) provenance_millis: u64,
    pub(crate) timestamp_normalization_millis: u64,
    pub(crate) export_millis: u64,
    pub(crate) push_millis: u64,
    pub(crate) executed_vertices: u64,
    pub(crate) cached_vertices: u64,
}

#[derive(Clone, Copy, Debug, Default)]
struct TimeWindow {
    first_millis: Option<i64>,
    last_millis: Option<i64>,
}

impl TimeWindow {
    fn include(&mut self, started: &str, completed: &str) {
        let Some(started) = timestamp_millis(started) else {
            return;
        };
        let Some(completed) = timestamp_millis(completed) else {
            return;
        };
        self.first_millis = Some(
            self.first_millis
                .map_or(started, |value| value.min(started)),
        );
        self.last_millis = Some(
            self.last_millis
                .map_or(completed, |value| value.max(completed)),
        );
    }

    fn duration_millis(self) -> u64 {
        match (self.first_millis, self.last_millis) {
            (Some(first), Some(last)) => u64::try_from(last.saturating_sub(first)).unwrap_or(0),
            _ => 0,
        }
    }
}

#[derive(Default)]
struct TraceSummary {
    all: TimeWindow,
    compile: TimeWindow,
    sbom: TimeWindow,
    provenance: TimeWindow,
    timestamp_normalization: TimeWindow,
    export: TimeWindow,
    push: TimeWindow,
    completed_vertices: BTreeMap<String, bool>,
}

impl TraceSummary {
    fn observe(&mut self, value: &Value) {
        if let Some(vertexes) = value.get("vertexes").and_then(Value::as_array) {
            for vertex in vertexes {
                let Some(digest) = vertex.get("digest").and_then(Value::as_str) else {
                    continue;
                };
                let Some(started) = vertex.get("started").and_then(Value::as_str) else {
                    continue;
                };
                let Some(completed) = vertex.get("completed").and_then(Value::as_str) else {
                    continue;
                };
                let name = vertex.get("name").and_then(Value::as_str).unwrap_or("");
                let cached = vertex
                    .get("cached")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                self.completed_vertices.insert(digest.to_owned(), cached);
                self.observe_window(name, started, completed);
            }
        }
        if let Some(statuses) = value.get("statuses").and_then(Value::as_array) {
            for status in statuses {
                let Some(started) = status.get("started").and_then(Value::as_str) else {
                    continue;
                };
                let Some(completed) = status.get("completed").and_then(Value::as_str) else {
                    continue;
                };
                let name = status
                    .get("id")
                    .and_then(Value::as_str)
                    .or_else(|| status.get("name").and_then(Value::as_str))
                    .unwrap_or("");
                self.observe_window(name, started, completed);
            }
        }
    }

    fn observe_window(&mut self, name: &str, started: &str, completed: &str) {
        self.all.include(started, completed);
        let name = name.to_ascii_lowercase();
        if name.contains("cargo build")
            || name.contains("cargo \"")
            || name.contains("veoveo_cargo_packages")
        {
            self.compile.include(started, completed);
        }
        if name.contains("sbom") || name.contains("spdx") {
            self.sbom.include(started, completed);
        }
        if name.contains("provenance") || name.contains("slsa") {
            self.provenance.include(started, completed);
        }
        if name.contains("rewriting layers with source-date-epoch") {
            self.timestamp_normalization.include(started, completed);
        }
        if name.contains("exporting") {
            self.export.include(started, completed);
        }
        if name.contains("pushing") || name.contains("uploading") {
            self.push.include(started, completed);
        }
    }

    fn finish(self) -> PhaseTimings {
        let cached_vertices = self
            .completed_vertices
            .values()
            .filter(|cached| **cached)
            .count();
        PhaseTimings {
            buildkit_millis: self.all.duration_millis(),
            compile_millis: self.compile.duration_millis(),
            sbom_millis: self.sbom.duration_millis(),
            provenance_millis: self.provenance.duration_millis(),
            timestamp_normalization_millis: self.timestamp_normalization.duration_millis(),
            export_millis: self.export.duration_millis(),
            push_millis: self.push.duration_millis(),
            executed_vertices: u64::try_from(self.completed_vertices.len()).unwrap_or(u64::MAX),
            cached_vertices: u64::try_from(cached_vertices).unwrap_or(u64::MAX),
        }
    }
}

pub(crate) fn execute(
    command: &mut Command,
    trace_path: &Path,
) -> Result<(ExitStatus, PhaseTimings)> {
    let mut trace = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(trace_path)
        .with_context(|| format!("creating BuildKit trace {}", trace_path.display()))?;
    let mut child = command
        .arg("--progress=rawjson")
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .context("starting Docker Buildx Bake")?;
    let stderr = child.stderr.take().context("capturing Buildx progress")?;
    let mut summary = TraceSummary::default();
    for line in BufReader::new(stderr).lines() {
        let line = line.context("reading BuildKit progress")?;
        trace.write_all(line.as_bytes())?;
        trace.write_all(b"\n")?;
        match serde_json::from_str::<Value>(&line) {
            Ok(value) => {
                summary.observe(&value);
                emit_logs(&value);
                emit_terminal_vertex(&value);
            }
            Err(_) => eprintln!("{line}"),
        }
    }
    let status = child.wait().context("waiting for Docker Buildx Bake")?;
    Ok((status, summary.finish()))
}

fn emit_logs(value: &Value) {
    let Some(logs) = value.get("logs").and_then(Value::as_array) else {
        return;
    };
    let mut stderr = std::io::stderr().lock();
    for log in logs {
        let Some(data) = log.get("data").and_then(Value::as_str) else {
            continue;
        };
        if let Ok(decoded) = STANDARD.decode(data) {
            let _ = stderr.write_all(&decoded);
        }
    }
}

fn emit_terminal_vertex(value: &Value) {
    let Some(vertexes) = value.get("vertexes").and_then(Value::as_array) else {
        return;
    };
    for vertex in vertexes {
        if vertex.get("completed").is_none() {
            continue;
        }
        let Some(name) = vertex.get("name").and_then(Value::as_str) else {
            continue;
        };
        if let Some(error) = vertex.get("error").and_then(Value::as_str) {
            eprintln!("BuildKit failed: {name}: {error}");
        }
    }
}

fn timestamp_millis(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|timestamp| timestamp.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_buildkit_phase_windows_without_double_counting_updates() {
        let mut summary = TraceSummary::default();
        for line in [
            r#"{"vertexes":[{"digest":"compile","name":"RUN cargo build --release","started":"2026-08-04T00:00:00Z","completed":"2026-08-04T00:00:02Z","cached":false}]}"#,
            r#"{"vertexes":[{"digest":"compile","name":"RUN cargo build --release","started":"2026-08-04T00:00:00Z","completed":"2026-08-04T00:00:02Z","cached":false}]}"#,
            r#"{"statuses":[{"id":"rewriting layers with source-date-epoch","started":"2026-08-04T00:00:02Z","completed":"2026-08-04T00:00:03.500Z"}]}"#,
            r#"{"vertexes":[{"digest":"export","name":"exporting to image","started":"2026-08-04T00:00:02Z","completed":"2026-08-04T00:00:05Z","cached":true}]}"#,
        ] {
            summary.observe(&serde_json::from_str(line).unwrap());
        }
        let result = summary.finish();
        assert_eq!(result.buildkit_millis, 5_000);
        assert_eq!(result.compile_millis, 2_000);
        assert_eq!(result.timestamp_normalization_millis, 1_500);
        assert_eq!(result.export_millis, 3_000);
        assert_eq!(result.executed_vertices, 2);
        assert_eq!(result.cached_vertices, 1);
    }
}
