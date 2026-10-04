use axum::http::StatusCode;
use rmcp::model::{
    ElicitRequestParams, ElicitResult, ElicitationAction, InputRequest, InputRequests,
    InputResponses,
};
use sha2::{Digest, Sha256};
use veoveo_mcp_contract::workspace as wire;

fn digest(request: &InputRequest) -> Result<String, StatusCode> {
    Ok(hex::encode(Sha256::digest(
        serde_json::to_vec(request).map_err(|_| StatusCode::BAD_GATEWAY)?,
    )))
}

/// Apps answer the native elicitation envelope; validation still uses the exact
/// outstanding server request and the ordinary Workspace input admission path.
pub(super) fn native_answers(
    requests: &InputRequests,
    responses: InputResponses,
) -> Result<Vec<wire::InputAnswer>, StatusCode> {
    if responses.len() > 32 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    responses
        .into_iter()
        .map(|(id, value)| {
            let request = requests.get(&id).ok_or(StatusCode::CONFLICT)?;
            let response: ElicitResult =
                serde_json::from_value(value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)?;
            Ok(wire::InputAnswer {
                digest: digest(request)?,
                id,
                decision: match response.action {
                    ElicitationAction::Accept => wire::InputDecision::Accept,
                    ElicitationAction::Decline => wire::InputDecision::Decline,
                    ElicitationAction::Cancel => wire::InputDecision::Cancel,
                    _ => return Err(StatusCode::NOT_IMPLEMENTED),
                },
                content: response
                    .content
                    .map(|value| {
                        serde_json::from_value(value).map_err(|_| StatusCode::UNPROCESSABLE_ENTITY)
                    })
                    .transpose()?,
            })
        })
        .collect()
}

pub(super) fn project(requests: &InputRequests) -> Result<Vec<wire::OperationInput>, StatusCode> {
    if requests.len() > 32 {
        return Err(StatusCode::BAD_GATEWAY);
    }
    requests
        .iter()
        .map(|(id, request)| {
            let mut input = wire::OperationInput {
                id: id.clone(),
                digest: digest(request)?,
                kind: wire::InputKind::Unsupported,
                message: "This operation requests input this client cannot provide.".into(),
                schema: None,
                url: None,
            };
            if let InputRequest::Elicitation(request) = request {
                match &request.params {
                    ElicitRequestParams::FormElicitationParams {
                        message,
                        requested_schema,
                        ..
                    } => {
                        input.kind = wire::InputKind::Form;
                        input.message = message.clone();
                        input.schema = Some(
                            serde_json::to_value(requested_schema)
                                .map_err(|_| StatusCode::BAD_GATEWAY)?,
                        );
                    }
                    ElicitRequestParams::UrlElicitationParams { message, url, .. }
                        if safe_url(url) =>
                    {
                        input.kind = wire::InputKind::Link;
                        input.message = message.clone();
                        input.url = Some(url.clone());
                    }
                    _ => {}
                }
            }
            Ok(input)
        })
        .collect()
}

/// The request key and exact schema/message are re-read from native MCP before
/// accepting a response. Client-supplied continuation state is never accepted.
pub(super) fn answers(
    requests: &InputRequests,
    submitted: Vec<wire::InputAnswer>,
) -> Result<InputResponses, StatusCode> {
    if submitted.is_empty() || submitted.len() > 32 {
        return Err(StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut result = InputResponses::new();
    for answer in submitted {
        let request = requests.get(&answer.id).ok_or(StatusCode::CONFLICT)?;
        if digest(request)? != answer.digest || result.contains_key(&answer.id) {
            return Err(StatusCode::CONFLICT);
        }
        let InputRequest::Elicitation(request) = request else {
            return Err(StatusCode::NOT_IMPLEMENTED);
        };
        let action = match answer.decision {
            wire::InputDecision::Accept => ElicitationAction::Accept,
            wire::InputDecision::Decline => ElicitationAction::Decline,
            wire::InputDecision::Cancel => ElicitationAction::Cancel,
        };
        let content = answer
            .content
            .map(|content| serde_json::Value::Object(content.into_iter().collect()));
        if action == ElicitationAction::Accept {
            match &request.params {
                ElicitRequestParams::FormElicitationParams {
                    requested_schema, ..
                } => {
                    let schema = serde_json::to_value(requested_schema)
                        .map_err(|_| StatusCode::BAD_GATEWAY)?;
                    let validator =
                        jsonschema::validator_for(&schema).map_err(|_| StatusCode::BAD_GATEWAY)?;
                    let value = content.as_ref().ok_or(StatusCode::UNPROCESSABLE_ENTITY)?;
                    if !validator.is_valid(value)
                        || value.as_object().is_none_or(|fields| {
                            fields
                                .keys()
                                .any(|key| !requested_schema.properties.contains_key(key))
                        })
                    {
                        return Err(StatusCode::UNPROCESSABLE_ENTITY);
                    }
                }
                ElicitRequestParams::UrlElicitationParams { url, .. }
                    if safe_url(url) && content.is_none() => {}
                _ => return Err(StatusCode::UNPROCESSABLE_ENTITY),
            }
        } else if content.is_some() {
            return Err(StatusCode::UNPROCESSABLE_ENTITY);
        }
        let mut response = ElicitResult::new(action);
        response.content = content;
        result.insert(
            answer.id,
            serde_json::to_value(response).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        );
    }
    Ok(result)
}

fn safe_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
    })
}
