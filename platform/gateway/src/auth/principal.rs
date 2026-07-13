use std::collections::BTreeSet;

use veoveo_mcp_contract::PrincipalAssurance;

use super::{claims::StringListClaim, support::AuthError};

pub(super) fn principal_assurances(
    claim: Option<StringListClaim>,
) -> Result<BTreeSet<PrincipalAssurance>, AuthError> {
    claim
        .map(StringListClaim::into_values)
        .unwrap_or_default()
        .into_iter()
        .map(|value| match value.as_str() {
            "us_person" => Ok(PrincipalAssurance::UsPerson),
            _ => Err(AuthError::InvalidPrincipalAssurance(value)),
        })
        .collect()
}
