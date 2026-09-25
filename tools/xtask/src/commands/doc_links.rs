//! Relative link and heading-anchor validation for tracked Markdown.
use std::{
    collections::{BTreeSet, HashMap},
    fs,
    path::{Component, Path},
};

use anyhow::{Context, Result, bail};

use crate::{context::RepositoryContext, process};

#[derive(Debug, PartialEq, Eq)]
struct Link {
    line: usize,
    target: String,
}

struct Tree {
    files: BTreeSet<String>,
    directories: BTreeSet<String>,
}

pub(crate) fn enforce(repository: &RepositoryContext) -> Result<()> {
    let root = repository.root();
    let listing = process::output("git", ["ls-files", "-z"], Some(root))?.stdout;
    let listing = String::from_utf8(listing).context("git ls-files output is not UTF-8")?;
    let tree = Tree::new(listing.split('\0').filter(|path| !path.is_empty()));
    let documents = tree
        .files
        .iter()
        .filter(|path| path.ends_with(".md"))
        .cloned()
        .collect::<Vec<_>>();

    let mut anchors = HashMap::<String, BTreeSet<String>>::new();
    let mut failures = Vec::new();
    let mut checked = 0;
    for document in &documents {
        let text = read(root, document)?;
        for link in links(&text) {
            checked += 1;
            if let Some(problem) = check(root, &tree, &mut anchors, document, &link.target)? {
                failures.push(format!(
                    "{document}:{}: {} ({problem})",
                    link.line, link.target
                ));
            }
        }
    }
    if !failures.is_empty() {
        bail!(
            "{} broken documentation links:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
    println!(
        "checked {checked} relative links in {} Markdown files",
        documents.len()
    );
    Ok(())
}

impl Tree {
    fn new<'a>(paths: impl Iterator<Item = &'a str>) -> Self {
        let mut files = BTreeSet::new();
        let mut directories = BTreeSet::new();
        for path in paths {
            let mut parent = Path::new(path).parent();
            while let Some(directory) = parent.filter(|value| !value.as_os_str().is_empty()) {
                directories.insert(directory.to_string_lossy().into_owned());
                parent = directory.parent();
            }
            files.insert(path.to_owned());
        }
        Self { files, directories }
    }

    fn contains(&self, path: &str) -> bool {
        path.is_empty() || self.files.contains(path) || self.directories.contains(path)
    }
}

fn read(root: &Path, path: &str) -> Result<String> {
    fs::read_to_string(root.join(path)).with_context(|| format!("reading {path}"))
}

fn check(
    root: &Path,
    tree: &Tree,
    anchors: &mut HashMap<String, BTreeSet<String>>,
    document: &str,
    target: &str,
) -> Result<Option<&'static str>> {
    let (path, fragment) = target.split_once('#').unwrap_or((target, ""));
    let path = path.split_once('?').map_or(path, |(path, _)| path);
    let resolved = if path.is_empty() {
        document.to_owned()
    } else {
        let base = match path.strip_prefix('/') {
            Some(_) => "",
            None => Path::new(document)
                .parent()
                .and_then(Path::to_str)
                .unwrap_or(""),
        };
        let Some(resolved) = normalize(base, path.trim_start_matches('/')) else {
            return Ok(Some("leaves the repository"));
        };
        if !tree.contains(&resolved) {
            return Ok(Some("missing target"));
        }
        resolved
    };
    if fragment.is_empty() || !resolved.ends_with(".md") || !tree.files.contains(&resolved) {
        return Ok(None);
    }
    if !anchors.contains_key(&resolved) {
        let headings = heading_anchors(&read(root, &resolved)?);
        anchors.insert(resolved.clone(), headings);
    }
    Ok((!anchors[&resolved].contains(fragment)).then_some("missing heading anchor"))
}

fn normalize(base: &str, relative: &str) -> Option<String> {
    let mut parts = Vec::new();
    for component in Path::new(base).join(relative).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?.to_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::CurDir => (),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}

/// Markdown and HTML link targets outside fenced blocks and inline code, excluding
/// URLs with a scheme.
fn links(text: &str) -> Vec<Link> {
    let mut found = Vec::new();
    for (line, content) in prose_lines(text) {
        let content = strip_inline_code(content);
        let mut targets = Vec::new();
        let mut rest = content.as_str();
        while let Some(start) = rest.find("](") {
            let after = rest[start + 2..].trim_start();
            let (target, remainder) = match after.strip_prefix('<') {
                Some(inner) => inner.split_once('>').unwrap_or((inner, "")),
                None => {
                    let end = after
                        .find(|character: char| character == ')' || character.is_whitespace())
                        .unwrap_or(after.len());
                    after.split_at(end)
                }
            };
            targets.push(target.to_owned());
            rest = remainder;
        }
        for attribute in ["href=\"", "src=\"", "srcset=\""] {
            let mut rest = content.as_str();
            while let Some(start) = rest.find(attribute) {
                let value = &rest[start + attribute.len()..];
                let end = value.find('"').unwrap_or(value.len());
                for candidate in value[..end].split(',') {
                    if let Some(target) = candidate.split_whitespace().next() {
                        targets.push(target.to_owned());
                    }
                }
                rest = &value[end..];
            }
        }
        found.extend(
            targets
                .into_iter()
                .filter(|target| !target.is_empty() && !external(target))
                .map(|target| Link { line, target }),
        );
    }
    found
}

fn external(target: &str) -> bool {
    if target.starts_with("//") {
        return true;
    }
    target.split_once(':').is_some_and(|(scheme, _)| {
        scheme
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "+.-".contains(character))
    })
}

/// GitHub heading anchors, including numbered duplicates and explicit HTML anchors.
fn heading_anchors(text: &str) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();
    let mut counts = HashMap::<String, usize>::new();
    for (_, content) in prose_lines(text) {
        let trimmed = content.trim_start();
        let level = trimmed
            .chars()
            .take_while(|&character| character == '#')
            .count();
        if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
            let heading = trimmed[level..].trim().trim_end_matches('#').trim_end();
            let slug = slug(heading);
            let count = counts.entry(slug.clone()).or_default();
            anchors.insert(match *count {
                0 => slug,
                number => format!("{slug}-{number}"),
            });
            *count += 1;
        }
        for attribute in ["<a name=\"", "<a id=\"", " id=\""] {
            let mut rest = content;
            while let Some(start) = rest.find(attribute) {
                let value = &rest[start + attribute.len()..];
                let end = value.find('"').unwrap_or(value.len());
                anchors.insert(value[..end].to_owned());
                rest = &value[end..];
            }
        }
    }
    anchors
}

fn slug(heading: &str) -> String {
    let mut plain = String::new();
    let mut in_tag = false;
    let mut in_url = false;
    let mut previous = '\0';
    for character in heading.chars() {
        match character {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            '(' if previous == ']' => in_url = true,
            ')' if in_url => in_url = false,
            _ if in_tag || in_url => (),
            _ => plain.push(character),
        }
        previous = character;
    }
    plain
        .trim()
        .to_lowercase()
        .chars()
        .filter_map(|character| match character {
            ' ' => Some('-'),
            '-' | '_' => Some(character),
            _ if character.is_alphanumeric() => Some(character),
            _ => None,
        })
        .collect()
}

fn prose_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut fenced = false;
    text.lines().enumerate().filter_map(move |(index, line)| {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            return None;
        }
        (!fenced).then_some((index + 1, line))
    })
}

fn strip_inline_code(line: &str) -> String {
    line.split('`')
        .enumerate()
        .filter(|(index, _)| index % 2 == 0)
        .map(|(_, part)| part)
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_github_heading_anchors() {
        assert_eq!(
            slug("Connect Robots And Simulators"),
            "connect-robots-and-simulators"
        );
        assert_eq!(slug("Edge & Access"), "edge--access");
        assert_eq!(
            slug("`uav-sim` [server](x.md) <b>v2</b>"),
            "uav-sim-server-v2"
        );
        assert_eq!(slug("Rerun 0.38.1 Recordings"), "rerun-0381-recordings");
    }

    #[test]
    fn duplicate_and_html_anchors_are_collected() {
        let anchors = heading_anchors(
            "# Setup\n## Setup\n```\n# Not A Heading\n```\n<a name=\"pinned\"></a>\n",
        );
        assert_eq!(
            anchors,
            BTreeSet::from(["setup".into(), "setup-1".into(), "pinned".into()])
        );
    }

    #[test]
    fn links_skip_code_and_external_targets() {
        let text = "See [a](docs/A.md#x) and <img src=\"b.png\">.\n\
                    `[code](nope.md)` [web](https://example.com) [mail](mailto:x@y)\n\
                    ```\n[fenced](nope.md)\n```\n[angle](<c d.md>) [self](#top)\n";
        let targets = links(text)
            .into_iter()
            .map(|link| (link.line, link.target))
            .collect::<Vec<_>>();
        assert_eq!(
            targets,
            vec![
                (1, "docs/A.md#x".to_owned()),
                (1, "b.png".to_owned()),
                (6, "c d.md".to_owned()),
                (6, "#top".to_owned()),
            ]
        );
    }

    #[test]
    fn paths_normalize_inside_the_repository() {
        assert_eq!(
            normalize("docs", "../README.md").as_deref(),
            Some("README.md")
        );
        assert_eq!(
            normalize("docs/a", "./b/../c.md").as_deref(),
            Some("docs/a/c.md")
        );
        assert_eq!(normalize("", "../outside.md"), None);
    }

    #[test]
    fn tree_knows_files_and_their_directories() {
        let tree = Tree::new(["docs/images/a.png", "README.md"].into_iter());
        assert!(tree.contains("docs/images"));
        assert!(tree.contains("docs"));
        assert!(tree.contains("README.md"));
        assert!(!tree.contains("docs/missing.md"));
    }
}
