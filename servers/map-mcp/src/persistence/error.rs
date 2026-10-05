use thiserror::Error;

#[derive(Debug, Error)]
pub enum MapStoreError {
    #[error("invalid map field {field}: {reason}")]
    InvalidMapField {
        field: &'static str,
        reason: &'static str,
    },
    #[error("map {entity} `{key}` conflicts with the current durable record")]
    MapRecordConflict { entity: &'static str, key: String },
    #[error("SurrealDB operation failed: {0}")]
    Database(#[from] surrealdb::Error),
    #[error(transparent)]
    Kernel(#[from] veoveo_platform_store::StoreError),
    #[error("SurrealDB returned no record for {operation}")]
    MissingRecord { operation: &'static str },
}
