# Contract Document Generation

## Standards And Protocols

The generator consumes hosted contract revision 4, requirement catalog revision 2 and
JSON Schema 2020-12. UTF-8 JSON and Markdown use LF endings. TOML manifests are parsed
with the maintained `toml` 1.1.6 release (specification 1.1.0); discovery needs actual
package and Hatch hook structure rather than text matching. The exact
[`pulldown-cmark` 0.13.4](https://github.com/pulldown-cmark/pulldown-cmark/releases/tag/v0.13.4)
parser supplies CommonMark events with the GitHub table extension for owning design
admission. This is source tooling,
independent of installed MCP transports.

## Ownership And Generation

`release contract-docs` discovers Rust MCP packages, Python document-hook packages and
Node document builders under component, template, fixture and showcase roots. It
reconciles actual Rust `server_docs!` selections using the existing Rust syntax parser.
Each owner supplies an admitted `contract-compliance.json` and adjacent documents.
A new independent server joins by declaring its package and profile.

Every discovered owner supplies an adjacent UTF-8 `DESIGN.md` with a top-level
level-two Standards And Protocols heading and content before the next peer or parent
heading. Visible inline formatting may appear in the title. Code fences, indented
code, quoted examples and HTML comments cannot supply the owning heading. Empty
sections, including comment-only bodies, fail with the owner design path and a
repair instruction. The generator admits all owner designs before writing any
manual or catalog output.

The generator replaces only the explicit compliance marker pair in each manual.
Missing, duplicated or reversed markers fail. It checks the rendered section against
the checked profile and preserves all surrounding bytes. Catalog metadata generates
the normative requirement table and identical catalog/schema copies for Rust, Python
and Charts packaging. Conformance supplies a separately versioned verification map.
`--check` compares these outputs without writing them.

## Verification

Owning unit tests check Markdown section admission, absent designs, invalid later
owners before generation effects, read-only stale-output checks, marker failures and
surrounding-byte preservation. The
contract crate owns profile and shared-byte admission cases; conformance owns live
well-known agreement and observed C32 applicability. Release qualification runs the
complete generator and then `--check`, with package tests for each language consumer.
