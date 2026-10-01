use super::*;
use crate::PlatformStore;
use std::collections::{BTreeMap, BTreeSet};
use veoveo_knowledge_contract::KnowledgeCollectionApproval;
use veoveo_types::{ScopeName, ServerSlug};

#[derive(Clone, Copy)]
pub enum CatalogCompletion {
    Source,
    Collection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogCompletionValue {
    Source(ServerSlug),
    Collection(CollectionId),
}
impl std::fmt::Display for CatalogCompletionValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(value) => value.fmt(f),
            Self::Collection(value) => value.fmt(f),
        }
    }
}

impl PlatformStore {
    pub async fn complete_knowledge_catalog(
        &self,
        tenant: &TenantId,
        approvals: &BTreeMap<CollectionId, KnowledgeCollectionApproval>,
        scopes: &BTreeSet<ScopeName>,
        domain: CatalogCompletion,
        prefix: &str,
    ) -> Result<Vec<CatalogCompletionValue>, StoreError> {
        super::catalog::approvals_valid(approvals, scopes)?;
        if prefix.len() > 512 || prefix.chars().any(char::is_control) {
            return Err(StoreError::Knowledge("invalid completion prefix"));
        }
        let field = match domain {
            CatalogCompletion::Source => "string::split(collection, '.')[0]",
            CatalogCompletion::Collection => "collection",
        };
        let sql = include_str!("completion.surql").replace("__FIELD__", field);
        let mut response = self
            .client()
            .query(sql)
            .bind(("tenant", tenant.to_string()))
            .bind((
                "approvals",
                approvals
                    .iter()
                    .map(|(id, a)| (id.to_string(), Document(a.clone())))
                    .collect::<Vec<_>>(),
            ))
            .bind((
                "scopes",
                scopes.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .bind(("prefix", prefix.to_owned()))
            .await?
            .knowledge_check()?;
        let values: Vec<String> = response.take(0)?;
        values
            .into_iter()
            .map(|value| match domain {
                CatalogCompletion::Source => value
                    .parse()
                    .map(CatalogCompletionValue::Source)
                    .map_err(|_| StoreError::Knowledge("invalid stored source identity")),
                CatalogCompletion::Collection => value
                    .parse()
                    .map(CatalogCompletionValue::Collection)
                    .map_err(|_| StoreError::Knowledge("invalid stored collection identity")),
            })
            .collect()
    }
}
