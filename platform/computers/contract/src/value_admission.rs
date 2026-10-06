//! Portable grant fields shared by public values and native projections.
use crate::{AutomationExecutionLimits, AutomationPermission, ComputerResultError};
use std::collections::BTreeSet;

pub fn name(value: &str) -> Result<(), ComputerResultError> {
    if value.is_empty()
        || value.len() > 64
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ComputerResultError);
    }
    Ok(())
}

pub fn grant_scope(
    name_value: &str,
    permissions: &BTreeSet<AutomationPermission>,
    limits: Option<AutomationExecutionLimits>,
) -> Result<(), ComputerResultError> {
    name(name_value)?;
    self::permissions(permissions, limits)
}

pub fn permissions(
    permissions: &BTreeSet<AutomationPermission>,
    limits: Option<AutomationExecutionLimits>,
) -> Result<(), ComputerResultError> {
    if permissions.is_empty()
        || permissions.len() > 4
        || permissions.contains(&AutomationPermission::Execute) != limits.is_some()
    {
        return Err(ComputerResultError);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::*;
    use veoveo_types::{OAuthClientId, PrincipalId};

    fn request() -> IssueAutomationGrantInputValue {
        IssueAutomationGrantInputValue {
            computer_id: ComputerId::new(),
            request_id: RequestId::new(),
            principal_id: PrincipalId::parse("https://computer.test#service").unwrap(),
            oauth_client_id: OAuthClientId::parse("client").unwrap(),
            name: "Runner".into(),
            permissions: [AutomationPermission::Read].into_iter().collect(),
            execution_limits: None,
            expires_at: chrono::Utc::now(),
        }
    }
    #[test]
    fn grant_scope_and_execution_bounds_are_checked_on_construction_and_decode() {
        let admitted = request().build().unwrap();
        assert_eq!(
            serde_json::from_value::<IssueAutomationGrantInput>(
                serde_json::to_value(&admitted).unwrap()
            )
            .unwrap(),
            admitted
        );
        for case in 0..4 {
            let mut draft = request();
            match case {
                0 => draft.name = " ".into(),
                1 => draft.permissions.clear(),
                2 => {
                    draft.permissions.insert(AutomationPermission::Execute);
                }
                _ => {
                    draft.execution_limits = Some(
                        AutomationExecutionLimitsValue {
                            maximum_seconds: 1,
                            maximum_output_bytes: 1,
                            on_interruption: AutomationInterruption::StopComputer,
                        }
                        .build()
                        .unwrap(),
                    )
                }
            }
            let bytes = serde_json::to_vec(&draft).unwrap();
            assert!(draft.build().is_err(), "case {case}");
            assert!(
                serde_json::from_slice::<IssueAutomationGrantInput>(&bytes).is_err(),
                "case {case}"
            );
        }
        for (maximum_seconds, maximum_output_bytes) in [(0, 1), (7201, 1), (1, 0), (1, 67108865)] {
            let draft = AutomationExecutionLimitsValue {
                maximum_seconds,
                maximum_output_bytes,
                on_interruption: AutomationInterruption::StopComputer,
            };
            assert!(
                serde_json::from_value::<AutomationExecutionLimits>(
                    serde_json::to_value(draft).unwrap()
                )
                .is_err()
            );
            assert!(draft.build().is_err());
        }
    }
    #[test]
    fn pairing_tokens_admit_the_same_owner_id_and_secret_profile_on_decode() {
        let grant = AccessGrantId::new();
        let value = format!("vcli1.{grant}.{}", "a".repeat(64));
        let token = CliPairingToken::new(value.clone()).unwrap();
        assert_eq!(
            serde_json::from_str::<CliPairingToken>(&serde_json::to_string(&token).unwrap())
                .unwrap()
                .expose_secret(),
            token.expose_secret()
        );
        for value in [
            value.replace("vcli1.", "vcli2."),
            format!("vcli1.{grant}.{}", "A".repeat(64)),
            "vcli1.wrong.identity".into(),
        ] {
            assert!(CliPairingToken::new(value.clone()).is_err());
            assert!(serde_json::from_value::<CliPairingToken>(serde_json::json!(value)).is_err());
        }
    }
}
