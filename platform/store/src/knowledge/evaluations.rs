use super::*;
use crate::PlatformStore;
use veoveo_knowledge_contract::RetrievalEvaluation;
use veoveo_types::Sha256Digest;

impl PlatformStore {
    /// Append a historical measurement without changing generation identity or
    /// activation. Repeating the same complete report is idempotent.
    pub async fn record_knowledge_evaluation(
        &self,
        tenant: &TenantId,
        evaluation: &RetrievalEvaluation,
    ) -> Result<Sha256Digest, StoreError> {
        let specification = self
            .knowledge_generation(tenant, evaluation.generation())
            .await?
            .ok_or(StoreError::Knowledge(
                "evaluation generation is absent in this tenant",
            ))?;
        if evaluation.specification_revision() != &specification.revision()
            || evaluation
                .dataset()
                .collections()
                .iter()
                .any(|id| !specification.collections().contains_key(id))
        {
            return Err(StoreError::Knowledge(
                "evaluation does not match its generation specification",
            ));
        }
        let revision = evaluation.revision();
        self.client()
            .query(include_str!("evaluation.surql"))
            .bind(("tenant", tenant.to_string()))
            .bind(("generation", generation_record(evaluation.generation())))
            .bind((
                "specification",
                evaluation.specification_revision().to_string(),
            ))
            .bind((
                "evaluation",
                RecordId::new("knowledge_evaluation", revision.to_string()),
            ))
            .bind(("document", Document(evaluation.clone())))
            .await?
            .knowledge_check()?;
        Ok(revision)
    }

    pub async fn knowledge_evaluation(
        &self,
        tenant: &TenantId,
        generation: GenerationId,
        revision: &Sha256Digest,
    ) -> Result<Option<RetrievalEvaluation>, StoreError> {
        let mut response = self.client().query("SELECT VALUE document FROM ONLY $evaluation WHERE tenant = $tenant AND generation = $generation;")
            .bind(("evaluation", RecordId::new("knowledge_evaluation", revision.to_string())))
            .bind(("tenant", tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .await?.knowledge_check()?;
        let document: Option<Document<RetrievalEvaluation>> = response.take(0)?;
        document
            .map(|document| {
                let evaluation = document.0;
                if evaluation.generation() != generation || &evaluation.revision() != revision {
                    return integrity();
                }
                Ok(evaluation)
            })
            .transpose()
    }
}
