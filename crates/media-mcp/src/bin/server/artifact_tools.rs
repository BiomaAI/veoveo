use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock},
    schemars,
    service::{RequestContext, RoleServer},
};
use veoveo_mcp_contract::ArtifactMetadata;

use super::{
    AppState,
    ownership::{artifact_owned_by, internal_identity},
};
use veoveo_media_mcp::uris;

const MAX_INLINE_ARTIFACT_BYTES: u64 = 3 * 1024 * 1024;

#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct ArtifactArgs {
    /// Canonical artifact resource URI, e.g. media://artifact/{sha256}.
    pub(super) artifact_uri: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub(super) struct ArtifactOutput {
    pub(super) artifact: ArtifactMetadata,
    pub(super) inlined: bool,
}

pub(super) async fn artifact_result(
    state: &AppState,
    args: ArtifactArgs,
    context: &RequestContext<RoleServer>,
) -> Result<CallToolResult, McpError> {
    let sha256 = uris::parse_artifact_uri(&args.artifact_uri).ok_or_else(|| {
        McpError::invalid_params("artifact_uri must be media://artifact/{sha256}", None)
    })?;
    let artifact = state
        .artifacts
        .get(sha256)
        .await
        .map_err(|err| McpError::internal_error(err.to_string(), None))?
        .ok_or_else(|| {
            McpError::resource_not_found(format!("unknown artifact '{sha256}'"), None)
        })?;
    let identity = internal_identity(context)?;
    artifact_owned_by(state, &artifact.metadata.sha256, &identity)?;

    let metadata = artifact.metadata.without_download_url();
    let mime = metadata
        .mime_type
        .as_deref()
        .unwrap_or("application/octet-stream");
    let can_inline =
        mime.starts_with("image/") && artifact.bytes.len() as u64 <= MAX_INLINE_ARTIFACT_BYTES;
    let mut blocks = vec![ContentBlock::text(format!(
        "Artifact {} ({mime}, {} byte(s)).",
        metadata.artifact_uri, metadata.byte_len
    ))];
    if can_inline {
        blocks.push(ContentBlock::image(
            BASE64_STANDARD.encode(&artifact.bytes),
            mime.to_string(),
        ));
    } else {
        blocks.push(ContentBlock::text(
            "Artifact bytes were not inlined. Use resources/read with the artifact URI from structuredContent.",
        ));
    }
    let mut result = CallToolResult::success(blocks);
    result.structured_content = Some(
        serde_json::to_value(ArtifactOutput {
            artifact: metadata,
            inlined: can_inline,
        })
        .map_err(|err| McpError::internal_error(err.to_string(), None))?,
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use rmcp::model::ContentBlock;
    use veoveo_mcp_contract::{ArtifactMetadata, now_utc};

    use super::ArtifactOutput;

    #[test]
    fn artifact_output_redacts_download_url() {
        let output = ArtifactOutput {
            artifact: ArtifactMetadata {
                sha256: "a".repeat(64),
                byte_len: 1,
                mime_type: Some("image/png".to_string()),
                filename: None,
                artifact_uri: format!("media://artifact/{}", "a".repeat(64)),
                download_url: Some("https://example.com/internal".to_string()),
                created_at: now_utc(),
                compliance: Default::default(),
                metadata: serde_json::Value::Null,
            }
            .without_download_url(),
            inlined: true,
        };

        let value = serde_json::to_value(output).unwrap();
        assert!(value["artifact"].get("download_url").is_none());
    }

    #[test]
    fn rmcp_image_content_serializes_as_image() {
        let block = ContentBlock::image("abcd", "image/png");
        let value = serde_json::to_value(block).unwrap();
        assert_eq!(value["type"], "image");
        assert_eq!(value["mimeType"], "image/png");
        assert_eq!(value["data"], "abcd");
    }
}
