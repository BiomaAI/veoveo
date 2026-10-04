use std::collections::{BTreeMap, BTreeSet};

use veoveo_mcp_contract::{Principal, PrincipalAssurance, PrincipalKind};

pub fn principal_audit_metadata(principal: &Principal) -> BTreeMap<String, String> {
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "principal_kind".to_string(),
        principal_kind_value(principal.kind).to_string(),
    );
    insert_joined(&mut metadata, "principal_groups", &principal.groups);
    insert_joined(&mut metadata, "principal_roles", &principal.roles);
    insert_joined(&mut metadata, "principal_scopes", &principal.scopes);
    insert_joined(
        &mut metadata,
        "principal_data_labels",
        &principal.data_labels,
    );
    if !principal.assurances.is_empty() {
        metadata.insert(
            "principal_assurances".to_string(),
            principal
                .assurances
                .iter()
                .map(|assurance| match assurance {
                    PrincipalAssurance::UsPerson => "us_person",
                })
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    metadata
}

pub fn merge_principal_audit_metadata(
    mut metadata: BTreeMap<String, String>,
    principal: &Principal,
) -> BTreeMap<String, String> {
    metadata.extend(principal_audit_metadata(principal));
    metadata
}

fn principal_kind_value(kind: PrincipalKind) -> &'static str {
    match kind {
        PrincipalKind::User => "user",
        PrincipalKind::Service => "service",
    }
}

fn insert_joined<T: ToString>(
    metadata: &mut BTreeMap<String, String>,
    key: &str,
    values: &BTreeSet<T>,
) {
    if !values.is_empty() {
        metadata.insert(
            key.to_string(),
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(","),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use chrono::Utc;
    use veoveo_mcp_contract::{
        Principal, PrincipalAssurance, PrincipalKind, TokenIssuer, TokenSubject,
    };
    use veoveo_types::{DataLabelId, GroupId, PrincipalId, RoleId, ScopeName};

    use super::principal_audit_metadata;

    #[test]
    fn principal_audit_metadata_projects_authz_attributes() {
        let principal = Principal {
            id: PrincipalId::parse("issuer#subject").unwrap(),
            kind: PrincipalKind::User,
            issuer: TokenIssuer::parse("issuer").unwrap(),
            subject: TokenSubject::parse("subject").unwrap(),
            tenant: None,
            groups: BTreeSet::from([GroupId::parse("engineering").unwrap()]),
            group_roles: BTreeSet::new(),
            roles: BTreeSet::from([RoleId::parse("operator").unwrap()]),
            scopes: BTreeSet::from([ScopeName::parse("operator:use").unwrap()]),
            data_labels: BTreeSet::from([DataLabelId::parse("cui").unwrap()]),
            assurances: BTreeSet::from([PrincipalAssurance::UsPerson]),
            authenticated_at: Some(Utc::now()),
        };

        let metadata = principal_audit_metadata(&principal);

        assert_eq!(metadata.get("principal_kind").unwrap(), "user");
        assert_eq!(metadata.get("principal_groups").unwrap(), "engineering");
        assert_eq!(metadata.get("principal_roles").unwrap(), "operator");
        assert_eq!(metadata.get("principal_scopes").unwrap(), "operator:use");
        assert_eq!(metadata.get("principal_data_labels").unwrap(), "cui");
        assert_eq!(metadata.get("principal_assurances").unwrap(), "us_person");
    }
}
