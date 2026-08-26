use std::{ffi::OsString, path::Path};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{ReleasePreflightArgs, commands::builder, context::RepositoryContext, process};

const GIB: u128 = 1024 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FilesystemSpace {
    total_bytes: u128,
    available_bytes: u128,
    used_percent: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct StorageBudget {
    reserve_bytes: u128,
    expected_growth_bytes: u128,
    required_available_bytes: u128,
    remaining_bytes: Option<u128>,
}

pub(crate) fn run(repository: &RepositoryContext, args: &ReleasePreflightArgs) -> Result<()> {
    let filesystem = filesystem_space(repository.root())?;
    let budget = storage_budget(
        filesystem,
        args.expected_growth_gib,
        args.reserve_free_percent,
    );
    let mut failures = Vec::new();

    println!(
        "{} host filesystem: {} GiB available of {} GiB ({}% used)",
        if budget.remaining_bytes.is_some() {
            "PASS"
        } else {
            "FAIL"
        },
        gib(filesystem.available_bytes),
        gib(filesystem.total_bytes),
        filesystem.used_percent,
    );
    println!(
        "INFO release budget: {} GiB expected growth plus {} GiB retained free ({}%)",
        gib(budget.expected_growth_bytes),
        gib(budget.reserve_bytes),
        args.reserve_free_percent,
    );
    match budget.remaining_bytes {
        Some(remaining) => println!(
            "PASS estimated post-build headroom: {} GiB above the retained reserve",
            gib(remaining)
        ),
        None => failures.push(format!(
            "host needs {} GiB available for this release budget but has {} GiB",
            gib(budget.required_available_bytes),
            gib(filesystem.available_bytes)
        )),
    }

    report_builder_cache(repository);

    if let Some(node) = args.kubernetes_node.as_deref() {
        check_node(repository, node, &args.namespace, &mut failures);
    } else {
        println!("HINT pass --kubernetes-node <name> to require Ready=True and DiskPressure=False");
    }

    println!(
        "HINT use `cargo xtask image affected --since <revision>` before selecting release groups"
    );
    println!(
        "HINT reclaim only rebuildable BuildKit cache when space is short; retain registry artifacts, PVCs, and rollback images"
    );
    println!(
        "HINT repository-recorded Cargo checks default to four build jobs; override CARGO_BUILD_JOBS only after measuring memory"
    );

    if failures.is_empty() {
        println!("Release resource preflight passed");
        Ok(())
    } else {
        for failure in &failures {
            eprintln!("FAIL {failure}");
        }
        bail!(
            "release resource preflight failed with {} unsafe condition(s)",
            failures.len()
        )
    }
}

fn filesystem_space(path: &Path) -> Result<FilesystemSpace> {
    let arguments = [
        OsString::from("--block-size=1"),
        OsString::from("--output=size,avail,pcent"),
        path.as_os_str().to_owned(),
    ];
    let output = process::output_text("df", arguments, None)?;
    parse_df(&output)
}

fn parse_df(output: &str) -> Result<FilesystemSpace> {
    let row = output
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .context("df returned no filesystem row")?;
    let fields = row.split_ascii_whitespace().collect::<Vec<_>>();
    if fields.len() != 3 {
        bail!("unexpected df filesystem row `{}`", row.trim())
    }
    let total_bytes = fields[0].parse().context("parsing df total bytes")?;
    let available_bytes = fields[1].parse().context("parsing df available bytes")?;
    let used_percent = fields[2]
        .strip_suffix('%')
        .context("df used percentage has no percent suffix")?
        .parse()
        .context("parsing df used percentage")?;
    Ok(FilesystemSpace {
        total_bytes,
        available_bytes,
        used_percent,
    })
}

fn storage_budget(
    filesystem: FilesystemSpace,
    expected_growth_gib: u64,
    reserve_free_percent: u8,
) -> StorageBudget {
    let reserve_bytes = filesystem
        .total_bytes
        .saturating_mul(u128::from(reserve_free_percent))
        / 100;
    let expected_growth_bytes = u128::from(expected_growth_gib).saturating_mul(GIB);
    let required_available_bytes = reserve_bytes.saturating_add(expected_growth_bytes);
    StorageBudget {
        reserve_bytes,
        expected_growth_bytes,
        required_available_bytes,
        remaining_bytes: filesystem
            .available_bytes
            .checked_sub(required_available_bytes),
    }
}

fn report_builder_cache(repository: &RepositoryContext) {
    let container = format!("buildx_buildkit_{}0", builder::BUILDER_NAME);
    let output = process::output_text(
        "docker",
        ["exec", container.as_str(), "buildctl", "du"],
        Some(repository.root()),
    );
    match output {
        Ok(output) => {
            for line in output
                .lines()
                .filter(|line| line.starts_with("Reclaimable:") || line.starts_with("Total:"))
            {
                println!("INFO BuildKit {line}");
            }
        }
        Err(_) => println!(
            "INFO managed BuildKit worker is not running; the release command will validate or create it"
        ),
    }
}

fn check_node(
    repository: &RepositoryContext,
    node: &str,
    namespace: &str,
    failures: &mut Vec<String>,
) {
    let output = process::output_text(
        "kubectl",
        ["get", "node", node, "-o", "json"],
        Some(repository.root()),
    );
    let value: Value = match output.and_then(|output| {
        serde_json::from_str(&output).context("decoding Kubernetes node response")
    }) {
        Ok(value) => value,
        Err(error) => {
            failures.push(format!(
                "cannot inspect Kubernetes node `{node}`: {error:#}"
            ));
            return;
        }
    };
    let conditions = value["status"]["conditions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let condition = |kind: &str| {
        conditions
            .iter()
            .find(|condition| condition["type"] == kind)
            .and_then(|condition| condition["status"].as_str())
    };
    let ready = condition("Ready");
    let disk_pressure = condition("DiskPressure");
    if ready == Some("True") && disk_pressure == Some("False") {
        println!("PASS Kubernetes node {node}: Ready=True, DiskPressure=False");
    } else {
        failures.push(format!(
            "Kubernetes node `{node}` reports Ready={ready:?}, DiskPressure={disk_pressure:?}"
        ));
    }

    let pods = process::output_text(
        "kubectl",
        [
            "get",
            "pods",
            "-n",
            namespace,
            "--field-selector=status.phase=Failed",
            "-o",
            "json",
        ],
        Some(repository.root()),
    );
    match pods.and_then(|output| {
        serde_json::from_str::<Value>(&output).context("decoding failed pod inventory")
    }) {
        Ok(pods) => {
            let evicted = pods["items"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|pod| pod["status"]["reason"] == "Evicted")
                .count();
            if evicted > 0 {
                println!(
                    "HINT namespace {namespace} retains {evicted} historical Evicted pod object(s); delete only controller-superseded objects after acceptance"
                );
            }
        }
        Err(error) => println!("HINT failed-pod inventory was unavailable: {error:#}"),
    }
}

fn gib(bytes: u128) -> u128 {
    bytes / GIB
}

#[cfg(test)]
mod tests {
    use super::{FilesystemSpace, GIB, parse_df, storage_budget};

    #[test]
    fn parses_machine_sized_df_output() {
        let space =
            parse_df("       1B-blocks    Avail Use%\n1967317549056 397197320192  79%\n").unwrap();
        assert_eq!(space.total_bytes, 1_967_317_549_056);
        assert_eq!(space.available_bytes, 397_197_320_192);
        assert_eq!(space.used_percent, 79);
    }

    #[test]
    fn release_budget_retains_growth_and_free_space_reserve() {
        let filesystem = FilesystemSpace {
            total_bytes: 2_000 * GIB,
            available_bytes: 800 * GIB,
            used_percent: 60,
        };
        let safe = storage_budget(filesystem, 320, 20);
        assert_eq!(safe.required_available_bytes, 720 * GIB);
        assert_eq!(safe.remaining_bytes, Some(80 * GIB));

        let unsafe_space = FilesystemSpace {
            available_bytes: 700 * GIB,
            ..filesystem
        };
        assert_eq!(storage_budget(unsafe_space, 320, 20).remaining_bytes, None);
    }
}
