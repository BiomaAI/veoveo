use crate::{
    ServiceError, chunk,
    embed::Embeddings,
    source::{KnowledgeSource, MemberLink, enumeration_uri},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};
use veoveo_embedding_contract::{EmbeddingBatch, EmbeddingText};
use veoveo_knowledge_contract::{
    CollectionApproval, CollectionRegistration, GenerationId, GenerationSpec, IndexedChunk,
    IndexedMember, KnowledgeError, MemberTitle, metadata_text,
};
use veoveo_mcp_knowledge_extension::IndexingMode;
use veoveo_platform_store::PlatformStore;
use veoveo_types::TenantId;

pub struct Indexer<'a, S, E> {
    pub store: &'a PlatformStore,
    pub lease: &'a veoveo_platform_store::knowledge::CoordinatorLease,
    pub source: &'a S,
    pub embeddings: &'a E,
}
impl<S: KnowledgeSource, E: Embeddings> Indexer<'_, S, E> {
    /// Build an inactive generation. The coordinator owns source listeners and
    /// activation after enumeration and queued invalidations have settled.
    pub async fn prepare(
        &self,
        tenant: &TenantId,
        registrations: &[CollectionRegistration],
        specification: &GenerationSpec,
    ) -> Result<GenerationId, ServiceError> {
        self.validate_inputs(tenant, registrations, specification)?;
        let generation = GenerationId::new();
        self.store
            .create_knowledge_generation(self.lease, tenant, generation, specification)
            .await?;
        Ok(generation)
    }

    pub(crate) fn validate_inputs(
        &self,
        tenant: &TenantId,
        registrations: &[CollectionRegistration],
        specification: &GenerationSpec,
    ) -> Result<(), ServiceError> {
        self.validate_specification(specification)?;
        if self.embeddings.space() != specification.space() {
            return Err(ServiceError::EmbeddingSpace);
        }
        let expected: BTreeMap<_, _> = registrations
            .iter()
            .map(|r| (r.descriptor.collection().clone(), r.revision()))
            .collect();
        if registrations.len() != expected.len()
            || &expected != specification.collections()
            || registrations
                .iter()
                .any(|r| &r.tenant != tenant || r.approval.mode != CollectionApproval::Index)
        {
            return Err(KnowledgeError(
                "generation must name each approved tenant collection once",
            )
            .into());
        }
        Ok(())
    }

    pub async fn build(
        &self,
        tenant: &TenantId,
        registrations: &[CollectionRegistration],
        specification: &GenerationSpec,
    ) -> Result<GenerationId, ServiceError> {
        let generation = self.prepare(tenant, registrations, specification).await?;
        // Failed builds stay inactive and can be explicitly reclaimed.
        tokio::time::timeout(Duration::from_secs(3600), async {
            for registration in registrations {
                let sync = self
                    .store
                    .knowledge_collection_sync(self.lease, registration, generation)
                    .await?;
                self.populate(registration, generation, specification, &sync)
                    .await?;
            }
            Ok::<_, ServiceError>(())
        })
        .await
        .map_err(|_| ServiceError::Deadline)??;
        Ok(generation)
    }

    async fn populate(
        &self,
        registration: &CollectionRegistration,
        generation: GenerationId,
        specification: &GenerationSpec,
        sync: &veoveo_platform_store::knowledge::CollectionSyncTicket,
    ) -> Result<(), ServiceError> {
        let mut cursor = None;
        let mut cursors = BTreeSet::new();
        let mut seen = BTreeSet::new();
        for _ in 0..10_000 {
            let uri = enumeration_uri(&registration.descriptor, cursor.as_deref())?;
            let page = tokio::time::timeout(
                Duration::from_secs(30),
                self.source.enumerate(&registration.descriptor, uri),
            )
            .await
            .map_err(|_| ServiceError::Deadline)??;
            for member in page.items() {
                if !seen.insert(member.uri.clone()) || seen.len() > 100_000 {
                    return Err(ServiceError::Traversal);
                }
                self.refresh(registration, generation, specification, member)
                    .await?;
            }
            let Some(next) = page.next_cursor() else {
                self.store.complete_knowledge_collection(sync).await?;
                return Ok(());
            };
            if !cursors.insert(next.to_owned()) {
                return Err(ServiceError::Traversal);
            }
            cursor = Some(next.to_owned());
        }
        Err(ServiceError::Traversal)
    }

    /// Reconcile a complete source enumeration under a new source epoch. Missing
    /// old members stay hidden; enumeration absence does not assert deletion.
    pub async fn reconcile(
        &self,
        registration: &CollectionRegistration,
        generation: GenerationId,
        specification: &GenerationSpec,
    ) -> Result<(), ServiceError> {
        let sync = self
            .store
            .invalidate_knowledge_collection(self.lease, registration, generation)
            .await?;
        tokio::time::timeout(
            Duration::from_secs(3600),
            self.populate(registration, generation, specification, &sync),
        )
        .await
        .map_err(|_| ServiceError::Deadline)?
    }

    /// Invalidation fences old chunks before any source I/O. A failed read,
    /// embedding error or cancellation leaves those chunks unavailable to search.
    pub async fn refresh(
        &self,
        registration: &CollectionRegistration,
        generation: GenerationId,
        specification: &GenerationSpec,
        link: &MemberLink,
    ) -> Result<(), ServiceError> {
        self.validate_specification(specification)?;
        if self.embeddings.space() != specification.space() {
            return Err(ServiceError::EmbeddingSpace);
        }
        let ticket = self
            .store
            .begin_knowledge_member_read(
                self.lease,
                registration,
                generation,
                specification,
                &link.uri,
            )
            .await?;
        tokio::time::timeout(Duration::from_secs(120), async {
            let title = link.title.clone().unwrap_or(MemberTitle::new(
                link.uri.as_str().chars().take(256).collect::<String>(),
            )?);
            let previous = ticket.previous().filter(|_| ticket.title() == Some(&title));
            let read = self
                .source
                .read(&registration.descriptor, link.uri.clone(), previous)
                .await?;
            let document = match read {
                crate::source::SourceRead::NotModified(observation) => {
                    let previous = previous.ok_or(KnowledgeError(
                        "source returned not-modified without a conditional request",
                    ))?;
                    observation.revalidated(previous)?;
                    registration.admit_observation(&observation)?;
                    observation.validate_collection(&registration.descriptor)?;
                    self.store
                        .revalidate_knowledge_member(&ticket, &observation)
                        .await?;
                    return Ok(());
                }
                crate::source::SourceRead::Modified(document) => document,
            };
            registration.admit_observation(document.observation())?;
            document
                .observation()
                .validate_collection(&registration.descriptor)?;
            let metadata;
            let text = match registration.descriptor.indexing() {
                IndexingMode::Content => document.text(),
                IndexingMode::Metadata => {
                    metadata = metadata_text(&title, document.observation());
                    &metadata
                }
                IndexingMode::None => {
                    return Err(KnowledgeError("collection disallows indexing").into());
                }
            };
            let ranges = chunk::ranges(text, specification.chunking())?;
            let mut chunks = Vec::new();
            let mut offset = 0;
            while offset < ranges.len() {
                let mut bytes = 0;
                let mut end = offset;
                while end < ranges.len() && end - offset < EmbeddingBatch::MAX_INPUTS {
                    let length = ranges[end].len();
                    if bytes + length > EmbeddingBatch::MAX_BYTES {
                        break;
                    }
                    bytes += length;
                    end += 1;
                }
                let batch = EmbeddingBatch::new(
                    ranges[offset..end]
                        .iter()
                        .map(|r| EmbeddingText::new(&text[r.clone()]))
                        .collect::<Result<_, _>>()?,
                )?;
                let vectors = self.embeddings.documents(batch).await?;
                if vectors.len() != end - offset {
                    return Err(
                        KnowledgeError("embedding batch returned the wrong vector count").into(),
                    );
                }
                for (range, vector) in ranges[offset..end].iter().zip(vectors) {
                    chunks.push(IndexedChunk::from_range(
                        text,
                        range.clone(),
                        vector,
                        specification,
                    )?);
                }
                offset = end;
            }
            let member = match registration.descriptor.indexing() {
                IndexingMode::Metadata => IndexedMember::metadata(
                    registration,
                    specification,
                    link.uri.clone(),
                    document.observation().clone(),
                    document.text(),
                    title,
                    chunks,
                )?,
                IndexingMode::Content => IndexedMember::new(
                    registration,
                    specification,
                    link.uri.clone(),
                    document.observation().clone(),
                    document.text(),
                    title,
                    chunks,
                )?,
                IndexingMode::None => unreachable!("checked before embedding"),
            };
            self.store
                .replace_knowledge_member(&ticket, &member)
                .await?;
            Ok(())
        })
        .await
        .map_err(|_| ServiceError::Deadline)?
    }

    fn validate_specification(&self, specification: &GenerationSpec) -> Result<(), ServiceError> {
        if specification.chunking().version() != chunk::VERSION {
            return Err(
                KnowledgeError("index generation requires the installed chunker version").into(),
            );
        }
        Ok(())
    }
}
