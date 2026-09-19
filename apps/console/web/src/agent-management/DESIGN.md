# Shared Agent Authoring Client

## Standards And Protocols

The Console and Workspace editor uses the generated Veoveo HTTP JSON schema and
contentless SSE notifications. Rust owns request types and JSON Schema 2020-12.
Browser validation uses the repository's existing Zod converter. Existing
same-origin session cookies and CSRF headers authorize each application's own edge.

## Editing And Publication

The shared editor manages private drafts, approved model choices, bounded budgets,
capability selection, metadata and publication review. It retains mutation request
IDs when a response is uncertain. Concurrent edits produce an explicit reload action;
notifications refresh the catalog without replacing unsaved form text. Creation
locks an uncertain request until retry recovers its outcome.

History displays immutable published content. Disable and archive explain their
separate effects before confirmation. Current permissions govern every action.
Observation does not invoke a model. The shared module is bundled independently
into each client, so it does not share session state between browser applications.
