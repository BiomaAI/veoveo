//! Match every tracked Rust macro definition against the reviewed exact catalog.
use std::{collections::BTreeMap, fs, path::Path};

use anyhow::{Context, Result, bail};
use syn::{
    Meta,
    visit::{self, Visit},
};

use crate::{context::RepositoryContext, process};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Declarative,
    Function,
    Derive,
    Attribute,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Definition {
    name: String,
    function: String,
    kind: Kind,
}
struct Entry {
    path: &'static str,
    name: &'static str,
    function: &'static str,
    kind: Kind,
}
const CATALOG: &[Entry] = &[
    Entry {
        path: "platform/macros/src/lib.rs",
        name: "Id",
        function: "id",
        kind: Kind::Derive,
    },
    Entry {
        path: "platform/macros/src/lib.rs",
        name: "ResourceAddress",
        function: "resource_address",
        kind: Kind::Derive,
    },
    Entry {
        path: "platform/macros/src/lib.rs",
        name: "Vocabulary",
        function: "vocabulary",
        kind: Kind::Derive,
    },
    Entry {
        path: "platform/macros/src/lib.rs",
        name: "embedded_document",
        function: "embedded_document",
        kind: Kind::Function,
    },
    Entry {
        path: "mcp/contract/src/docs.rs",
        name: "server_docs",
        function: "server_docs",
        kind: Kind::Declarative,
    },
    Entry {
        path: "servers/recording-mcp/src/playback.rs",
        name: "impl_scoped_redap_service",
        function: "impl_scoped_redap_service",
        kind: Kind::Declarative,
    },
];

#[derive(Default)]
struct Definitions {
    found: Vec<Definition>,
    failures: Vec<String>,
}
impl Definitions {
    fn proc_attribute(&mut self, meta: &Meta, function: &str) {
        let path = meta.path();
        let kind = if path.is_ident("proc_macro") {
            Some(Kind::Function)
        } else if path.is_ident("proc_macro_derive") {
            Some(Kind::Derive)
        } else if path.is_ident("proc_macro_attribute") {
            Some(Kind::Attribute)
        } else {
            None
        };
        if let Some(kind) = kind {
            let name = if kind == Kind::Derive {
                match meta {
                    Meta::List(list) => {
                        match list.parse_args_with(
                            syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                        ) {
                            Ok(args) => match args.first() {
                                Some(Meta::Path(path)) => path.get_ident().map(ToString::to_string),
                                _ => None,
                            },
                            Err(_) => None,
                        }
                    }
                    _ => None,
                }
            } else {
                Some(function.to_owned())
            };
            if let Some(name) = name {
                self.found.push(Definition {
                    name,
                    function: function.into(),
                    kind,
                });
            } else {
                self.failures.push(format!(
                    "malformed procedural macro declaration on {function}"
                ));
            }
        } else if path.is_ident("cfg_attr") {
            match meta {
                Meta::List(list) => match list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                ) {
                    Ok(args) => {
                        for nested in args.iter().skip(1) {
                            self.proc_attribute(nested, function);
                        }
                    }
                    Err(error) => self
                        .failures
                        .push(format!("invalid cfg_attr on {function}: {error}")),
                },
                _ => self
                    .failures
                    .push(format!("invalid cfg_attr on {function}")),
            }
        }
    }
}
impl<'ast> Visit<'ast> for Definitions {
    fn visit_item_macro(&mut self, node: &'ast syn::ItemMacro) {
        if let Some(name) = &node.ident {
            if node.mac.path.is_ident("macro_rules") {
                self.found.push(Definition {
                    name: name.to_string(),
                    function: name.to_string(),
                    kind: Kind::Declarative,
                });
            } else {
                self.failures
                    .push(format!("unsupported named macro definition {name}"));
            }
        }
        // An invocation's tokens are not Rust items; string fixtures and macro bodies
        // do not introduce source definitions until expansion by their owner.
    }
    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        for attr in &node.attrs {
            self.proc_attribute(&attr.meta, &node.sig.ident.to_string());
        }
        visit::visit_item_fn(self, node);
    }
    fn visit_item(&mut self, node: &'ast syn::Item) {
        if matches!(node, syn::Item::Verbatim(_)) {
            self.failures
                .push("unsupported Rust item syntax; parser qualification required".into());
        }
        visit::visit_item(self, node);
    }
}
fn definitions(source: &str) -> Result<Vec<Definition>> {
    let file = syn::parse_file(source).context("parsing Rust source")?;
    let mut collector = Definitions::default();
    collector.visit_file(&file);
    if !collector.failures.is_empty() {
        bail!("{}", collector.failures.join("\n"));
    }
    Ok(collector.found)
}
fn qualify(found: &[(String, Definition)], catalog: &[Entry]) -> Result<()> {
    let mut counts = BTreeMap::new();
    let mut failures = Vec::new();
    for (path, definition) in found {
        if let Some(index) = catalog.iter().position(|entry| {
            entry.path == path
                && entry.name == definition.name
                && entry.function == definition.function
                && entry.kind == definition.kind
        }) {
            *counts.entry(index).or_insert(0_usize) += 1;
        } else {
            failures.push(format!(
                "{path}: unreviewed {:?} macro {} (Rust name {})",
                definition.kind, definition.name, definition.function
            ));
        }
    }
    for (index, entry) in catalog.iter().enumerate() {
        match counts.get(&index).copied().unwrap_or(0) {
            1 => (),
            count => failures.push(format!(
                "{}: catalog macro {} has {count} definitions; expected one",
                entry.path, entry.name
            )),
        }
    }
    if !failures.is_empty() {
        bail!("macro declaration policy failed:\n{}", failures.join("\n"));
    }
    Ok(())
}
fn scan(root: &Path) -> Result<(usize, Vec<(String, Definition)>)> {
    let listing = process::output("git", ["ls-files", "-z"], Some(root))?.stdout;
    let listing = String::from_utf8(listing).context("git ls-files paths must be UTF-8")?;
    let mut found = Vec::new();
    let mut count = 0;
    for path in listing.split('\0').filter(|path| path.ends_with(".rs")) {
        let source = fs::read_to_string(root.join(path))
            .with_context(|| format!("reading tracked Rust {path}"))?;
        let definitions =
            definitions(&source).with_context(|| format!("checking tracked Rust {path}"))?;
        found.extend(
            definitions
                .into_iter()
                .map(|definition| (path.to_owned(), definition)),
        );
        count += 1;
    }
    Ok((count, found))
}
pub(crate) fn enforce(repository: &RepositoryContext) -> Result<()> {
    let (count, found) = scan(repository.root())?;
    qualify(&found, CATALOG)?;
    println!(
        "checked {} macro definitions in {count} tracked Rust sources",
        found.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests;
