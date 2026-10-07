use super::*;

// Verify governed bytes through the public gateway. Importing an Artifact or
// Recording implementation here would defeat the external-client build boundary.
pub async fn assert_governed_artifact_access(
    installation: &InstalledTarget,
    artifact_id: &veoveo_artifact_contract::ArtifactId,
) -> Result<()> {
    let administrator = installation.administrator()?;
    let admin_token = administrator.token().await?;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?;
    let snapshot: Value = client
        .get(installation.public_url(&[
            "admin",
            administrator.profile.as_str(),
            "console",
            "snapshot",
        ])?)
        .bearer_auth(&admin_token)
        .send()
        .await
        .context("requesting the governed Console snapshot")?
        .error_for_status()
        .context("governed Console snapshot returned an error")?
        .json()
        .await
        .context("decoding the governed Console snapshot")?;
    let artifact = snapshot
        .get("artifacts")
        .and_then(Value::as_array)
        .and_then(|artifacts| {
            artifacts.iter().find(|artifact| {
                artifact.get("id").and_then(Value::as_str) == Some(artifact_id.to_string().as_str())
            })
        })
        .with_context(|| format!("Console snapshot omitted governed artifact {artifact_id}"))?;
    ensure!(
        artifact
            .pointer("/provenance/workContext")
            .and_then(Value::as_str)
            == Some(installation.operator.work_context.id.as_str())
            && artifact
                .pointer("/provenance/producer")
                .and_then(Value::as_str)
                == Some(installation.operator.principal.as_str())
            && artifact.pointer("/provenance/invocationMode")
                == Some(&serde_json::to_value(
                    installation.operator.invocation_mode
                )?)
            && artifact
                .pointer("/provenance/policyRevision")
                .and_then(Value::as_str)
                .is_some_and(|revision| !revision.is_empty())
            && artifact.get("outputOwner")
                == Some(&serde_json::to_value(
                    &installation.operator.work_context.output_policy.owner
                )?)
            && artifact
                .pointer("/effectiveAccess/read")
                .and_then(Value::as_bool)
                == Some(true),
        "governed artifact provenance or effective access is incomplete: {artifact}"
    );

    let download_url = installation.public_url(&[
        "artifacts",
        installation.profile(),
        &artifact_id.to_string(),
        "download",
    ])?;
    let preview_json = download_governed_json_artifact(installation, artifact_id).await?;
    ensure!(
        preview_json.is_object(),
        "authorized governed artifact preview did not contain a JSON object"
    );

    let comparison = installation
        .operator
        .comparison_context
        .as_ref()
        .context("artifact isolation requires operator.comparisonContext")?;
    let independent_token = installation.operator.token_for_context(comparison).await?;
    let no_redirect = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    let denied = no_redirect
        .get(download_url)
        .bearer_auth(independent_token)
        .send()
        .await
        .context("requesting the governed artifact from an independent Work Context")?;
    ensure!(
        denied.status() == reqwest::StatusCode::FORBIDDEN,
        "independent Work Context received {}, expected 403",
        denied.status()
    );
    Ok(())
}

pub async fn download_governed_json_artifact(
    installation: &InstalledTarget,
    artifact_id: &veoveo_artifact_contract::ArtifactId,
) -> Result<Value> {
    let token = installation.token().await?;
    let response = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?
        .get(installation.public_url(&[
            "artifacts",
            installation.profile(),
            &artifact_id.to_string(),
            "download",
        ])?)
        .bearer_auth(token)
        .send()
        .await
        .with_context(|| format!("downloading governed JSON artifact {artifact_id}"))?
        .error_for_status()
        .with_context(|| format!("governed JSON artifact {artifact_id} returned an error"))?;
    let media_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_owned();
    ensure!(
        media_type == "application/json"
            || (media_type.starts_with("application/") && media_type.ends_with("+json")),
        "governed artifact {artifact_id} returned media type `{media_type}`"
    );
    serde_json::from_slice(&response.bytes().await?)
        .with_context(|| format!("governed artifact {artifact_id} contained invalid JSON"))
}
