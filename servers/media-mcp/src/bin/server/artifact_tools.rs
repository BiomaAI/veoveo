use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use rmcp::{
    ErrorData as McpError,
    model::{CallToolResult, ContentBlock},
    service::{RequestContext, RoleServer},
};

use super::AppState;
use veoveo_mcp_contract::hosting::plane_caller;

const MAX_INLINE_ARTIFACT_BYTES: u64 = 3 * 1024 * 1024;

use veoveo_media_mcp::contract::{ArtifactArgs, ArtifactOutputValue};

pub(super) async fn artifact_result(
    state: &AppState,
    args: ArtifactArgs,
    context: &RequestContext<RoleServer>,
) -> Result<CallToolResult, McpError> {
    let artifact_id = args.artifact_uri.artifact_id();
    // The plane enforces access with the caller's identity.
    let caller = plane_caller(context)?;
    let artifact = state
        .artifacts
        .get(&caller, &artifact_id)
        .await
        .map_err(|err| McpError::internal_error(err.to_string(), None))?
        .ok_or_else(|| {
            McpError::resource_not_found(format!("unknown artifact '{artifact_id}'"), None)
        })?;

    if artifact.metadata.byte_len != artifact.bytes.len() as u64 {
        return Err(McpError::internal_error(
            "Artifact bytes disagree with metadata length",
            None,
        ));
    }
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
        serde_json::to_value(
            ArtifactOutputValue {
                artifact: metadata,
                inlined: can_inline,
            }
            .build()
            .map_err(|err| McpError::internal_error(err.to_string(), None))?,
        )
        .map_err(|err| McpError::internal_error(err.to_string(), None))?,
    );
    Ok(result)
}

#[cfg(test)]
mod tests {
    use rmcp::model::ContentBlock;
    use veoveo_artifact_contract::{ArtifactId, ArtifactMetadata, ArtifactReleaseState};
    use veoveo_mcp_contract::now_utc;

    use super::ArtifactOutputValue;

    #[test]
    fn artifact_output_redacts_download_url() {
        let output = ArtifactOutputValue {
            artifact: ArtifactMetadata {
                byte_len: 1,
                mime_type: Some("image/png".to_string()),
                filename: None,
                artifact_uri: veoveo_artifact_contract::ArtifactUri::presented(
                    &veoveo_media_mcp::uris::SCHEME,
                    ArtifactId::new(),
                ),
                download_url: Some("https://example.com/internal".to_string()),
                created_at: now_utc(),
                release_state: ArtifactReleaseState::Private,
                compliance: Default::default(),
                metadata: serde_json::Value::Null,
            }
            .without_download_url(),
            inlined: true,
        }
        .build()
        .unwrap();

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
