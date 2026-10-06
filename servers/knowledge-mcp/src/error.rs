#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("knowledge indexing machine registration or connection configuration is invalid")]
    MachineConfiguration,
    #[error("knowledge indexing machine authentication failed")]
    MachineAuthentication,
    #[error("another knowledge coordinator holds this tenant's lease")]
    CoordinatorBusy,
    #[error("knowledge input: {0}")]
    Contract(#[from] veoveo_knowledge_contract::KnowledgeError),
    #[error("knowledge source contract is invalid")]
    SourceContract(#[from] veoveo_mcp_knowledge_extension::KnowledgeError),
    #[error("knowledge storage operation failed")]
    Store(#[source] veoveo_platform_store::StoreError),
    #[error(
        "knowledge embedding/generation admission changed; repeat this read or search against current admission"
    )]
    EmbeddingAdmissionChanged,
    #[error("knowledge embedding operation failed")]
    Embedding(#[from] veoveo_embedding_client::EmbeddingClientError),
    #[error("knowledge embedding input is invalid")]
    EmbeddingInput(#[from] veoveo_embedding_contract::EmbeddingError),
    #[error("knowledge source unavailable; member stays stale")]
    SourceUnavailable,
    #[error("knowledge operation exceeded its deadline")]
    Deadline,
    #[error("knowledge source exceeded its traversal bound")]
    Traversal,
    #[error("knowledge candidate budget exhausted; narrow the collection selection")]
    CandidateBudget,
    #[error("knowledge access changed during search; repeat the request")]
    AccessChanged,
    #[error("knowledge embedding runtime does not match the active generation")]
    EmbeddingSpace,
}

impl From<veoveo_platform_store::StoreError> for ServiceError {
    fn from(error: veoveo_platform_store::StoreError) -> Self {
        match error {
            veoveo_platform_store::StoreError::KnowledgeEmbeddingAdmissionChanged => {
                Self::EmbeddingAdmissionChanged
            }
            error => Self::Store(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_embedding_admission_has_an_actionable_public_error() {
        let changed = ServiceError::from(
            veoveo_platform_store::StoreError::KnowledgeEmbeddingAdmissionChanged,
        );
        assert!(matches!(changed, ServiceError::EmbeddingAdmissionChanged));
        assert_eq!(
            changed.to_string(),
            "knowledge embedding/generation admission changed; repeat this read or search against current admission"
        );
        let ordinary = ServiceError::from(veoveo_platform_store::StoreError::Knowledge(
            "unrelated integrity error",
        ));
        assert!(matches!(ordinary, ServiceError::Store(_)));
        assert_eq!(ordinary.to_string(), "knowledge storage operation failed");
        assert!(std::error::Error::source(&ordinary).is_some());
    }
}
