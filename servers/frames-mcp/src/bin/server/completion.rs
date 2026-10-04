use super::{FramesMcp, ownership::frame_scope_from_identity};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{CompleteRequestParams, CompleteResult, Reference},
    service::RequestContext,
};
use veoveo_frames_mcp::{
    contract::{FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri},
    uris,
};
use veoveo_mcp_contract::hosting::{completion, gateway_identity};

#[derive(Debug, PartialEq, Eq)]
enum Selection {
    Worlds,
    Revisions(FrameWorldId),
    Frames(FrameWorldRevisionUri),
}

fn invalid(error: impl std::fmt::Display) -> McpError {
    McpError::invalid_params(error.to_string(), None)
}
fn internal(error: impl std::fmt::Display) -> McpError {
    McpError::internal_error(error.to_string(), None)
}

fn selection(
    template: &str,
    request: &CompleteRequestParams,
) -> Result<Option<Selection>, McpError> {
    let parent = |name: &str| {
        request
            .context
            .as_ref()
            .and_then(|ctx| ctx.get_argument(name))
    };
    match (template, request.argument.name.as_str()) {
        (
            uris::WORLD_TEMPLATE | uris::WORLD_REVISION_TEMPLATE | uris::WORLD_FRAME_TEMPLATE,
            "world_id",
        ) => Ok(Some(Selection::Worlds)),
        (uris::WORLD_REVISION_TEMPLATE | uris::WORLD_FRAME_TEMPLATE, "revision_id") => {
            parent("world_id")
                .map(|id| {
                    FrameWorldId::parse(id.clone())
                        .map(Selection::Revisions)
                        .map_err(invalid)
                })
                .transpose()
        }
        (uris::WORLD_FRAME_TEMPLATE, "frame_id") => {
            let world = parent("world_id")
                .map(|value| FrameWorldId::parse(value.clone()))
                .transpose()
                .map_err(invalid)?;
            let revision = parent("revision_id")
                .map(|value| FrameWorldRevisionId::parse(value.clone()))
                .transpose()
                .map_err(invalid)?;
            let (Some(world), Some(revision)) = (world, revision) else {
                return Ok(None);
            };
            Ok(Some(Selection::Frames(FrameWorldRevisionUri::new(
                &world, &revision,
            ))))
        }
        _ => Ok(None),
    }
}

impl FramesMcp {
    pub(super) async fn complete_frames(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let identity = gateway_identity(&context)?;
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let needle = &request.argument.value;
        if needle.len() > 512 || needle.chars().any(char::is_control) {
            return Err(invalid(
                "completion search text must be at most 512 bytes without control characters",
            ));
        }
        let Some(selection) = selection(&reference.uri, &request)? else {
            return completion(vec![]);
        };
        let scope = frame_scope_from_identity(&self.state, &identity).await?;
        let values = match selection {
            Selection::Worlds => self
                .state
                .frames
                .complete_worlds(&scope, needle)
                .await
                .map_err(internal)?
                .into_iter()
                .map(String::from)
                .collect(),
            Selection::Revisions(world) => self
                .state
                .frames
                .complete_revisions(&scope, &world, needle)
                .await
                .map_err(internal)?
                .into_iter()
                .map(String::from)
                .collect(),
            Selection::Frames(revision) => self
                .state
                .frames
                .complete_frames(&scope, &revision, needle)
                .await
                .map_err(internal)?
                .into_iter()
                .map(String::from)
                .collect(),
        };
        completion(values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parent_completion_requires_typed_world_and_revision_arguments() {
        use rmcp::model::{ArgumentInfo, CompletionContext};
        use std::collections::HashMap;
        let request = CompleteRequestParams::new(
            Reference::for_resource(uris::WORLD_FRAME_TEMPLATE),
            ArgumentInfo::new("frame_id", ""),
        );
        assert_eq!(
            selection(uris::WORLD_FRAME_TEMPLATE, &request).unwrap(),
            None
        );
        let request = request.with_context(CompletionContext::with_arguments(HashMap::from([(
            "world_id".into(),
            "world".into(),
        )])));
        assert_eq!(
            selection(uris::WORLD_FRAME_TEMPLATE, &request).unwrap(),
            None
        );
        let request = request.with_context(CompletionContext::with_arguments(HashMap::from([
            ("world_id".into(), "world".into()),
            ("revision_id".into(), "revision".into()),
        ])));
        assert_eq!(
            selection(uris::WORLD_FRAME_TEMPLATE, &request).unwrap(),
            Some(Selection::Frames(FrameWorldRevisionUri::new(
                &FrameWorldId::parse("world").unwrap(),
                &FrameWorldRevisionId::parse("revision").unwrap()
            )))
        );
        let request = request.with_context(CompletionContext::with_arguments(HashMap::from([
            ("world_id".into(), "world".into()),
            ("revision_id".into(), "' OR true --".into()),
        ])));
        assert!(selection(uris::WORLD_FRAME_TEMPLATE, &request).is_err());
        let request = request.with_context(CompletionContext::with_arguments(HashMap::from([(
            "world_id".into(),
            "../world".into(),
        )])));
        assert!(selection(uris::WORLD_FRAME_TEMPLATE, &request).is_err());
    }
}
