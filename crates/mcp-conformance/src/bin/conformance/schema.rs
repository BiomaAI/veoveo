use super::*;

struct ContractSchema {
    filename: &'static str,
    schema: Value,
}

fn contract_schemas() -> Result<Vec<ContractSchema>> {
    macro_rules! add_schema {
        ($schemas:ident, $filename:literal, $ty:ty) => {{
            $schemas.push(ContractSchema {
                filename: $filename,
                schema: serde_json::to_value(schemars::schema_for!($ty))?,
            });
        }};
    }

    let mut schemas = Vec::new();
    add_schema!(
        schemas,
        "gateway-control-plane.schema.json",
        GatewayControlPlane
    );
    add_schema!(
        schemas,
        "gateway-control-plane-revision.schema.json",
        GatewayControlPlaneRevision
    );
    add_schema!(schemas, "server-manifest.schema.json", ServerManifest);
    add_schema!(schemas, "gateway-profile.schema.json", GatewayProfile);
    add_schema!(
        schemas,
        "profile-server-exposure.schema.json",
        ProfileServerExposure
    );
    add_schema!(
        schemas,
        "mcp-surface-capabilities.schema.json",
        McpSurfaceCapabilities
    );
    add_schema!(schemas, "upstream-endpoint.schema.json", UpstreamEndpoint);
    add_schema!(schemas, "secret-reference.schema.json", SecretReference);
    add_schema!(schemas, "identity-provider.schema.json", IdentityProvider);
    add_schema!(
        schemas,
        "resource-authorization-server.schema.json",
        ResourceAuthorizationServer
    );
    add_schema!(
        schemas,
        "oauth-client-registration.schema.json",
        OAuthClientRegistration
    );
    add_schema!(
        schemas,
        "identity-provider-oidc-client-registration.schema.json",
        IdentityProviderOidcClientRegistration
    );
    add_schema!(schemas, "policy-set.schema.json", PolicySet);
    add_schema!(schemas, "policy-rule.schema.json", PolicyRule);
    add_schema!(
        schemas,
        "data-label-definition.schema.json",
        DataLabelDefinition
    );
    add_schema!(schemas, "tenant-definition.schema.json", TenantDefinition);
    add_schema!(schemas, "principal.schema.json", Principal);
    add_schema!(
        schemas,
        "principal-audit-attributes.schema.json",
        PrincipalAuditAttributes
    );
    add_schema!(
        schemas,
        "access-token-subject.schema.json",
        AccessTokenSubject
    );
    add_schema!(schemas, "policy-decision.schema.json", PolicyDecision);
    add_schema!(schemas, "audit-event.schema.json", AuditEvent);
    add_schema!(schemas, "auth-audit-event.schema.json", AuthAuditEvent);
    add_schema!(
        schemas,
        "gateway-jwt-revocation-request.schema.json",
        GatewayJwtRevocationRequest
    );
    add_schema!(
        schemas,
        "gateway-jwt-revocation.schema.json",
        GatewayJwtRevocation
    );
    add_schema!(
        schemas,
        "gateway-jwt-revocation-apply-result.schema.json",
        GatewayJwtRevocationApplyResult
    );
    add_schema!(
        schemas,
        "gateway-jwt-revocation-prune-result.schema.json",
        GatewayJwtRevocationPruneResult
    );
    add_schema!(
        schemas,
        "gateway-task-mapping.schema.json",
        GatewayTaskMapping
    );
    add_schema!(
        schemas,
        "gateway-resource-subscription.schema.json",
        GatewayResourceSubscription
    );
    add_schema!(
        schemas,
        "gateway-resource-projection.schema.json",
        GatewayResourceProjection
    );
    add_schema!(
        schemas,
        "gateway-internal-identity.schema.json",
        GatewayInternalIdentity
    );
    add_schema!(
        schemas,
        "gateway-authorization-request.schema.json",
        GatewayAuthorizationRequest
    );
    add_schema!(
        schemas,
        "gateway-authorization-code-record.schema.json",
        GatewayAuthorizationCodeRecord
    );
    add_schema!(
        schemas,
        "self-hosted-deployment-plan.schema.json",
        SelfHostedDeploymentPlan
    );
    add_schema!(
        schemas,
        "self-hosted-deployment-profile.schema.json",
        SelfHostedDeploymentProfile
    );
    add_schema!(
        schemas,
        "service-to-service-security.schema.json",
        ServiceToServiceSecurity
    );
    add_schema!(
        schemas,
        "object-store-deployment.schema.json",
        ObjectStoreDeployment
    );
    add_schema!(
        schemas,
        "state-store-deployment.schema.json",
        StateStoreDeployment
    );
    add_schema!(
        schemas,
        "telemetry-sink-deployment.schema.json",
        TelemetrySinkDeployment
    );
    add_schema!(
        schemas,
        "network-boundary-rule.schema.json",
        NetworkBoundaryRule
    );
    add_schema!(
        schemas,
        "data-retention-policy.schema.json",
        DataRetentionPolicy
    );
    add_schema!(
        schemas,
        "regulated-data-controls.schema.json",
        RegulatedDataControls
    );
    add_schema!(
        schemas,
        "compliance-metadata.schema.json",
        ComplianceMetadata
    );
    add_schema!(schemas, "artifact-metadata.schema.json", ArtifactMetadata);
    add_schema!(schemas, "frame-definition.schema.json", FrameDefinition);
    add_schema!(
        schemas,
        "coordinate-position.schema.json",
        CoordinatePosition
    );
    add_schema!(
        schemas,
        "coordinate-operation-provenance.schema.json",
        CoordinateOperationProvenance
    );
    add_schema!(schemas, "geofence-geometry.schema.json", GeofenceGeometry);
    add_schema!(
        schemas,
        "convert-frame-request.schema.json",
        ConvertFrameRequest
    );
    add_schema!(
        schemas,
        "convert-frame-output.schema.json",
        ConvertFrameOutput
    );
    add_schema!(
        schemas,
        "transform-crs-request.schema.json",
        TransformCrsRequest
    );
    add_schema!(
        schemas,
        "transform-crs-output.schema.json",
        TransformCrsOutput
    );
    add_schema!(
        schemas,
        "derive-local-frame-request.schema.json",
        DeriveLocalFrameRequest
    );
    add_schema!(
        schemas,
        "derive-local-frame-output.schema.json",
        DeriveLocalFrameOutput
    );
    add_schema!(
        schemas,
        "geodesic-inverse-request.schema.json",
        GeodesicInverseRequest
    );
    add_schema!(
        schemas,
        "geodesic-inverse-output.schema.json",
        GeodesicInverseOutput
    );
    add_schema!(
        schemas,
        "geodesic-direct-request.schema.json",
        GeodesicDirectRequest
    );
    add_schema!(
        schemas,
        "geodesic-direct-output.schema.json",
        GeodesicDirectOutput
    );
    add_schema!(
        schemas,
        "validate-geofence-request.schema.json",
        ValidateGeofenceRequest
    );
    add_schema!(
        schemas,
        "validate-geofence-output.schema.json",
        ValidateGeofenceOutput
    );
    add_schema!(
        schemas,
        "batch-transform-request.schema.json",
        BatchTransformRequest
    );
    add_schema!(
        schemas,
        "batch-transform-output.schema.json",
        BatchTransformOutput
    );
    add_schema!(
        schemas,
        "generation-prediction-summary.schema.json",
        GenerationPredictionSummary
    );
    add_schema!(
        schemas,
        "generation-run-output.schema.json",
        GenerationRunOutput
    );
    add_schema!(schemas, "usage-record.schema.json", UsageRecord);
    add_schema!(schemas, "usage-report.schema.json", UsageReport);
    Ok(schemas)
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
