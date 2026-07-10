//! Generic runtime for long-lived autonomous MCP agents.
//!
//! An agent runs forever against the veoveo gateway in bounded *episodes*:
//! it sleeps between episodes and wakes on task results, timers, or operator
//! input. SurrealDB is the scheduling and delivery authority. DuckDB and RRD
//! are local analytical memory planes; chat history is never the memory.
//!
//! Long gateway tools are dispatched through the gateway's explicit Rig task
//! projection over canonical final-extension tasks and detached at episode end.
//! Their descriptors, retry schedule, retention pins, results, and result wakes
//! remain durable across process restarts.

pub mod budget;
pub mod connection;
pub mod context;
pub mod delegate;
pub mod elicitation;
pub mod episode;
pub mod llm;
pub mod manifest;
pub mod memory;
pub mod recorder;
pub mod replay;
pub mod rrd;
pub mod summary;
pub mod tasks;
pub mod timeline;
pub mod tools;
pub mod wake;
