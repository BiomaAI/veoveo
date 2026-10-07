//! Knowledge's indexed native approval profile is independent of public JSON.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use veoveo_knowledge_contract::{
    CollectionApproval, KnowledgeCollectionApproval, KnowledgeSubject,
};
use veoveo_mcp_knowledge_extension::CollectionId;
use veoveo_types::{DataLabelId, GroupId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, veoveo_types::Vocabulary)]
enum NativeApprovalMode {
    #[vocabulary(rename = "catalog-only")]
    CatalogOnly,
    #[vocabulary(rename = "index")]
    Index,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeApproval {
    collection: CollectionId,
    mode: NativeApprovalMode,
    stewards: BTreeSet<GroupId>,
    authoritative_for: BTreeSet<KnowledgeSubject>,
    data_labels: BTreeSet<DataLabelId>,
}
impl From<&KnowledgeCollectionApproval> for NativeApproval {
    fn from(value: &KnowledgeCollectionApproval) -> Self {
        Self {
            collection: value.collection.clone(),
            mode: match value.mode {
                CollectionApproval::CatalogOnly => NativeApprovalMode::CatalogOnly,
                CollectionApproval::Index => NativeApprovalMode::Index,
            },
            stewards: value.stewards.clone(),
            authoritative_for: value.authoritative_for.clone(),
            data_labels: value.data_labels.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_approval_bytes_preserve_database_profile_beside_current_public_wire() {
        let approval = KnowledgeCollectionApproval {
            collection: "fixture.records".parse().unwrap(),
            mode: CollectionApproval::CatalogOnly,
            stewards: ["stewards".parse().unwrap()].into(),
            authoritative_for: [KnowledgeSubject::new("facilities").unwrap()].into(),
            data_labels: ["restricted".parse().unwrap()].into(),
        };
        approval.validate().unwrap();
        let native = NativeApproval::from(&approval);
        let bytes = serde_json::to_vec(&native).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "collection":"fixture.records", "mode":"catalog-only", "stewards":["stewards"],
                "authoritative_for":["facilities"], "data_labels":["restricted"]
            })
        );
        assert_eq!(
            serde_json::from_slice::<NativeApproval>(&bytes).unwrap(),
            native
        );
        assert!(serde_json::from_slice::<KnowledgeCollectionApproval>(&bytes).is_err());
        let current = serde_json::to_value(&approval).unwrap();
        assert_eq!(current["mode"], "catalog_only");
        assert_eq!(
            current["authoritativeFor"],
            serde_json::json!(["facilities"])
        );
        assert_eq!(current["dataLabels"], serde_json::json!(["restricted"]));
        assert!(serde_json::from_value::<NativeApproval>(current).is_err());
    }
}
