//! Immutable execution registry and same-snapshot admission of retained producers.
use super::*;
use crate::PlatformStore;
use veoveo_embedding_contract::{
    EmbeddingExecutionProfile, EmbeddingExecutionProfileId, EmbeddingQualification,
    QualifiedEmbeddingRuntime,
};
use veoveo_types::Sha256Digest;

fn profile_record(id: &EmbeddingExecutionProfileId) -> RecordId {
    RecordId::new("knowledge_embedding_profile", id.as_ref())
}

#[derive(Debug, Clone)]
pub(super) struct ExecutionDocument<T>(pub(super) T);
impl<T: Serialize + DeserializeOwned> SurrealValue for ExecutionDocument<T> {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        crate::native_json_into_value(
            serde_json::to_value(self.0).expect("admitted execution facts serialize"),
        )
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(crate::native_json_from_value_strict(value)?)
            .map(Self)
            .map_err(|_| Error::internal("invalid embedding execution facts".into()))
    }
}

#[derive(Debug, Clone, SurrealValue)]
struct ProfileRow {
    id: RecordId,
    document: ExecutionDocument<EmbeddingExecutionProfile>,
}
impl ProfileRow {
    fn checked(&self) -> Result<&EmbeddingExecutionProfile, StoreError> {
        if self.id != profile_record(self.document.0.id()) {
            return Err(StoreError::Knowledge(
                "embedding profile row identity mismatch",
            ));
        }
        Ok(&self.document.0)
    }
}

#[derive(Debug, Clone, SurrealValue)]
struct QualificationRow {
    id: RecordId,
    query_profile: RecordId,
    producer_profile: RecordId,
    document: ExecutionDocument<EmbeddingQualification>,
}
impl QualificationRow {
    fn checked(&self) -> Result<&EmbeddingQualification, StoreError> {
        let document = &self.document.0;
        if self.id != RecordId::new("knowledge_embedding_qualification", document.id().as_ref())
            || self.query_profile != profile_record(document.query_profile())
            || self.producer_profile != profile_record(document.producer_profile())
        {
            return Err(StoreError::Knowledge(
                "embedding qualification lookup mismatch",
            ));
        }
        Ok(document)
    }
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeRows {
    profiles: Vec<ProfileRow>,
    qualifications: Vec<QualificationRow>,
    producer: RecordId,
    qualified_producers: Vec<RecordId>,
    space_revision: Sha256Digest,
}
impl RuntimeRows {
    pub(super) fn new(runtime: &QualifiedEmbeddingRuntime) -> Self {
        let profiles = runtime
            .profiles()
            .iter()
            .map(|profile| ProfileRow {
                id: profile_record(profile.id()),
                document: ExecutionDocument(profile.clone()),
            })
            .collect();
        let qualifications = runtime
            .qualifications()
            .iter()
            .filter(|entry| entry.query_profile() == runtime.profile().id())
            .map(|qualification| QualificationRow {
                id: RecordId::new(
                    "knowledge_embedding_qualification",
                    qualification.id().as_ref(),
                ),
                query_profile: profile_record(qualification.query_profile()),
                producer_profile: profile_record(qualification.producer_profile()),
                document: ExecutionDocument(qualification.clone()),
            })
            .collect();
        let qualified_producers = runtime
            .qualifications()
            .iter()
            .filter(|entry| entry.query_profile() == runtime.profile().id())
            .map(|entry| profile_record(entry.producer_profile()))
            .collect();
        Self {
            profiles,
            qualifications,
            producer: profile_record(runtime.profile().id()),
            qualified_producers,
            space_revision: runtime.space().revision(),
        }
    }
    pub(super) fn bind<'a, C: surrealdb::Connection>(
        &self,
        query: surrealdb::method::Query<'a, C>,
    ) -> surrealdb::method::Query<'a, C> {
        query
            .bind(("embedding_profiles", self.profiles.clone()))
            .bind(("embedding_qualifications", self.qualifications.clone()))
            .bind((
                "embedding_qualification_records",
                self.qualifications
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<Vec<_>>(),
            ))
            .bind(("embedding_producer", self.producer.clone()))
            .bind((
                "embedding_qualified_producers",
                self.qualified_producers.clone(),
            ))
            .bind(("embedding_space_revision", self.space_revision.to_string()))
    }
    pub(super) fn producer(&self) -> &RecordId {
        &self.producer
    }
}

pub(super) fn registration_query(sql: &str) -> String {
    sql.replace(
        "-- REGISTER_EMBEDDING_RUNTIME",
        include_str!("../queries/knowledge/register_embedding_runtime.surql"),
    )
    .replace(
        "-- VALIDATE_EMBEDDING_RUNTIME",
        include_str!("../queries/knowledge/validate_embedding_runtime.surql"),
    )
}

/// Immutable admission captured from one native transaction; ranking fences it again.
#[derive(Debug, Clone)]
pub struct EmbeddingAdmission {
    tenant: TenantId,
    generation: GenerationId,
    pub(super) epoch: i64,
    pub(super) producer_records: Vec<RecordId>,
    pub(super) rows: RuntimeRows,
}
impl EmbeddingAdmission {
    pub(super) fn matches(
        &self,
        tenant: &TenantId,
        generation: GenerationId,
        vector: &veoveo_embedding_contract::EmbeddingVector,
    ) -> bool {
        &self.tenant == tenant
            && self.generation == generation
            && self.rows.producer == profile_record(vector.profile_id())
            && self.rows.space_revision == vector.space().revision()
    }
    pub(super) fn bind<'a, C: surrealdb::Connection>(
        &self,
        query: surrealdb::method::Query<'a, C>,
    ) -> surrealdb::method::Query<'a, C> {
        self.rows
            .bind(query)
            .bind(("embedding_epoch", self.epoch))
            .bind((
                "embedding_retained_producers",
                self.producer_records.clone(),
            ))
    }
}

impl PlatformStore {
    pub async fn admit_knowledge_embeddings(
        &self,
        tenant: &TenantId,
        generation: GenerationId,
        runtime: &QualifiedEmbeddingRuntime,
    ) -> Result<EmbeddingAdmission, StoreError> {
        let rows = RuntimeRows::new(runtime);
        let sql = registration_query(include_str!(
            "../queries/knowledge/embedding_admission.surql"
        ));
        let mut response = rows
            .bind(self.client().query(sql))
            .bind(("tenant", tenant.to_string()))
            .bind(("generation", generation_record(generation)))
            .await?
            .knowledge_check()?;
        #[derive(SurrealValue)]
        struct Snapshot {
            epoch: i64,
            #[surreal(wrap)]
            space_revision: Sha256Digest,
            #[surreal(wrap)]
            revision: Sha256Digest,
            document: ExecutionDocument<veoveo_knowledge_contract::GenerationSpec>,
            producer_records: Vec<RecordId>,
            profiles: Vec<ProfileRow>,
            qualifications: Vec<QualificationRow>,
        }
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::Knowledge(
                "embedding admission snapshot missing",
            ))?;
        let snapshot: Option<Snapshot> = response.take(index)?;
        let snapshot = snapshot.ok_or(StoreError::Knowledge(
            "embedding admission generation missing",
        ))?;
        if snapshot.epoch <= 0
            || snapshot.producer_records.len() > 64
            || snapshot
                .producer_records
                .iter()
                .enumerate()
                .any(|(index, id)| snapshot.producer_records[..index].contains(id))
            || snapshot.space_revision != runtime.space().revision()
            || snapshot.space_revision != snapshot.document.0.space().revision()
            || snapshot.revision != snapshot.document.0.revision()
            || snapshot.profiles.len() != snapshot.producer_records.len()
        {
            return Err(StoreError::Knowledge(
                "generation execution provenance is incomplete or mismatched",
            ));
        }
        for id in &snapshot.producer_records {
            let row = snapshot
                .profiles
                .iter()
                .find(|entry| &entry.id == id)
                .ok_or(StoreError::Knowledge(
                    "retained embedding producer is missing",
                ))?;
            let profile = row.checked()?;
            if runtime
                .producer(profile.id())
                .map_err(|e| StoreError::Knowledge(e.0))?
                != profile
            {
                return Err(StoreError::Knowledge(
                    "retained embedding producer contents changed",
                ));
            }
            let expected = runtime
                .qualification_for(profile.id())
                .map_err(|e| StoreError::Knowledge(e.0))?;
            let qualifier = snapshot
                .qualifications
                .iter()
                .find(|entry| entry.document.0.id() == expected.id())
                .ok_or(StoreError::Knowledge(
                    "retained producer qualification is absent",
                ))?;
            if qualifier.checked()? != expected {
                return Err(StoreError::Knowledge(
                    "retained producer qualification contents changed",
                ));
            }
        }
        Ok(EmbeddingAdmission {
            tenant: tenant.clone(),
            generation,
            epoch: snapshot.epoch,
            producer_records: snapshot.producer_records,
            rows,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_documents_reject_native_values_and_undeclared_fields() {
        let space = veoveo_embedding_contract::EmbeddingSpace {
            model: "synthetic".parse().unwrap(),
            revision: "fixture".parse().unwrap(),
            dimension: veoveo_embedding_contract::EmbeddingDimension::new(3).unwrap(),
            pooling: veoveo_embedding_contract::EmbeddingPooling::LastToken,
            normalization: veoveo_embedding_contract::EmbeddingNormalization::L2,
            precision: veoveo_embedding_contract::EmbeddingPrecision::Float32,
            max_input_tokens: veoveo_embedding_contract::EmbeddingMaxInputTokens::new(8192)
                .unwrap(),
        };
        for invalid in [
            Value::None,
            RecordId::new("principal", "fixture").into_value(),
            chrono::Utc::now().into_value(),
        ] {
            let Value::Object(mut fields) =
                crate::native_json_into_value(serde_json::to_value(&space).unwrap())
            else {
                panic!("space object")
            };
            fields.insert("extra", invalid);
            assert!(
                ExecutionDocument::<veoveo_embedding_contract::EmbeddingSpace>::from_value(
                    Value::Object(fields)
                )
                .is_err()
            );
        }
        let spec = veoveo_knowledge_contract::GenerationSpec::new(
            space.clone(),
            "Retrieve records",
            veoveo_knowledge_contract::ChunkSettings::new("structure-v1", 1000, 0).unwrap(),
            [(
                "fixture.records".parse().unwrap(),
                Sha256Digest::from_bytes([1; 32]),
            )]
            .into(),
        )
        .unwrap();
        let value = ExecutionDocument(spec.clone()).into_value();
        assert_eq!(
            ExecutionDocument::<veoveo_knowledge_contract::GenerationSpec>::from_value(
                value.clone()
            )
            .unwrap()
            .0
            .revision(),
            spec.revision()
        );
        let Value::Object(mut fields) = value else {
            panic!("generation object")
        };
        fields.insert("unknown", Value::None);
        assert!(
            ExecutionDocument::<veoveo_knowledge_contract::GenerationSpec>::from_value(
                Value::Object(fields.clone())
            )
            .is_err()
        );
        fields.remove("unknown");
        let Some(Value::Object(space_fields)) = fields.get_mut("space") else {
            panic!("space object")
        };
        space_fields.insert("unknown", Value::None);
        assert!(
            ExecutionDocument::<veoveo_knowledge_contract::GenerationSpec>::from_value(
                Value::Object(fields)
            )
            .is_err()
        );
        let value = ExecutionDocument(space.clone()).into_value();
        assert_eq!(
            ExecutionDocument::<veoveo_embedding_contract::EmbeddingSpace>::from_value(value)
                .unwrap()
                .0,
            space
        );
    }
}
