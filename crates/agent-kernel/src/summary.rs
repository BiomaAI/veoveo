//! Deterministic episode summaries: cheap, honest, and always available.
//!
//! A summary compresses one episode into a single ledger line future context
//! assembly can afford. The deterministic template needs no model call; an
//! LLM summarization mode can layer on later with this as its fallback.

use crate::episode::EpisodeReport;

pub fn deterministic(report: &EpisodeReport, wake_note: &str, tool_calls: u64) -> String {
    let mut output_excerpt: String = report.output.chars().take(400).collect();
    if report.output.chars().count() > 400 {
        output_excerpt.push('…');
    }
    format!(
        "episode {seq} (wake: {wake_note}): {tool_calls} tool call(s), {detached} task(s) \
         detached; final: {output_excerpt}",
        seq = report.seq,
        detached = report.detached_tasks,
    )
}
