fn main() -> Result<(), Box<dyn std::error::Error>> {
    let stdout = std::io::stdout();
    serde_json::to_writer_pretty(stdout.lock(), &veoveo_computers_contract::schema_bundle())?;
    println!();
    Ok(())
}
