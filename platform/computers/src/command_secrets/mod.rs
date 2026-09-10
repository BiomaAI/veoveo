//! Installation-owned encryption for recoverable queued commands. These private
//! values never belong in a Task request, audit event or public operation view.
mod cipher;
mod payload;
pub use cipher::{CommandKeyRing, CommandSealingKey, SealedCommand};
pub use payload::{CommandBinding, CommandPayload};

#[cfg(test)]
mod tests;
