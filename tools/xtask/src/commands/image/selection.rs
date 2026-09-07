use std::collections::BTreeSet;

use anyhow::{Result, bail, ensure};
use serde::Serialize;

use super::validate_identifier;
use crate::ImageSelectionArgs;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Selection {
    pub(crate) kind: SelectionKind,
    pub(crate) name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) targets: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SelectionKind {
    Target,
    Group,
    Exact,
}

impl Selection {
    pub(crate) fn from_args(args: &ImageSelectionArgs) -> Result<Self> {
        Self::from_targets(&args.target, args.group.as_deref())
    }

    pub(crate) fn from_targets(targets: &[String], group: Option<&str>) -> Result<Self> {
        match (targets, group) {
            ([target], None) => Self::target(target),
            ([], Some(group)) => Self::group(group),
            ([], None) | (_, Some(_)) => bail!("select one or more --target values or one --group"),
            (_, None) => Self::exact("selected", targets.iter().cloned()),
        }
    }

    pub(crate) fn target(target: &str) -> Result<Self> {
        validate_identifier("Bake target", target)?;
        Ok(Self {
            kind: SelectionKind::Target,
            name: target.to_owned(),
            targets: Vec::new(),
        })
    }

    pub(crate) fn group(group: &str) -> Result<Self> {
        validate_identifier("Bake group", group)?;
        Ok(Self {
            kind: SelectionKind::Group,
            name: group.to_owned(),
            targets: Vec::new(),
        })
    }

    pub(crate) fn exact(name: &str, targets: impl IntoIterator<Item = String>) -> Result<Self> {
        validate_identifier("exact Bake selection", name)?;
        let targets = targets.into_iter().collect::<BTreeSet<_>>();
        ensure!(!targets.is_empty(), "exact Bake selection cannot be empty");
        for target in &targets {
            validate_identifier("Bake target", target)?;
        }
        Ok(Self {
            kind: SelectionKind::Exact,
            name: name.to_owned(),
            targets: targets.into_iter().collect(),
        })
    }

    pub(super) fn bake_patterns(&self) -> Vec<&str> {
        match self.kind {
            SelectionKind::Target | SelectionKind::Group => vec![self.name.as_str()],
            SelectionKind::Exact => self.targets.iter().map(String::as_str).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;

    use super::*;
    use crate::{Cli, Command, ImageCommand, ReleaseCommand};

    #[test]
    fn repeated_targets_produce_one_exact_sorted_selection() {
        let cli = Cli::try_parse_from([
            "xtask",
            "image",
            "plan",
            "--target",
            "mcp-gateway",
            "--target",
            "console-bff",
        ])
        .unwrap();
        let Command::Image {
            command: ImageCommand::Plan(args),
        } = cli.command
        else {
            panic!("image plan")
        };
        let selection = Selection::from_args(&args.selection).unwrap();
        assert!(matches!(selection.kind, SelectionKind::Exact));
        assert_eq!(selection.bake_patterns(), ["console-bff", "mcp-gateway"]);
        assert!(
            Cli::try_parse_from([
                "xtask",
                "image",
                "plan",
                "--target",
                "mcp-gateway",
                "--group",
                "platform-full",
            ])
            .is_err()
        );
    }

    #[test]
    fn staged_selections_can_be_qualified_together() {
        for operation in [["image", "stage"], ["release", "images"]] {
            let cli = Cli::try_parse_from([
                "xtask",
                operation[0],
                operation[1],
                "--target",
                "mcp-gateway",
                "--target",
                "console-bff",
                "--revision",
                "HEAD",
                "--push-registry",
                "localhost:5001",
                "--pull-registry",
                "registry:5000",
                "--registry-transport",
                "insecure-http",
            ])
            .unwrap();
            let selection = match cli.command {
                Command::Image {
                    command: ImageCommand::Stage(args),
                } => Selection::from_args(&args.selection),
                Command::Release {
                    command: ReleaseCommand::Images(args),
                } => Selection::from_targets(&args.target, args.group.as_deref()),
                _ => panic!("publication command"),
            }
            .unwrap();
            assert_eq!(selection.bake_patterns(), ["console-bff", "mcp-gateway"]);
        }
    }
}
