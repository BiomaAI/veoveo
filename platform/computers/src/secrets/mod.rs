//! Installation-owned encryption for commands and maintenance. These private
//! values never belong in a Task request, audit event or public operation view.
mod cipher;
mod file_access;
mod files;
mod maintenance;
mod output_access;
mod payload;
pub use cipher::{ComputerKeyRing, ComputerSealingKey, SealedCommand};
pub use file_access::{FileTransferAccess, SealedFileTransferAccess};
pub use files::{FileTransferBinding, FileTransferPayload, SealedFileTransfer};
pub use maintenance::{MaintenanceBinding, MaintenanceCheckpoint, SealedMaintenanceCheckpoint};
pub use output_access::{CommandOutputAccess, SealedOutputAccess};
pub use payload::{CommandBinding, CommandPayload};

#[cfg(test)]
mod tests;
