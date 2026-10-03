//! The example domain: a fixed glossary of Veoveo hosting terms.
//!
//! Replace this module with the new server's domain logic. It knows nothing
//! about MCP, so it is tested directly.

use crate::contract::{Definition, TermId};

const ENTRIES: [(&str, &str, &str, &[&str]); 4] = [
    (
        "domain-read",
        "Domain read",
        "The result of reading one admitted resource address. It names its cache \
         policy: private for ordinary content, or no-store when access or freshness \
         can change between reads.",
        &["resource-address"],
    ),
    (
        "durable-task",
        "Durable task",
        "A task-augmented tool call stored in the platform task runtime, so it \
         survives replica restarts and reports its result through tasks/get.",
        &["hosted-server"],
    ),
    (
        "hosted-server",
        "Hosted server",
        "An MCP server built with HostedServer. The host supplies routes, gateway \
         authentication, discovery, documents and shutdown, and the server supplies \
         its domain.",
        &["domain-read", "durable-task"],
    ),
    (
        "resource-address",
        "Resource address",
        "A typed value for one resource URI. The host parses each requested URI \
         into the server's address type and rejects URIs that do not parse.",
        &["domain-read"],
    ),
];

/// Every entry, ordered by term.
pub fn entries() -> Vec<Definition> {
    ENTRIES
        .iter()
        .map(|(term, title, definition, see_also)| Definition {
            term: TermId::new(*term).expect("declared term"),
            title: (*title).to_owned(),
            definition: (*definition).to_owned(),
            see_also: see_also
                .iter()
                .map(|term| TermId::new(*term).expect("declared term"))
                .collect(),
        })
        .collect()
}

/// The entry for `term`, if the glossary has one.
pub fn lookup(term: &TermId) -> Option<Definition> {
    entries().into_iter().find(|entry| entry.term == *term)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_sorted_and_link_only_to_known_terms() {
        let entries = entries();
        let terms = entries.iter().map(|entry| &entry.term).collect::<Vec<_>>();
        assert!(terms.is_sorted());
        for entry in &entries {
            for related in &entry.see_also {
                assert!(lookup(related).is_some(), "{related}");
            }
        }
    }
}
