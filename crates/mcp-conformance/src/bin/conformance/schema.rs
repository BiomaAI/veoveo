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
    add_schema!(schemas, "server-manifest.schema.json", ServerManifest);
    add_schema!(schemas, "principal.schema.json", Principal);
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
        "self-hosted-deployment-plan.schema.json",
        SelfHostedDeploymentPlan
    );
    add_schema!(
        schemas,
        "compliance-metadata.schema.json",
        ComplianceMetadata
    );
    add_schema!(schemas, "artifact-metadata.schema.json", ArtifactMetadata);
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
