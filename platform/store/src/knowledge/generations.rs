use super::profiles::{ExecutionDocument, RuntimeRows, registration_query};
use super::*;
use crate::PlatformStore;
use veoveo_embedding_contract::QualifiedEmbeddingRuntime;
use veoveo_knowledge_contract::GenerationSpec;

#[derive(Debug, Clone, SurrealValue)]
struct CollectionRequirementRecord {
    record: RecordId,
    #[surreal(wrap)]
    revision: veoveo_types::Sha256Digest,
}

impl PlatformStore {
    /// Reclaim a retired or abandoned generation. The active pointer is checked
    /// in the same transaction that cascades member records and drops its indexes.
    pub async fn remove_knowledge_generation(
        &self,
        lease: &CoordinatorLease,
        tenant: &TenantId,
        generation: GenerationId,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(tenant)?;
        let sql = include_str!("../queries/knowledge/remove_generation.surql");
        lease
            .bind(self.client().query(sql))
            .bind(("tenant", tenant.to_string()))
            .bind(("chunk_table", chunk_table(generation)))
            .bind(("generation", generation_record(generation)))
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .await?
            .knowledge_check()?;
        Ok(())
    }
    pub async fn active_knowledge_generation(
        &self,
        tenant: &TenantId,
    ) -> Result<Option<GenerationId>, StoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/knowledge/generations/active_knowledge_generation.surql"
            ))
            .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
            .bind(("tenant", tenant.to_string()))
            .await?
            .knowledge_check()?;
        let id: Option<String> = response.take(0)?;
        id.map(|id| {
            id.try_into()
                .map_err(|_| StoreError::Knowledge("invalid active generation identity"))
        })
        .transpose()
    }
    pub async fn create_knowledge_generation(
        &self,
        lease: &CoordinatorLease,
        tenant: &TenantId,
        id: GenerationId,
        spec: &GenerationSpec,
        runtime: &QualifiedEmbeddingRuntime,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(tenant)?;
        if spec.space() != runtime.space() {
            return Err(StoreError::Knowledge(
                "generation uses another qualified embedding space",
            ));
        }
        // The native HNSW grammar requires a literal dimension from the checked spec.
        let table = chunk_table(id);
        let schema = registration_query(include_str!("../queries/knowledge/generation.surql"))
            .replace("__DIMENSION__", &spec.space().dimension.get().to_string());
        let collections: Vec<_> = spec
            .collections()
            .iter()
            .map(|(collection, revision)| CollectionRequirementRecord {
                record: collection_record(tenant, collection),
                revision: revision.clone(),
            })
            .collect();
        RuntimeRows::new(runtime)
            .bind(
                lease
                    .bind(self.client().query(&schema))
                    .bind(("chunk_table", table))
                    .bind(("generation", generation_record(id)))
                    .bind(("tenant", tenant.to_string()))
                    .bind(("document", ExecutionDocument(spec.clone())))
                    .bind(("revision", spec.revision().to_string()))
                    .bind(("collections", collections)),
            )
            .await?
            .knowledge_check()?;
        Ok(())
    }

    pub async fn knowledge_generation(
        &self,
        tenant: &TenantId,
        id: GenerationId,
    ) -> Result<Option<GenerationSpec>, StoreError> {
        let mut response = self
            .client()
            .query(include_str!(
                "../queries/knowledge/generations/knowledge_generation.surql"
            ))
            .bind(("generation", generation_record(id)))
            .bind(("tenant", tenant.to_string()))
            .await?
            .knowledge_check()?;
        #[derive(SurrealValue)]
        struct GenerationRow {
            document: ExecutionDocument<GenerationSpec>,
            #[surreal(wrap)]
            revision: veoveo_types::Sha256Digest,
            #[surreal(wrap)]
            space_revision: veoveo_types::Sha256Digest,
        }
        let row: Option<GenerationRow> = response.take(0)?;
        row.map(|row| {
            if row.revision != row.document.0.revision()
                || row.space_revision != row.document.0.space().revision()
            {
                return Err(StoreError::Knowledge(
                    "generation lookup disagrees with its specification",
                ));
            }
            Ok(row.document.0)
        })
        .transpose()
    }

    pub async fn activate_knowledge_generation(
        &self,
        lease: &CoordinatorLease,
        tenant: &TenantId,
        generation: GenerationId,
        previous: Option<GenerationId>,
        runtime: &QualifiedEmbeddingRuntime,
    ) -> Result<(), StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(tenant)?;
        let sql = registration_query(include_str!("../queries/knowledge/activate.surql"));
        RuntimeRows::new(runtime)
            .bind(
                lease
                    .bind(self.client().query(sql))
                    .bind(("tenant", tenant.to_string()))
                    .bind(("generation", generation_record(generation)))
                    .bind(("active", RecordId::new("knowledge_active", tenant.as_str())))
                    .bind(("previous", previous.map(generation_record))),
            )
            .await?
            .knowledge_check()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requirement_record_preserves_native_links_and_admits_digest() {
        let record = collection_record(
            &"tenant".parse().unwrap(),
            &"fixture.records".parse().unwrap(),
        );
        let revision = veoveo_types::Sha256Digest::from_bytes([7; 32]);
        let value = CollectionRequirementRecord {
            record: record.clone(),
            revision: revision.clone(),
        }
        .into_value();
        let Value::Object(mut fields) = value.clone() else {
            panic!("native requirement object");
        };
        assert_eq!(fields.get("record"), Some(&record.into_value()));
        assert_eq!(
            fields.get("revision"),
            Some(&revision.to_string().into_value())
        );
        let decoded = CollectionRequirementRecord::from_value(value).unwrap();
        assert_eq!(decoded.revision, revision);
        fields.insert("revision", "not-a-digest".into_value());
        assert!(CollectionRequirementRecord::from_value(fields.clone().into_value()).is_err());
        fields.remove("revision");
        assert!(CollectionRequirementRecord::from_value(fields.into_value()).is_err());
    }
}
