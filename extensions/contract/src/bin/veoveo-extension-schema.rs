use std::{env, process::ExitCode};

use veoveo_extension_contract::{compatibility_manifest_schema, extension_release_schema};

fn main() -> ExitCode {
    let Some(schema) = env::args().nth(1) else {
        eprintln!("usage: veoveo-extension-schema <compatibility-manifest|extension-release>");
        return ExitCode::from(2);
    };
    let document = match schema.as_str() {
        "compatibility-manifest" => compatibility_manifest_schema(),
        "extension-release" => extension_release_schema(),
        _ => {
            eprintln!("unknown schema {schema:?}");
            return ExitCode::from(2);
        }
    };
    match serde_json::to_string_pretty(&document) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("encoding schema: {error}");
            ExitCode::FAILURE
        }
    }
}
