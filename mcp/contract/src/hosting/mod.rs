//! Hosting for MCP servers behind the Veoveo gateway.
//!
//! A server supplies its domain and nothing else:
//!
//! - its checked [`McpServerSetup`]: scopes, typed resource addresses, templates,
//!   and embedded documents;
//! - a [`DomainServer`] implementation: its tools, a typed `read` for the addresses
//!   it owns, and optional completion;
//! - optional [`TaskSupport`] for durable tasks and subscriptions.
//!
//! This module supplies the rest of the server contract, identically for every
//! server. [`Hosted`] is the one `ServerHandler`, and [`HostedServer`] builds the
//! HTTP service around it: routes, gateway authentication, host validation, health,
//! readiness, administrative documents, the stateless MCP transport, and shutdown.
//! Inside a domain method, [`gateway_identity`], [`forwarded_bearer`] and
//! [`plane_caller`] return the verified caller.
//!
//! ```ignore
//! let server = HostedServer::for_domain::<MyDomain>()
//!     .deployment(&public_deployment, args.allow_loopback_hosts)?
//!     .internal_trust(GatewayInternalTrustBundle::from_json(&args.internal_trust_jwks)?)?
//!     .handler(move || Hosted::new(MyDomain::new(state.clone())))
//!     .build();
//! server.serve(address).await
//! ```
//!
//! [`McpServerSetup`]: crate::server_contract::McpServerSetup

mod admin;
mod auth;
mod catalog;
mod domain;
mod host;
mod results;
mod server;
mod subscriptions;
#[cfg(test)]
mod tests;

pub use auth::{ForwardedBearer, forwarded_bearer, gateway_identity, plane_caller};
pub use catalog::CATALOG_PAGE_SIZE;
pub use domain::{
    DomainAddress, DomainRead, DomainServer, Hosted, NoTasks, ReadCache, TaskSupport,
    served_by_host, unknown_prompt,
};
pub use results::{completion, json_read, product_result, rank_completions, structured_result};
pub use server::{Deployment, HostedServer, HostedServerBuilder, Missing, Provided};
pub use subscriptions::{ResourceSubscriptions, admit_resource_subscriptions, requested_addresses};
