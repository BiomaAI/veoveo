//! Per-episode context assembly: the prompt is a view over the memory planes.
//!
//! Nothing rides in model history between episodes. Each episode's user
//! prompt is rebuilt from a fixed state header, the wake that started it, the
//! manifest's SQL-backed sections (priority order, individually truncated),
//! and a fixed operating footer — all under an approximate token budget.

use anyhow::Result;

use crate::{
    manifest::{AgentManifest, ContextSection},
    memory::MemoryStore,
};

/// chars/4 plus slack: deliberately approximate. The budget protects the
/// context window, it does not meter billing.
pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64) / 4 + 8
}

pub fn assemble(
    manifest: &AgentManifest,
    memory: &MemoryStore,
    wake_body: &str,
    pending_tasks: usize,
    unconsumed_results: usize,
) -> Result<String> {
    let budget = manifest.context.max_context_tokens;
    let mut spent = 0u64;
    let mut parts: Vec<String> = Vec::new();

    let header = state_header(pending_tasks, unconsumed_results);
    spent += estimate_tokens(&header);
    parts.push(header);

    let wake = format!("## Wake\n\n{wake_body}");
    spent += estimate_tokens(&wake);
    parts.push(wake);

    let mut sections: Vec<&ContextSection> = manifest.context.sections.iter().collect();
    sections.sort_by_key(|section| section.priority);
    for section in sections {
        let rendered = match render_section(memory, section) {
            Ok(rendered) => rendered,
            Err(err) => {
                tracing::warn!(section = section.name, %err, "context section failed");
                format!("## {}\n\n(section unavailable: {err:#})", section.name)
            }
        };
        let cost = estimate_tokens(&rendered);
        if spent + cost > budget {
            parts.push(format!(
                "## {}\n\n(omitted for context budget — use memory_query)",
                section.name
            ));
            continue;
        }
        spent += cost;
        parts.push(rendered);
    }

    parts.push(
        "## Operating rules\n\nAct using your tools. Long-running gateway tools are dispatched \
         as background tasks — you will be woken with their results; never wait or poll for \
         them. Record durable conclusions with memory_write. End your turn when nothing \
         actionable remains."
            .to_string(),
    );
    Ok(parts.join("\n\n"))
}

fn state_header(pending_tasks: usize, unconsumed_results: usize) -> String {
    format!(
        "## State\n\nUTC now: {}. Background tasks in flight: {pending_tasks}. Task results awaiting \
         you: {unconsumed_results}.",
        chrono::Utc::now().format("%Y-%m-%d %H:%M:%S")
    )
}

fn render_section(memory: &MemoryStore, section: &ContextSection) -> Result<String> {
    let rows = memory.query_json(&section.sql, section.max_rows)?;
    let mut body = String::new();
    let mut spent = 0u64;
    let mut shown = 0usize;
    for row in &rows {
        let line = row.to_string();
        let cost = estimate_tokens(&line);
        if spent + cost > section.max_tokens {
            break;
        }
        spent += cost;
        shown += 1;
        body.push_str(&line);
        body.push('\n');
    }
    if body.is_empty() {
        body.push_str("(no rows)\n");
    }
    if shown < rows.len() {
        body.push_str(&format!(
            "(+{} more rows — use memory_query)\n",
            rows.len() - shown
        ));
    }
    Ok(format!("## {}\n\n{}", section.name, body.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate_scales_with_length() {
        assert!(estimate_tokens("word") < estimate_tokens(&"long text ".repeat(100)));
    }
}
