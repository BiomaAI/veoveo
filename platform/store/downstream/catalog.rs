//! Fork-owned catalog. Append local migrations here instead of modifying upstream history.
use crate::migrations::DownstreamMigration;

pub(crate) const MIGRATIONS: &[DownstreamMigration] = &[];
