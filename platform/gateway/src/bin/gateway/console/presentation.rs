use chrono::Utc;
use veoveo_mcp_contract::{
    ConsoleInstallation, ConsoleSession, ConsoleTenant, GatewayControlPlane, PrincipalDisplayName,
    TenantId,
};
use veoveo_mcp_gateway::AuthenticatedSubject;

pub(crate) fn presentation(
    control: &GatewayControlPlane,
    subject: &AuthenticatedSubject,
    offline_mode: bool,
) -> anyhow::Result<(ConsoleInstallation, ConsoleSession)> {
    let tenant_id = subject
        .principal
        .tenant
        .clone()
        .unwrap_or(TenantId::new("installation")?);
    let tenant_name = control
        .tenants
        .iter()
        .find(|tenant| tenant.id == tenant_id)
        .and_then(|tenant| tenant.title.clone())
        .unwrap_or_else(|| tenant_id.to_string());
    let work_context_title = control
        .work_contexts
        .iter()
        .find(|context| context.id == subject.authority.work_context)
        .map_or_else(
            || subject.authority.work_context.to_string(),
            |context| context.title.clone(),
        );
    let branding = control.branding.as_ref();
    Ok((
        ConsoleInstallation {
            name: branding
                .map(|branding| branding.name.trim().to_owned())
                .unwrap_or_else(|| "Veoveo".to_owned()),
            product_label: branding
                .and_then(|branding| branding.product_label.clone())
                .unwrap_or_else(|| "Operations".to_owned()),
            logo: branding.and_then(|branding| branding.logo.clone()),
            accent_color: branding.and_then(|branding| branding.accent_color.clone()),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            offline_mode,
            generated_at: Utc::now(),
        },
        ConsoleSession {
            display_name: console_display_name(
                subject.principal_display_name.as_ref(),
                None,
                subject.principal.id.as_str(),
                subject.principal.subject.as_str(),
            ),
            principal_id: subject.principal.id.clone(),
            actor_id: subject.actor.id.clone(),
            tenant_id: tenant_id.clone(),
            tenant_name: tenant_name.clone(),
            work_context: subject.authority.work_context.clone(),
            work_context_title,
            membership: subject.authority.membership,
            invocation_mode: subject.authority.provenance.mode(),
            available_tenants: vec![ConsoleTenant {
                id: tenant_id,
                name: tenant_name,
            }],
        },
    ))
}

pub(crate) fn console_display_name(
    authenticated: Option<&PrincipalDisplayName>,
    projected: Option<&str>,
    principal_id: &str,
    principal_subject: &str,
) -> String {
    if let Some(authenticated) = authenticated {
        return authenticated.to_string();
    }
    if let Some(projected) = projected {
        let candidate = projected.trim();
        if PrincipalDisplayName::new(candidate).is_ok()
            && candidate != principal_id
            && candidate != principal_subject
            && principal_identifier_segment(principal_id) != candidate
        {
            return candidate.to_owned();
        }
    }
    compact_principal_label(principal_subject)
}

fn principal_identifier_segment(principal_id: &str) -> &str {
    principal_id
        .split(['#', '/'])
        .rfind(|segment| !segment.trim().is_empty())
        .unwrap_or(principal_id)
        .trim()
}

fn compact_principal_label(subject: &str) -> String {
    let candidate = principal_identifier_segment(subject);
    if candidate.is_empty() {
        return "User".to_owned();
    }
    let mut label = String::new();
    for (index, character) in candidate.chars().enumerate() {
        if index == 61 {
            label.push_str("...");
            break;
        }
        label.push(character);
    }
    label
}

#[cfg(test)]
mod tests {
    use veoveo_mcp_contract::PrincipalDisplayName;

    use super::console_display_name;

    #[test]
    fn console_display_name_prefers_authenticated_identity_metadata() {
        assert_eq!(
            console_display_name(
                Some(&PrincipalDisplayName::new("Mara Chen").unwrap()),
                Some("Stored Operator"),
                "https://login.example/tenant#object-id",
                "object-id"
            ),
            "Mara Chen"
        );
    }

    #[test]
    fn console_display_name_uses_human_projection_before_subject_fallback() {
        assert_eq!(
            console_display_name(
                None,
                Some("Stored Operator"),
                "https://login.example/tenant#object-id",
                "object-id"
            ),
            "Stored Operator"
        );
        assert_eq!(
            console_display_name(
                None,
                Some("https://login.example/tenant#object-id"),
                "https://login.example/tenant#object-id",
                "object-id"
            ),
            "object-id"
        );
        assert_eq!(
            console_display_name(
                None,
                Some("object-id"),
                "https://login.example/tenant#object-id",
                "https://login.example/users/mara"
            ),
            "mara"
        );
        assert_eq!(console_display_name(None, Some("  "), "   ", "   "), "User");
    }
}
