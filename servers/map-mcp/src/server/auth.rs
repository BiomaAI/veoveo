//! Typed Map scope admission shared by requests and tasks.

pub(crate) fn require_scope(
    grants: &std::collections::BTreeSet<veoveo_types::ScopeName>,
    required: crate::contract::MapScope,
) -> Result<(), rmcp::ErrorData> {
    use veoveo_types::ScopeDefinition;
    grants
        .contains(required.name())
        .then_some(())
        .ok_or_else(|| {
            rmcp::ErrorData::invalid_request(
                format!(
                    "You don't have permission to make this request. Missing scope `{required}`."
                ),
                None,
            )
        })
}

#[cfg(test)]
mod tests {
    use super::require_scope;
    use crate::contract::MapScope;
    use std::collections::BTreeSet;
    use veoveo_types::ScopeName;

    #[test]
    fn typed_admission_preserves_other_domains_and_revocation() {
        let mut grants = BTreeSet::from([
            ScopeName::parse("another-server:custom").unwrap(),
            MapScope::FeatureRead.into(),
        ]);
        assert!(require_scope(&grants, MapScope::FeatureRead).is_ok());
        assert!(require_scope(&grants, MapScope::FeatureWrite).is_err());
        grants.remove(&MapScope::FeatureRead.into());
        assert!(require_scope(&grants, MapScope::FeatureRead).is_err());
        grants.insert(MapScope::Admin.into());
        assert!(require_scope(&grants, MapScope::Admin).is_ok());
        assert!(require_scope(&grants, MapScope::FeatureRead).is_err());
    }
}
