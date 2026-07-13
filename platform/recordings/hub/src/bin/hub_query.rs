//! hub-query: snapshot read-back over a spool tree via the local QueryEngine.
//! Prints `{rows_by_recording, total_rows}` as JSON — the authoritative file
//! reader the smokes assert against (independent of the catalog server).

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "hub-query", about = "Query spooled segments via QueryEngine")]
struct Args {
    /// Root of the spool tree (or any subtree) to query.
    #[arg(long)]
    root: PathBuf,
    /// Entity path filter (Rerun filter syntax).
    #[arg(long, default_value = "/**")]
    entities: String,
    /// Index timeline to order by.
    #[arg(long, default_value = "tick")]
    timeline: String,
    #[arg(long, default_value_t = 1_000_000)]
    max_rows: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let result = veoveo_recording_hub::query_tree(
        &args.root,
        &args.entities,
        &args.timeline,
        args.max_rows,
    )?;
    let total: u64 = result.rows_by_recording.values().sum();
    let out = serde_json::json!({
        "rows_by_recording": result.rows_by_recording,
        "total_rows": total,
    });
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}
