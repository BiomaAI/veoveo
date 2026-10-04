use super::*;

struct ContractSchema {
    filename: &'static str,
    schema: Value,
}

fn contract_schema<T: schemars::JsonSchema>(filename: &'static str) -> Result<ContractSchema> {
    Ok(ContractSchema {
        filename,
        schema: serde_json::to_value(schemars::schema_for!(T))?,
    })
}

fn gateway_contract_schema<T: schemars::JsonSchema>(
    filename: &'static str,
    registry: &veoveo_gateway_contract::CatalogRegistry,
) -> Result<ContractSchema> {
    Ok(ContractSchema {
        filename,
        schema: serde_json::to_value(veoveo_mcp_contract::composed_gateway_schema::<T>(registry))?,
    })
}

fn contract_schemas() -> Result<Vec<ContractSchema>> {
    let registry = veoveo_gateway_catalog::registry()?;
    Ok(vec![
        contract_schema::<veoveo_mcp_conformance::HostedServerConformanceProfile>(
            "mcp-conformance-profile.schema.json",
        )?,
        contract_schema::<veoveo_mcp_conformance::ConformanceReport>(
            "mcp-conformance-report.schema.json",
        )?,
        gateway_contract_schema::<GatewayControlPlane>(
            "gateway-control-plane.schema.json",
            &registry,
        )?,
        gateway_contract_schema::<GatewayControlPlaneRevision>(
            "gateway-control-plane-revision.schema.json",
            &registry,
        )?,
        contract_schema::<ServerManifest>("server-manifest.schema.json")?,
        contract_schema::<GatewayProfile>("gateway-profile.schema.json")?,
        contract_schema::<ProfileServerExposure>("profile-server-exposure.schema.json")?,
        contract_schema::<McpSurfaceCapabilities>("mcp-surface-capabilities.schema.json")?,
        contract_schema::<UpstreamEndpoint>("upstream-endpoint.schema.json")?,
        contract_schema::<SecretReference>("secret-reference.schema.json")?,
        contract_schema::<IdentityProvider>("identity-provider.schema.json")?,
        contract_schema::<ResourceAuthorizationServer>(
            "resource-authorization-server.schema.json",
        )?,
        contract_schema::<OAuthClientRegistration>("oauth-client-registration.schema.json")?,
        contract_schema::<IdentityProviderOidcClientRegistration>(
            "identity-provider-oidc-client-registration.schema.json",
        )?,
        gateway_contract_schema::<PolicySet>("policy-set.schema.json", &registry)?,
        gateway_contract_schema::<PolicyRule>("policy-rule.schema.json", &registry)?,
        contract_schema::<DataLabelDefinition>("data-label-definition.schema.json")?,
        contract_schema::<TenantDefinition>("tenant-definition.schema.json")?,
        contract_schema::<Principal>("principal.schema.json")?,
        contract_schema::<PrincipalAuditAttributes>("principal-audit-attributes.schema.json")?,
        contract_schema::<AccessTokenSubject>("access-token-subject.schema.json")?,
        gateway_contract_schema::<PolicyDecision>("policy-decision.schema.json", &registry)?,
        contract_schema::<veoveo_mcp_contract::audit::AuditRecord>("audit-record.schema.json")?,
        contract_schema::<veoveo_mcp_contract::audit::AuditBlock>("audit-block.schema.json")?,
        contract_schema::<GatewayJwtRevocationRequest>(
            "gateway-jwt-revocation-request.schema.json",
        )?,
        contract_schema::<GatewayJwtRevocation>("gateway-jwt-revocation.schema.json")?,
        contract_schema::<GatewayJwtRevocationApplyResult>(
            "gateway-jwt-revocation-apply-result.schema.json",
        )?,
        contract_schema::<GatewayJwtRevocationPruneResult>(
            "gateway-jwt-revocation-prune-result.schema.json",
        )?,
        contract_schema::<GatewayResourceSubscription>(
            "gateway-resource-subscription.schema.json",
        )?,
        contract_schema::<GatewayResourceProjection>("gateway-resource-projection.schema.json")?,
        contract_schema::<GatewayInternalIdentity>("gateway-internal-identity.schema.json")?,
        contract_schema::<GatewayAuthorizationRequest>(
            "gateway-authorization-request.schema.json",
        )?,
        contract_schema::<GatewayAuthorizationCodeRecord>(
            "gateway-authorization-code-record.schema.json",
        )?,
        contract_schema::<SelfHostedDeploymentPlan>("self-hosted-deployment-plan.schema.json")?,
        contract_schema::<SelfHostedDeploymentProfile>(
            "self-hosted-deployment-profile.schema.json",
        )?,
        contract_schema::<ServiceToServiceSecurity>("service-to-service-security.schema.json")?,
        contract_schema::<ObjectStoreDeployment>("object-store-deployment.schema.json")?,
        contract_schema::<PlatformStoreDeployment>("platform-store-deployment.schema.json")?,
        contract_schema::<AnalyticalRuntimeDeployment>(
            "analytical-runtime-deployment.schema.json",
        )?,
        contract_schema::<IngressDeployment>("ingress-deployment.schema.json")?,
        contract_schema::<IdentityProviderDeployment>("identity-provider-deployment.schema.json")?,
        contract_schema::<SecretManagerDeployment>("secret-manager-deployment.schema.json")?,
        contract_schema::<TelemetryDeployment>("telemetry-deployment.schema.json")?,
        contract_schema::<TenantModel>("tenant-model.schema.json")?,
        contract_schema::<DataRetentionPolicy>("data-retention-policy.schema.json")?,
        contract_schema::<ComplianceMetadata>("compliance-metadata.schema.json")?,
        contract_schema::<ArtifactMetadata>("artifact-metadata.schema.json")?,
        contract_schema::<CoordinateOperationProvenance>(
            "coordinate-operation-provenance.schema.json",
        )?,
        contract_schema::<GenerationPredictionSummary>(
            "generation-prediction-summary.schema.json",
        )?,
        contract_schema::<MediaGenerationResult>("media-generation-result.schema.json")?,
        contract_schema::<UsageRecord>("usage-record.schema.json")?,
        contract_schema::<UsageReport>("usage-report.schema.json")?,
    ])
}

pub(super) fn cmd_contract_schemas(output_dir: PathBuf) -> Result<()> {
    let schemas = contract_schemas()?;
    std::fs::create_dir_all(&output_dir)?;
    for contract_schema in &schemas {
        let path = output_dir.join(contract_schema.filename);
        let bytes = serde_json::to_vec_pretty(&contract_schema.schema)?;
        std::fs::write(&path, bytes)?;
    }
    println!(
        "wrote {} contract schema(s) to {}",
        schemas.len(),
        output_dir.display()
    );
    Ok(())
}
