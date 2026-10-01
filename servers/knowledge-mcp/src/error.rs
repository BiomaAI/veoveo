#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("knowledge input: {0}")]
    Contract(#[from] veoveo_knowledge_contract::KnowledgeError),
    #[error("knowledge source contract is invalid")]
    SourceContract(#[from] veoveo_mcp_knowledge_extension::KnowledgeError),
    #[error("knowledge storage operation failed")]
    Store(#[from] veoveo_platform_store::StoreError),
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
