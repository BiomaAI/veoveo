use super::admission::Admission;
use super::profiles::{RuntimeRows, registration_query};
use super::*;
use crate::PlatformStore;
use veoveo_embedding_contract::QualifiedEmbeddingRuntime;
use veoveo_knowledge_contract::{CollectionRegistration, GenerationSpec, IndexedMember};
use veoveo_types::{ResourceUri, Sha256Digest};

/// Native indexed row; links and dates stay native while observations use the owner codec.
#[derive(Debug, SurrealValue)]
struct IndexedChunkRecord {
    member: RecordId,
    #[surreal(wrap)]
    tenant: TenantId,
    collection: RecordId,
    #[surreal(wrap)]
    collection_id: CollectionId,
    #[surreal(wrap)]
    approval_revision: Sha256Digest,
    #[surreal(wrap)]
    uri: ResourceUri,
    ordinal: i64,
    admission: Admission,
    text: String,
    #[surreal(wrap)]
    title: veoveo_knowledge_contract::MemberTitle,
    embedding: Vec<f32>,
    producer_batch: RecordId,
    producer_profile: RecordId,
    observation: Document<veoveo_mcp_knowledge_extension::Observation>,
}

#[derive(Debug, Clone)]
pub struct MemberReadTicket {
    lease: CoordinatorLease,
    source_epoch: i64,
    previous: Option<veoveo_mcp_knowledge_extension::Observation>,
    title: Option<veoveo_knowledge_contract::MemberTitle>,
    freshness: veoveo_mcp_knowledge_extension::Freshness,
    tenant: TenantId,
    generation: GenerationId,
    collection: CollectionId,
    approval: Sha256Digest,
    specification: Sha256Digest,
    uri: ResourceUri,
    epoch: i64,
    runtime: RuntimeRows,
}
impl MemberReadTicket {
    pub fn previous(&self) -> Option<&veoveo_mcp_knowledge_extension::Observation> {
        self.previous.as_ref()
    }
    pub fn title(&self) -> Option<&veoveo_knowledge_contract::MemberTitle> {
        self.title.as_ref()
    }

    fn record(&self) -> RecordId {
        member_record(self.generation, &self.collection, &self.uri)
    }
}
fn member_record(
    generation: GenerationId,
    collection: &CollectionId,
    uri: &ResourceUri,
) -> RecordId {
    RecordId::new(
        "knowledge_member",
        Array::from(vec![
            generation.to_string(),
            collection.to_string(),
            uri.to_string(),
        ]),
    )
}

impl PlatformStore {
    /// Call only after the owner definitively confirms deletion. A failed read,
    /// timeout or disconnected subscription never establishes that fact.
    pub async fn confirm_knowledge_member_deleted(
        &self,
        ticket: &MemberReadTicket,
    ) -> Result<(), StoreError> {
        let _mutation = ticket.lease.mutation().await;
        let sql = include_str!("../queries/knowledge/delete_member.surql");
        ticket
            .lease
            .bind(self.client().query(sql))
            .bind(("sync", sync_record(ticket.generation, &ticket.collection)))
            .bind(("source_epoch", ticket.source_epoch))
            .bind(("tenant", ticket.tenant.to_string()))
            .bind(("generation", generation_record(ticket.generation)))
            .bind((
                "collection",
                collection_record(&ticket.tenant, &ticket.collection),
            ))
            .bind(("approval", ticket.approval.to_string()))
            .bind(("chunk_table", chunk_table(ticket.generation)))
            .bind(("member", ticket.record()))
            .bind(("epoch", ticket.epoch))
            .await?
            .knowledge_check()?;
        Ok(())
    }
    /// Preserve existing vectors only when the source revalidates every observation field.
    pub async fn revalidate_knowledge_member(
        &self,
        ticket: &MemberReadTicket,
        observation: &veoveo_mcp_knowledge_extension::Observation,
    ) -> Result<(), StoreError> {
        let _mutation = ticket.lease.mutation().await;
        let previous = ticket.previous.as_ref().ok_or(StoreError::Knowledge(
            "conditional member has no cached observation",
        ))?;
        let current = observation
            .revalidated(previous)
            .map_err(|error| StoreError::Knowledge(error.0))?;
        let sql = registration_query(include_str!("../queries/knowledge/revalidate.surql"));
        ticket
            .runtime
            .bind(
                ticket
                    .lease
                    .bind(self.client().query(sql))
                    .bind(("chunk_table", chunk_table(ticket.generation)))
                    .bind(("member", ticket.record()))
                    .bind(("generation", generation_record(ticket.generation)))
                    .bind(("epoch", ticket.epoch))
                    .bind(("sync", sync_record(ticket.generation, &ticket.collection)))
                    .bind(("source_epoch", ticket.source_epoch))
                    .bind((
                        "collection",
                        collection_record(&ticket.tenant, &ticket.collection),
                    ))
                    .bind(("approval", ticket.approval.to_string()))
                    .bind(("previous", Document(previous.clone())))
                    .bind(("observation", Document(current.clone())))
                    .bind(("valid_until", valid_until(&current, ticket.freshness)?)),
            )
            .await?
            .knowledge_check()?;
        Ok(())
    }

    /// Reserve before the source read. Calling this for an invalidation immediately
    /// hides old chunks and fences every earlier in-flight read of this member.
    pub async fn begin_knowledge_member_read(
        &self,
        lease: &CoordinatorLease,
        registration: &CollectionRegistration,
        generation: GenerationId,
        specification: &GenerationSpec,
        uri: &ResourceUri,
        runtime: &QualifiedEmbeddingRuntime,
    ) -> Result<MemberReadTicket, StoreError> {
        let _mutation = lease.mutation().await;
        lease.check_tenant(&registration.tenant)?;
        if specification.space() != runtime.space() {
            return Err(StoreError::Knowledge(
                "read ticket uses another qualified embedding space",
            ));
        }
        let runtime_rows = RuntimeRows::new(runtime);
        let sql = registration_query(include_str!("../queries/knowledge/begin_read.surql"));
        let collection = registration.descriptor.collection();
        let mut response = runtime_rows
            .bind(
                lease
                    .bind(self.client().query(sql))
                    .bind(("sync", sync_record(generation, collection)))
                    .bind(("member", member_record(generation, collection, uri)))
                    .bind(("tenant", registration.tenant.to_string()))
                    .bind(("generation", generation_record(generation)))
                    .bind((
                        "collection",
                        collection_record(&registration.tenant, collection),
                    ))
                    .bind(("coverage", coverage_record(generation, collection)))
                    .bind(("approval", registration.revision().to_string()))
                    .bind(("specification", specification.revision().to_string()))
                    .bind(("uri", uri.to_string())),
            )
            .await?
            .knowledge_check()?;
        let index = response
            .num_statements()
            .checked_sub(2)
            .ok_or(StoreError::Knowledge("read ticket response missing"))?;
        #[derive(SurrealValue)]
        struct Prior {
            epoch: i64,
            source_epoch: i64,
            observation: Option<Document<veoveo_mcp_knowledge_extension::Observation>>,
            title: Option<String>,
        }
        let prior: Option<Prior> = response.take(index)?;
        let prior = prior.ok_or(StoreError::Knowledge("member read was not reserved"))?;
        Ok(MemberReadTicket {
            lease: lease.clone(),
            freshness: registration.descriptor.freshness(),
            source_epoch: prior.source_epoch,
            previous: prior.observation.map(|d| d.0),
            title: prior
                .title
                .map(veoveo_knowledge_contract::MemberTitle::new)
                .transpose()
                .map_err(|_| StoreError::Knowledge("invalid member title"))?,
            tenant: registration.tenant.clone(),
            generation,
            collection: collection.clone(),
            approval: registration.revision(),
            specification: specification.revision(),
            uri: uri.clone(),
            epoch: prior.epoch,
            runtime: runtime_rows,
        })
    }

    pub async fn replace_knowledge_member(
        &self,
        ticket: &MemberReadTicket,
        member: &IndexedMember,
    ) -> Result<(), StoreError> {
        let _mutation = ticket.lease.mutation().await;
        if member.uri() != &ticket.uri
            || member.observation().collection() != &ticket.collection
            || member.generation_revision() != &ticket.specification
            || member.chunks().iter().any(|chunk| {
                RecordId::new(
                    "knowledge_embedding_profile",
                    chunk.vector().profile_id().as_ref(),
                ) != *ticket.runtime.producer()
            })
        {
            return Err(StoreError::Knowledge(
                "source read does not match its ticket",
            ));
        }
        let table = chunk_table(ticket.generation);
        let batch = RecordId::new(
            "knowledge_embedding_batch",
            Array::from(vec![
                generation_record(ticket.generation).into_value(),
                ticket.record().into_value(),
                ticket.epoch.into_value(),
            ]),
        );
        let chunks: Vec<_> = member
            .chunks()
            .iter()
            .enumerate()
            .map(|(ordinal, chunk)| IndexedChunkRecord {
                member: ticket.record(),
                tenant: ticket.tenant.clone(),
                collection: collection_record(&ticket.tenant, &ticket.collection),
                collection_id: ticket.collection.clone(),
                approval_revision: ticket.approval.clone(),
                uri: ticket.uri.clone(),
                ordinal: ordinal as i64,
                admission: Admission::from(member.observation().access()),
                text: chunk.text().to_owned(),
                title: member.title().clone(),
                embedding: chunk.vector().values().to_vec(),
                producer_batch: batch.clone(),
                producer_profile: ticket.runtime.producer().clone(),
                observation: Document(member.observation().clone()),
            })
            .collect();
        let sql = registration_query(include_str!("../queries/knowledge/replace.surql"));
        ticket
            .runtime
            .bind(
                ticket
                    .lease
                    .bind(self.client().query(sql))
                    .bind(("chunk_target", surrealdb::types::Table::new(table)))
                    .bind(("sync", sync_record(ticket.generation, &ticket.collection)))
                    .bind(("source_epoch", ticket.source_epoch))
                    .bind(("title", member.title().as_str().to_owned()))
                    .bind((
                        "valid_until",
                        valid_until(member.observation(), ticket.freshness)?,
                    ))
                    .bind(("tenant", ticket.tenant.to_string()))
                    .bind(("generation", generation_record(ticket.generation)))
                    .bind(("specification", ticket.specification.to_string()))
                    .bind((
                        "collection",
                        collection_record(&ticket.tenant, &ticket.collection),
                    ))
                    .bind(("approval", ticket.approval.to_string()))
                    .bind(("chunk_table", chunk_table(ticket.generation)))
                    .bind(("member", ticket.record()))
                    .bind(("epoch", ticket.epoch))
                    .bind(("embedding_batch", batch))
                    .bind(("chunks", chunks))
                    .bind(("observation", Document(member.observation().clone()))),
            )
            .await?
            .knowledge_check()?;
        Ok(())
    }
}

fn valid_until(
    observation: &veoveo_mcp_knowledge_extension::Observation,
    freshness: veoveo_mcp_knowledge_extension::Freshness,
) -> Result<Option<chrono::DateTime<chrono::Utc>>, StoreError> {
    match freshness {
        veoveo_mcp_knowledge_extension::Freshness::Immutable { .. } => Ok(None),
        veoveo_mcp_knowledge_extension::Freshness::MaxAge { max_age_seconds } => observation
            .observed_at()
            .checked_add_signed(chrono::TimeDelta::seconds(i64::from(max_age_seconds)))
            .map(Some)
            .ok_or(StoreError::Knowledge("invalid source freshness deadline")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_record_keeps_native_links_floats_and_observation() {
        let descriptor = veoveo_mcp_knowledge_extension::docs::collection(
            &"knowledge".parse().unwrap(),
            &"knowledge".parse().unwrap(),
        );
        let observation = veoveo_mcp_knowledge_extension::Observation::builder(
            descriptor.collection().clone(),
            veoveo_mcp_knowledge_extension::Revision::parse("revision-1").unwrap(),
            veoveo_mcp_knowledge_extension::content_digest("body"),
            chrono::Utc::now(),
        )
        .build(&descriptor)
        .unwrap();
        let tenant: TenantId = "tenant".parse().unwrap();
        let collection = descriptor.collection().clone();
        let uri = ResourceUri::new("knowledge://docs/design").unwrap();
        let member = member_record(GenerationId::new(), &collection, &uri);
        let value = IndexedChunkRecord {
            producer_batch: RecordId::new("knowledge_embedding_batch", "fixture"),
            producer_profile: RecordId::new("knowledge_embedding_profile", "fixture"),
            member: member.clone(),
            tenant: tenant.clone(),
            collection: collection_record(&tenant, &collection),
            collection_id: collection,
            approval_revision: Sha256Digest::from_bytes([7; 32]),
            uri,
            ordinal: 0,
            admission: Admission::from(observation.access()),
            text: "body".into(),
            title: veoveo_knowledge_contract::MemberTitle::new("Title").unwrap(),
            embedding: vec![1.0, 0.0, 0.0],
            observation: Document(observation.clone()),
        }
        .into_value();
        let Value::Object(mut fields) = value.clone() else {
            panic!("chunk record");
        };
        assert_eq!(fields.get("member"), Some(&member.into_value()));
        assert_eq!(
            fields.get("embedding"),
            Some(&vec![1_f32, 0.0, 0.0].into_value())
        );
        assert_eq!(
            fields.get("observation"),
            Some(&Document(observation.clone()).into_value())
        );
        assert_eq!(
            IndexedChunkRecord::from_value(value).unwrap().observation.0,
            observation
        );
        fields.insert("member", "not-a-native-record".into_value());
        assert!(IndexedChunkRecord::from_value(fields.into_value()).is_err());
    }
}
