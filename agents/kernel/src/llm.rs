//! Episode LLM construction from manifest model config.
//!
//! Any OpenAI-compatible chat-completions endpoint works — Cloudflare Workers
//! AI in production, a scripted fake in smoke. The completions API client is
//! used explicitly: the responses API is an OpenAI-only dialect.

use anyhow::{Context, Result};
use rig_core::{
    agent::Agent, client::CompletionClient, providers::openai::CompletionsClient,
    tool::server::ToolServerHandle,
};

use crate::manifest::AgentManifest;

/// The concrete completion model every kernel agent runs on.
pub type KernelModel = <CompletionsClient as CompletionClient>::CompletionModel;

pub fn build_agent(
    manifest: &AgentManifest,
    tool_server_handle: ToolServerHandle,
) -> Result<Agent<KernelModel>> {
    let api_key = std::env::var(&manifest.model.api_key_env).with_context(|| {
        format!(
            "model api key env `{}` is not set",
            manifest.model.api_key_env
        )
    })?;
    let client = CompletionsClient::builder()
        .api_key(&api_key)
        .base_url(&manifest.model.base_url)
        .build()
        .context("building completions client")?;
    let mut builder = client
        .agent(&manifest.model.model)
        .name(&manifest.agent.display_name)
        .preamble(&manifest.preamble)
        .default_max_turns(manifest.episode.max_turns)
        .tool_server_handle(tool_server_handle);
    if let Some(temperature) = manifest.model.temperature {
        builder = builder.temperature(temperature);
    }
    if let Some(max_tokens) = manifest.model.max_output_tokens {
        builder = builder.max_tokens(max_tokens);
    }
    Ok(builder.build())
}
