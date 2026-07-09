//! Generic runtime for long-lived autonomous MCP agents.
//!
//! An agent runs forever against the veoveo gateway in bounded *episodes*:
//! it sleeps between episodes and wakes on task results, timers, or operator
//! input. Durable state lives in two local files — a DuckDB ledger (current
//! truth) and, from slice 2, an RRD decision log. Episode LLM context is
//! assembled from those files each time; chat history is never the memory.
//!
//! Long gateway tools are dispatched as MCP tasks (SEP-1686) and detached at
//! episode end: the serializable descriptors go to the ledger, watchers
//! rehydrate live handles via `McpTaskResumer` — across process restarts —
//! and each result wakes a fresh episode.

pub mod budget;
pub mod connection;
pub mod context;
pub mod delegate;
pub mod elicitation;
pub mod episode;
pub mod ledger;
pub mod llm;
pub mod manifest;
pub mod operator;
pub mod recorder;
pub mod replay;
pub mod rrd;
pub mod summary;
pub mod tasks;
pub mod timeline;
pub mod tools;
pub mod wake;
