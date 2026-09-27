use super::{
    FramesMcp, SERVER_DOCS,
    ownership::{frame_scope_from_identity, internal_identity},
};
use rmcp::{
    ErrorData as McpError, RoleServer,
    model::{CompleteRequestParams, CompleteResult, CompletionInfo, Reference},
    service::RequestContext,
};
use veoveo_frames_mcp::{
    contract::{FrameWorldId, FrameWorldRevisionId, FrameWorldRevisionUri},
    uris,
};

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
                    FrameWorldId::new(id.clone())
                        .map(Selection::Revisions)
                        .map_err(invalid)
                })
                .transpose()
        }
        (uris::WORLD_FRAME_TEMPLATE, "frame_id") => {
            let world = parent("world_id")
                .map(|value| FrameWorldId::new(value.clone()))
                .transpose()
                .map_err(invalid)?;
            let revision = parent("revision_id")
                .map(|value| FrameWorldRevisionId::new(value.clone()))
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

fn completion(mut values: Vec<String>) -> Result<CompleteResult, McpError> {
    let has_more = values.len() > CompletionInfo::MAX_VALUES;
    let total = (!has_more).then_some(values.len() as u32);
    values.truncate(CompletionInfo::MAX_VALUES);
    Ok(CompleteResult::new(
        CompletionInfo::with_pagination(values, total, has_more).map_err(internal)?,
    ))
}

impl FramesMcp {
    pub(super) async fn complete_frames(
        &self,
        request: CompleteRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CompleteResult, McpError> {
        let identity = internal_identity(&context)?;
        let Reference::Resource(reference) = &request.r#ref else {
            return Ok(CompleteResult::default());
        };
        let needle = &request.argument.value;
        if needle.len() > 512 || needle.chars().any(char::is_control) {
            return Err(invalid(
                "completion search text must be at most 512 bytes without control characters",
            ));
        }
        if reference.uri == uris::DOC_TEMPLATE && request.argument.name == "doc_id" {
            let needle = needle.to_lowercase();
            return completion(
                SERVER_DOCS
                    .iter()
                    .map(|doc| doc.id.to_owned())
                    .filter(|id| id.contains(&needle))
                    .collect(),
            );
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
    fn completion_reports_only_known_totals() {
        let bounded = completion((0..101).map(|i| i.to_string()).collect())
            .unwrap()
            .completion;
        assert_eq!(bounded.values.len(), 100);
        assert_eq!(bounded.total, None);
        assert_eq!(bounded.has_more, Some(true));
        let exact = completion(vec!["world".into()]).unwrap().completion;
        assert_eq!(exact.total, Some(1));
        assert_eq!(exact.has_more, Some(false));
    }

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
                &FrameWorldId::new("world").unwrap(),
                &FrameWorldRevisionId::new("revision").unwrap()
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
