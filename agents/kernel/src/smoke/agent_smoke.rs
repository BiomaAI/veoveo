//! Deterministic owner fixture for the maintained OpenAI completion boundary.
use anyhow::Result;
use axum::{Json as AxumJson, Router as AxumRouter, routing::post as axum_post};
use clap::Parser;
use serde_json::{Value, json};
use std::path::PathBuf;

#[path = "utility/cli.rs"]
mod cli;
#[path = "utility/fake_services.rs"]
mod fake_services;
use cli::{Args, Cmd};
use fake_services::cmd_fake_openai_llm;

#[tokio::main]
async fn main() -> Result<()> {
    veoveo_testing_support::lifecycle::owner::run(execute()).await
}
async fn execute() -> Result<()> {
    match Args::parse().cmd {
        Cmd::FakeOpenaiLlm { port, ready_file } => cmd_fake_openai_llm(port, ready_file).await,
    }
}
