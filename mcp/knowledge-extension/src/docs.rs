//! Protocol-independent docs collection, addresses, build digests and paging.
use crate::{
    AccessModel, ChangeSignal, CollectionDescriptor, CollectionId, CollectionName, DocumentId,
    EntityKind, Freshness, IndexingMode, KnowledgeError, Observation, Revision,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use veoveo_types::{
    ResourceScheme, ResourceTemplateUri, ResourceUri, ResourceUriBuilder, ServerSlug, Sha256Digest,
    UriAuthority, UriSegment,
};

pub const DOC_PAGE_SIZE: usize = 32;

pub fn index_uri(scheme: &ResourceScheme) -> ResourceUri {
    ResourceUriBuilder::from_components(
        scheme,
        UriAuthority::new("docs").expect("declared authority"),
    )
    .expect("resource scheme")
    .build()
    .expect("docs URI")
}
pub fn member_uri(scheme: &ResourceScheme, id: &DocumentId) -> ResourceUri {
    ResourceUriBuilder::new(index_uri(scheme).as_str())
        .expect("docs URI")
        .segment(UriSegment::new(id.as_str()).expect("validated document id"))
        .build()
        .expect("docs member URI")
}
pub fn member_template(scheme: &ResourceScheme) -> ResourceTemplateUri {
    // The variable is a fixed route declaration; concrete IDs always use the URI builder.
    ResourceTemplateUri::new(format!("{}/{{doc_id}}", index_uri(scheme)))
        .expect("declared docs template")
}
pub fn collection(server: &ServerSlug, scheme: &ResourceScheme) -> CollectionDescriptor {
    CollectionDescriptor::new(
        CollectionId::new(
            server.clone(),
            CollectionName::new("docs").expect("declared name"),
        )
        .expect("server slug"),
        EntityKind::new("document").expect("declared kind"),
        ResourceTemplateUri::new(index_uri(scheme).to_string()).expect("docs URI"),
        Freshness::immutable(),
        ChangeSignal::Immutable,
        AccessModel::Profile,
        IndexingMode::Content,
    )
    .expect("docs descriptor")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentEntry {
    pub id: DocumentId,
    pub title: String,
    pub uri: ResourceUri,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentPage {
    pub items: Vec<DocumentEntry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<DocumentId>,
}

/// The image is immutable. A cursor is the last admitted document ID; sorted
/// keyset paging never returns it again and rejects IDs absent in this image.
pub fn page(
    mut entries: Vec<DocumentEntry>,
    after: Option<&DocumentId>,
) -> Result<DocumentPage, KnowledgeError> {
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    if entries.windows(2).any(|w| w[0].id == w[1].id) {
        return Err(KnowledgeError("duplicate document id"));
    }
    let start = match after {
        Some(id) => {
            entries
                .binary_search_by(|v| v.id.cmp(id))
                .map_err(|_| KnowledgeError("unknown document cursor"))?
                + 1
        }
        None => 0,
    };
    let end = (start + DOC_PAGE_SIZE).min(entries.len());
    let next_cursor = (end < entries.len()).then(|| entries[end - 1].id.clone());
    Ok(DocumentPage {
        items: entries.drain(start..end).collect(),
        next_cursor,
    })
}

pub fn observation(
    collection: &CollectionDescriptor,
    digest: Sha256Digest,
    observed_at: DateTime<Utc>,
) -> Observation {
    Observation::builder(
        collection.collection().clone(),
        Revision::new(digest.as_str()).expect("digest revision"),
        digest,
        observed_at,
    )
    .build(collection)
    .expect("profile docs observation")
}
