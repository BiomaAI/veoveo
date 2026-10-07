//! Framework reports qualify execution; an exit code alone cannot prove a selected case ran.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
pub enum Framework {
    #[vocabulary(rename = "libtest")]
    Libtest,
    #[vocabulary(rename = "pytest")]
    Pytest,
    #[vocabulary(rename = "node")]
    Node,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum FrameworkFormat {
    #[vocabulary(rename = "veoveo.ai/framework-outcome/v1")]
    V1,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameworkOutcome {
    pub format: FrameworkFormat,
    pub framework: Framework,
    pub case: String,
    pub matched: u64,
    pub passed: u64,
    pub skipped: u64,
    pub failed: u64,
}
impl FrameworkOutcome {
    pub fn check(&self, framework: Framework, case: &str) -> Result<()> {
        ensure!(
            self.format == FrameworkFormat::V1 && self.framework == framework && self.case == case,
            "framework report identity mismatch"
        );
        ensure!(
            self.matched == 1 && self.passed == 1 && self.skipped == 0 && self.failed == 0,
            "selected framework case did not run exactly once without skips or failures"
        );
        Ok(())
    }
    pub fn read(path: &Path, framework: Framework, case: &str) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        ensure!(bytes.len() <= 16 * 1024, "framework report exceeds16KiB");
        let outcome: Self = serde_json::from_slice(&bytes)?;
        outcome.check(framework, case)?;
        Ok(outcome)
    }
}
/// Stable pretty libtest with normal capture and --show-output separates status from diagnostics.
/// Case status lives before successes/failures; owner output cannot supply a framework case line.
pub fn libtest_summary(bytes: &[u8], case: &str) -> Result<FrameworkOutcome> {
    ensure!(bytes.len() <= 8 * 1024 * 1024, "libtest output exceeds8MiB");
    let text = std::str::from_utf8(bytes)?;
    let first_boundary = text
        .lines()
        .find(|line| {
            matches!(*line, "successes:" | "failures:") || line.starts_with("test result:")
        })
        .context("missing framework boundary")?;
    if first_boundary.starts_with("test result:") {
        ensure!(
            text.lines()
                .filter(|line| line.starts_with("test result:"))
                .count()
                == 1
                && text.lines().rev().find(|line| !line.trim().is_empty()) == Some(first_boundary),
            "trailing or duplicated framework output"
        );
    }
    let preamble = text.lines().take_while(|line| {
        !matches!(*line, "successes:" | "failures:") && !line.starts_with("test result:")
    });
    let cases: Vec<_> = preamble
        .filter_map(|line| {
            line.strip_prefix("test ")
                .and_then(|line| line.rsplit_once(" ... "))
        })
        .collect();
    ensure!(
        cases.as_slice() == [(case, "ok")],
        "libtest executed a different, missing, ignored or failing case"
    );
    // The formatter writes its summary last, after captured output, including printed summary text.
    let summary = text
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .context("missing libtest outcome")?
        .strip_prefix("test result: ok. ")
        .context("libtest did not pass or its final outcome is missing")?;
    let field = |name: &str| -> Result<u64> {
        let mut found = summary
            .split(';')
            .filter_map(|item| item.trim().strip_suffix(name));
        let value = found
            .next()
            .context("libtest summary field is missing")?
            .trim()
            .parse()?;
        ensure!(found.next().is_none(), "duplicate libtest summary field");
        Ok(value)
    };
    let passed = field("passed")?;
    let failed = field("failed")?;
    let skipped = field("ignored")?;
    let outcome = FrameworkOutcome {
        format: FrameworkFormat::V1,
        framework: Framework::Libtest,
        case: case.into(),
        matched: passed
            .checked_add(failed)
            .and_then(|v| v.checked_add(skipped))
            .context("framework count overflow")?,
        passed,
        failed,
        skipped,
    };
    outcome.check(Framework::Libtest, case)?;
    Ok(outcome)
}
/// Exact selectors are dispatcher-owned; extra argv cannot modify listing/execution.
pub fn admit_exact_arguments(arguments: &[String]) -> Result<()> {
    ensure!(
        arguments.is_empty(),
        "exact framework scenarios do not admit additional selection arguments"
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_framework_protocol_rejects_unknown_revision_and_extra_keys() {
        let valid = serde_json::json!({"format":"veoveo.ai/framework-outcome/v1","framework":"libtest","case":"fixture","matched":1,"passed":1,"skipped":0,"failed":0});
        serde_json::from_value::<FrameworkOutcome>(valid.clone())
            .unwrap()
            .check(Framework::Libtest, "fixture")
            .unwrap();
        let mut bad = valid.clone();
        bad["format"] = serde_json::json!("veoveo.ai/framework-outcome/v2");
        assert!(serde_json::from_value::<FrameworkOutcome>(bad).is_err());
        let mut bad = valid;
        bad["unknownField"] = serde_json::json!(true);
        assert!(serde_json::from_value::<FrameworkOutcome>(bad).is_err());
    }
    #[test]
    fn missing_zero_ignored_failed_and_ambiguous_framework_outcomes_are_refused() {
        let valid=b"running 1 test\ntest fixture ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 0.00s\n";
        libtest_summary(valid, "fixture").unwrap();
        for invalid in [
            "running 1 test\ntest different_case ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
            "",
            "test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 1 filtered out; finished in 0.00s",
            "test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s",
            "test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s",
        ] {
            assert!(libtest_summary(invalid.as_bytes(), "fixture").is_err());
        }
        assert!(
            libtest_summary(&[valid.as_slice(), valid.as_slice()].concat(), "fixture").is_err()
        );
        for argv in [
            vec!["--skip".into()],
            vec!["other_case".into()],
            vec!["--list".into()],
            vec!["--ignored".into()],
        ] {
            assert!(admit_exact_arguments(&argv).is_err());
        }
        for framework in [Framework::Pytest, Framework::Node] {
            for (matched, passed, skipped, failed) in
                [(0, 0, 0, 0), (1, 0, 1, 0), (1, 0, 0, 1), (2, 2, 0, 0)]
            {
                let report = FrameworkOutcome {
                    format: FrameworkFormat::V1,
                    framework,
                    case: "fixture".into(),
                    matched,
                    passed,
                    skipped,
                    failed,
                };
                assert!(report.check(framework, "fixture").is_err());
            }
        }
    }
}
