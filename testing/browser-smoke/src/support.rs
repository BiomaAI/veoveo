// Shared installed-client validation and token exchange have one source owner.
#[allow(dead_code)]
#[path = "../../smoke/src/bin/smoke/support/gateway_auth/context.rs"]
mod auth;
#[allow(dead_code)]
#[path = "../../smoke/src/bin/smoke/support/installation.rs"]
mod installation;
pub(crate) use installation::InstalledTarget;
