use std::{env, process::ExitCode};

use veoveo_deploy_contract::{deployment_lock_schema, deployment_profile_schema};

fn main() -> ExitCode {
    let Some(name) = env::args().nth(1) else {
        eprintln!("usage: veoveo-deployment-schema <profile|lock>");
        return ExitCode::from(2);
    };
    let schema = match name.as_str() {
        "profile" => deployment_profile_schema(),
        "lock" => deployment_lock_schema(),
        _ => {
            eprintln!("unknown deployment schema {name:?}");
            return ExitCode::from(2);
        }
    };
    match serde_json::to_string_pretty(&schema) {
        Ok(document) => {
            println!("{document}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("encoding deployment schema: {error}");
            ExitCode::FAILURE
        }
    }
}
