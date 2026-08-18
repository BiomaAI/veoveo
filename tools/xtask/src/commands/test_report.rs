use std::{
    env,
    ffi::{OsStr, OsString},
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Instant,
};

use anyhow::{Context, Result, bail};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{context::RepositoryContext, process};

const REPORT_PATH: &str = "testing/local-test-report.json";
const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalTestReport {
    schema_version: u32,
    source_digest: String,
    updated_at: String,
    checks: Vec<LocalTestCheck>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalTestCheck {
    name: String,
    command: Vec<String>,
    status: CheckStatus,
    finished_at: String,
    duration_seconds: f64,
    detail: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Passed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReportStatus {
    Passed,
    Failed,
    Missing,
    Stale,
}

pub(crate) fn run(
    repository: &RepositoryContext,
    name: &str,
    arguments: &[OsString],
) -> Result<()> {
    validate_name(name)?;
    let (program, command_arguments) = arguments
        .split_first()
        .context("a test-report command is required after --")?;
    let source_before = source_digest(repository.root())?;
    let mut report = load_for_source(repository.root(), &source_before)?;

    let started = Instant::now();
    let outcome = execute(repository.root(), program, command_arguments);
    let duration_seconds = started.elapsed().as_secs_f64();
    let source_after = source_digest(repository.root())?;

    let (status, detail) = match &outcome {
        Ok(exit) if exit.success() && source_before == source_after => {
            (CheckStatus::Passed, "completed successfully".to_owned())
        }
        Ok(exit) if source_before != source_after => (
            CheckStatus::Failed,
            "the command changed tracked or untracked source; rerun checks for the new source"
                .to_owned(),
        ),
        Ok(exit) => (CheckStatus::Failed, format!("exited with {exit}")),
        Err(error) => (CheckStatus::Failed, format!("could not run: {error:#}")),
    };

    if source_before != source_after {
        report = LocalTestReport::empty(source_after);
    }
    report.upsert(LocalTestCheck {
        name: name.to_owned(),
        command: arguments
            .iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect(),
        status,
        finished_at: now(),
        duration_seconds,
        detail,
    });
    write_report(repository.root(), &report)?;

    println!("updated {REPORT_PATH}");
    if status == CheckStatus::Failed {
        bail!("local check {name:?} failed; the result was recorded")
    }
    Ok(())
}

pub(crate) fn show(repository: &RepositoryContext, github_summary: bool) -> Result<()> {
    let current_digest = source_digest(repository.root())?;
    let report = read_report(repository.root())?;
    let status = report_status(report.as_ref(), &current_digest);
    let markdown = render_markdown(report.as_ref(), &current_digest, status);
    print!("{markdown}");

    if github_summary {
        append_github_summary(&markdown)?;
    }

    match status {
        ReportStatus::Passed => Ok(()),
        ReportStatus::Failed => bail!("the committed local test report contains failures"),
        ReportStatus::Missing => bail!("the committed local test report is missing"),
        ReportStatus::Stale => bail!("the committed local test report does not match this source"),
    }
}

impl LocalTestReport {
    fn empty(source_digest: String) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            source_digest,
            updated_at: now(),
            checks: Vec::new(),
        }
    }

    fn upsert(&mut self, check: LocalTestCheck) {
        self.updated_at.clone_from(&check.finished_at);
        if let Some(existing) = self
            .checks
            .iter_mut()
            .find(|existing| existing.name == check.name)
        {
            *existing = check;
        } else {
            self.checks.push(check);
            self.checks
                .sort_by(|left, right| left.name.cmp(&right.name));
        }
    }
}

fn validate_name(name: &str) -> Result<()> {
    let valid = !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        bail!("check name must contain 1-64 lowercase ASCII letters, digits, or hyphens")
    }
    Ok(())
}

fn execute(
    root: &Path,
    program: &OsStr,
    arguments: &[OsString],
) -> Result<std::process::ExitStatus> {
    let mut command = Command::new(program);
    command
        .args(arguments)
        .current_dir(root)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    process::remove_parent_cargo_package_environment(&mut command);
    command.status().context("starting local check")
}

fn load_for_source(root: &Path, source_digest: &str) -> Result<LocalTestReport> {
    let Some(report) = read_report(root)? else {
        return Ok(LocalTestReport::empty(source_digest.to_owned()));
    };
    if report.schema_version != SCHEMA_VERSION || report.source_digest != source_digest {
        return Ok(LocalTestReport::empty(source_digest.to_owned()));
    }
    Ok(report)
}

fn read_report(root: &Path) -> Result<Option<LocalTestReport>> {
    let path = root.join(REPORT_PATH);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = fs::read(&path).with_context(|| format!("reading {}", path.display()))?;
    let report =
        serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))?;
    Ok(Some(report))
}

fn write_report(root: &Path, report: &LocalTestReport) -> Result<()> {
    let path = root.join(REPORT_PATH);
    let parent = path.parent().context("local test report has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let temporary = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("creating temporary report in {}", parent.display()))?;
    let mut writer = BufWriter::new(temporary.as_file());
    serde_json::to_writer_pretty(&mut writer, report).context("encoding local test report")?;
    writer
        .write_all(b"\n")
        .context("terminating local test report")?;
    writer.flush().context("flushing local test report")?;
    drop(writer);
    temporary
        .persist(&path)
        .map_err(|error| error.error)
        .with_context(|| format!("publishing {}", path.display()))?;
    Ok(())
}

fn source_digest(root: &Path) -> Result<String> {
    let output = process::output(
        "git",
        [
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
        Some(root),
    )?;
    let mut paths = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
        .map(|bytes| String::from_utf8(bytes.to_vec()).context("repository path is not UTF-8"))
        .collect::<Result<Vec<_>>>()?;
    paths.retain(|path| path != REPORT_PATH && root.join(path).symlink_metadata().is_ok());
    paths.sort();

    let regular_files = paths
        .iter()
        .filter_map(|relative| {
            let path = root.join(relative);
            let metadata = fs::symlink_metadata(&path).ok()?;
            metadata.is_file().then_some(relative.as_str())
        })
        .collect::<Vec<_>>();
    let object_ids = canonical_object_ids(root, &regular_files)?;
    let mut object_ids = regular_files.into_iter().zip(object_ids);

    let mut digest = Sha256::new();
    digest.update(b"veoveo-local-test-source-v1\0");
    for relative in &paths {
        let path = root.join(relative);
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("reading metadata for {}", path.display()))?;
        digest.update(relative.as_bytes());
        digest.update([0]);
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(&path)
                .with_context(|| format!("reading symlink {}", path.display()))?;
            digest.update(b"symlink\0");
            digest.update(target.as_os_str().to_string_lossy().as_bytes());
        } else if metadata.is_file() {
            digest.update(b"file\0");
            let (object_path, object_id) = object_ids
                .next()
                .context("Git did not return an object identity for every source file")?;
            if object_path != relative.as_str() {
                bail!("Git object identities were returned out of source order")
            }
            digest.update(object_id.as_bytes());
        } else {
            bail!("unsupported repository entry {}", path.display())
        }
        digest.update([0]);
    }
    Ok(format!("sha256:{}", hex::encode(digest.finalize())))
}

fn canonical_object_ids(root: &Path, paths: &[&str]) -> Result<Vec<String>> {
    if paths
        .iter()
        .any(|path| path.as_bytes().contains(&b'\n') || path.as_bytes().contains(&b'\r'))
    {
        bail!("source paths containing newlines cannot be fingerprinted")
    }

    let mut child = Command::new("git")
        .args(["hash-object", "--stdin-paths"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .context("starting Git source canonicalization")?;
    {
        let mut stdin = child.stdin.take().context("opening Git hash input")?;
        for path in paths {
            writeln!(stdin, "{path}").context("writing Git hash input")?;
        }
    }
    let output = child
        .wait_with_output()
        .context("waiting for Git source canonicalization")?;
    if !output.status.success() {
        bail!("Git source canonicalization exited with {}", output.status)
    }
    let object_ids = String::from_utf8(output.stdout)
        .context("Git object identity output is not UTF-8")?
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if object_ids.len() != paths.len() {
        bail!(
            "Git returned {} object identities for {} source files",
            object_ids.len(),
            paths.len()
        )
    }
    Ok(object_ids)
}

fn report_status(report: Option<&LocalTestReport>, current_digest: &str) -> ReportStatus {
    let Some(report) = report else {
        return ReportStatus::Missing;
    };
    if report.schema_version != SCHEMA_VERSION || report.source_digest != current_digest {
        return ReportStatus::Stale;
    }
    if report.checks.is_empty() {
        return ReportStatus::Missing;
    }
    if report
        .checks
        .iter()
        .any(|check| check.status == CheckStatus::Failed)
    {
        ReportStatus::Failed
    } else {
        ReportStatus::Passed
    }
}

fn render_markdown(
    report: Option<&LocalTestReport>,
    current_digest: &str,
    status: ReportStatus,
) -> String {
    let title = match status {
        ReportStatus::Passed => "✅ Local checks passed",
        ReportStatus::Failed => "❌ Local checks reported failures",
        ReportStatus::Missing => "⚪ No local test results recorded",
        ReportStatus::Stale => "⚠️ Local test report is stale",
    };
    let mut output = format!(
        "# Local test report\n\n{title}\n\nThis check is informational. It does not gate commits, pushes, or deployment.\n\n"
    );
    if let Some(report) = report {
        output.push_str(&format!(
            "- Report updated: `{}`\n- Report source: `{}`\n- Current source: `{current_digest}`\n\n",
            escape_inline(&report.updated_at),
            escape_inline(&report.source_digest)
        ));
        if !report.checks.is_empty() {
            output.push_str("| Check | Result | Duration | Command | Detail |\n");
            output.push_str("|---|---:|---:|---|---|\n");
            for check in &report.checks {
                let result = match check.status {
                    CheckStatus::Passed => "passed",
                    CheckStatus::Failed => "failed",
                };
                let command = check
                    .command
                    .iter()
                    .map(|part| shell_display(part))
                    .collect::<Vec<_>>()
                    .join(" ");
                output.push_str(&format!(
                    "| `{}` | {result} | {:.1}s | `{}` | {} |\n",
                    escape_table(&check.name),
                    check.duration_seconds,
                    escape_table(&command),
                    escape_table(&check.detail)
                ));
            }
            output.push('\n');
        }
    } else {
        output.push_str(&format!("Current source: `{current_digest}`\n\n"));
    }
    output
}

fn append_github_summary(markdown: &str) -> Result<()> {
    let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    let path = PathBuf::from(path);
    let mut file = File::options()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening GitHub step summary {}", path.display()))?;
    file.write_all(markdown.as_bytes())
        .with_context(|| format!("writing GitHub step summary {}", path.display()))
}

fn shell_display(value: &str) -> String {
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-._/:=@".contains(&byte))
    {
        value.to_owned()
    } else {
        format!("{:?}", value)
    }
}

fn escape_inline(value: &str) -> String {
    value.replace('`', "\\`")
}

fn escape_table(value: &str) -> String {
    escape_inline(value).replace('|', "\\|").replace('\n', " ")
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::{
        CheckStatus, LocalTestCheck, LocalTestReport, ReportStatus, render_markdown, report_status,
        source_digest, validate_name,
    };

    fn report(status: CheckStatus) -> LocalTestReport {
        LocalTestReport {
            schema_version: 1,
            source_digest: "sha256:source".to_owned(),
            updated_at: "2026-08-18T00:00:00Z".to_owned(),
            checks: vec![LocalTestCheck {
                name: "rust-workspace".to_owned(),
                command: vec!["cargo".to_owned(), "test".to_owned()],
                status,
                finished_at: "2026-08-18T00:00:00Z".to_owned(),
                duration_seconds: 2.5,
                detail: "completed successfully".to_owned(),
            }],
        }
    }

    #[test]
    fn check_names_are_stable_identifiers() {
        assert!(validate_name("uav-browser").is_ok());
        assert!(validate_name("UAV browser").is_err());
        assert!(validate_name("").is_err());
    }

    #[test]
    fn status_distinguishes_failure_and_stale_source() {
        let passed = report(CheckStatus::Passed);
        let failed = report(CheckStatus::Failed);
        assert_eq!(
            report_status(Some(&passed), "sha256:source"),
            ReportStatus::Passed
        );
        assert_eq!(
            report_status(Some(&failed), "sha256:source"),
            ReportStatus::Failed
        );
        assert_eq!(
            report_status(Some(&passed), "sha256:other"),
            ReportStatus::Stale
        );
    }

    #[test]
    fn markdown_states_that_the_result_is_informational() {
        let report = report(CheckStatus::Failed);
        let markdown = render_markdown(Some(&report), "sha256:source", ReportStatus::Failed);
        assert!(markdown.contains("informational"));
        assert!(markdown.contains("rust-workspace"));
        assert!(markdown.contains("failed"));
    }

    #[test]
    fn source_digest_accepts_a_tracked_file_deleted_from_the_worktree() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        assert!(
            Command::new("git")
                .arg("init")
                .arg(root)
                .status()
                .unwrap()
                .success()
        );
        fs::write(root.join("retained.txt"), "retained").unwrap();
        fs::write(root.join("deleted.txt"), "deleted").unwrap();
        assert!(
            Command::new("git")
                .args(["add", "retained.txt", "deleted.txt"])
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
        fs::remove_file(root.join("deleted.txt")).unwrap();

        assert!(source_digest(root).is_ok());
    }

    #[test]
    fn source_digest_uses_clean_filtered_content() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        assert!(
            Command::new("git")
                .arg("init")
                .arg(root)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args([
                    "config",
                    "filter.canonical.clean",
                    "sed s/materialized/canonical/"
                ])
                .current_dir(root)
                .status()
                .unwrap()
                .success()
        );
        fs::write(root.join(".gitattributes"), "*.asset filter=canonical\n").unwrap();
        fs::write(root.join("camera.asset"), "materialized\n").unwrap();
        let materialized = source_digest(root).unwrap();

        fs::write(root.join("camera.asset"), "canonical\n").unwrap();
        let canonical = source_digest(root).unwrap();

        assert_eq!(materialized, canonical);
    }
}
