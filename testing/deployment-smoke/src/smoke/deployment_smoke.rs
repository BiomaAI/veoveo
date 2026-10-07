//! Typed deployment-plan validation utility.
use anyhow::Result;
use clap::Parser;
use veoveo_mcp_contract::SelfHostedDeploymentPlan;

#[path = "utility/cli.rs"]
mod cli;
use cli::{Args, Cmd};

#[tokio::main]
async fn main() -> Result<()> {
    veoveo_testing_support::lifecycle::owner::run(execute()).await
}

async fn execute() -> Result<()> {
    match Args::parse().cmd {
        Cmd::DeploymentValidate { file } => {
            let plan = SelfHostedDeploymentPlan::load_json(&file)?;
            println!("ok: {} deployment profile(s)", plan.profiles.len());
            Ok(())
        }
    }
}
