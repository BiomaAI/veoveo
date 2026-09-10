//! Print the canonical retained-template fingerprint for installation inputs.
use std::{error::Error, fs};
use veoveo_computers_runtime::{
    DevelopmentTemplate, PERSISTENT_COMMAND, PersistentHome, parse_policy,
};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 6 {
        return Err(
            "usage: retained_template IMAGE CPUS MEMORY_MIB HOME_MIB TEMPORARY_MIB POLICY_JSON"
                .into(),
        );
    }
    let policy = fs::read(&args[5])?;
    if policy.len() > 1024 * 1024 {
        return Err("policy exceeds 1 MiB".into());
    }
    let template = DevelopmentTemplate::new(
        args[0].clone(),
        args[1].parse()?,
        args[2].parse()?,
        parse_policy(&serde_json::from_slice(&policy)?)?,
        PERSISTENT_COMMAND.map(str::to_owned).into(),
        Some(PersistentHome::new(args[3].parse()?, args[4].parse()?)?),
    )?;
    println!("{}", template.fingerprint());
    Ok(())
}
