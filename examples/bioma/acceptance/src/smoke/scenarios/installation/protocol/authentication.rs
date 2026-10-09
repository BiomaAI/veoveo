//! Anonymous discovery must stop at the installed gateway's HTTP authentication gate.
use super::{Evidence, Method};
use anyhow::{Result, bail};
use rmcp::model::{
    ClientCapabilities, ClientJsonRpcMessage, ClientRequest, ListPromptsRequest,
    ListResourceTemplatesRequest, ListResourcesRequest, ListToolsRequest, PaginatedRequestParams,
    ProtocolVersion, RequestId, RequestMetaObject,
};
use std::time::Duration;

const METHODS: [Method; 4] = [
    Method::ToolsList,
    Method::ResourcesList,
    Method::ResourceTemplatesList,
    Method::PromptsList,
];

fn request(
    client: &reqwest::Client,
    endpoint: &url::Url,
    method: Method,
) -> Result<reqwest::Request> {
    let mut meta = RequestMetaObject::default();
    meta.set_protocol_version(ProtocolVersion::V_2026_07_28);
    meta.set_client_capabilities(ClientCapabilities::default());
    let mut params = PaginatedRequestParams::default();
    params.meta = Some(meta);
    let request = match method {
        Method::ToolsList => ClientRequest::ListToolsRequest(ListToolsRequest::with_param(params)),
        Method::ResourcesList => {
            ClientRequest::ListResourcesRequest(ListResourcesRequest::with_param(params))
        }
        Method::ResourceTemplatesList => ClientRequest::ListResourceTemplatesRequest(
            ListResourceTemplatesRequest::with_param(params),
        ),
        Method::PromptsList => {
            ClientRequest::ListPromptsRequest(ListPromptsRequest::with_param(params))
        }
        _ => bail!("anonymous discovery requires a catalog list method"),
    };
    let wire_method = request.method().to_owned();
    let message = ClientJsonRpcMessage::request(request, RequestId::Number(1));
    Ok(client
        .post(endpoint.clone())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header(
            reqwest::header::ACCEPT,
            "application/json, text/event-stream",
        )
        .header(
            "mcp-protocol-version",
            ProtocolVersion::V_2026_07_28.as_str(),
        )
        .header("mcp-method", wire_method)
        .body(serde_json::to_vec(&message)?)
        .build()?)
}

pub(super) async fn run(endpoint: &url::Url, evidence: &mut Evidence) -> Result<()> {
    // A separate client prevents operator/admin default headers or cookie stores
    // from turning this negative check into an authenticated request.
    let client = isolated_client()?;
    for method in METHODS {
        let request = request(&client, endpoint, method)?;
        evidence
            .http_unauthenticated(&client, request, method)
            .await?;
    }
    Ok(())
}

fn isolated_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .cookie_store(false)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(15))
        .build()?)
}

#[cfg(test)]
mod authentication_tests {
    use super::*;

    #[test]
    fn anonymous_discovery_requests_are_current_sdk_messages_without_credentials() -> Result<()> {
        let client = isolated_client()?;
        let endpoint = url::Url::parse("http://127.0.0.1:1/mcp/operator-initial")?;
        let expected = [
            "tools/list",
            "resources/list",
            "resources/templates/list",
            "prompts/list",
        ];
        for (method, expected) in METHODS.into_iter().zip(expected) {
            let request = request(&client, &endpoint, method)?;
            assert_eq!(request.url(), &endpoint);
            assert_eq!(request.method(), reqwest::Method::POST);
            let headers = request.headers();
            assert!(!headers.contains_key(reqwest::header::AUTHORIZATION));
            assert!(!headers.contains_key(reqwest::header::PROXY_AUTHORIZATION));
            assert!(!headers.contains_key(reqwest::header::COOKIE));
            assert!(!headers.contains_key("mcp-session-id"));
            assert_eq!(headers[reqwest::header::CONTENT_TYPE], "application/json");
            assert_eq!(
                headers[reqwest::header::ACCEPT],
                "application/json, text/event-stream"
            );
            assert_eq!(headers["mcp-protocol-version"], "2026-07-28");
            assert_eq!(headers["mcp-method"], expected);
            let message: ClientJsonRpcMessage = serde_json::from_slice(
                request
                    .body()
                    .and_then(reqwest::Body::as_bytes)
                    .expect("SDK request body"),
            )?;
            let ClientJsonRpcMessage::Request(message) = message else {
                bail!("expected an SDK JSON-RPC request");
            };
            assert_eq!(message.id, RequestId::Number(1));
            assert_eq!(message.request.method(), expected);
            let (params, extensions) = match message.request {
                ClientRequest::ListToolsRequest(request) => (request.params, request.extensions),
                ClientRequest::ListResourcesRequest(request) => {
                    (request.params, request.extensions)
                }
                ClientRequest::ListResourceTemplatesRequest(request) => {
                    (request.params, request.extensions)
                }
                ClientRequest::ListPromptsRequest(request) => (request.params, request.extensions),
                _ => bail!("unexpected SDK request variant"),
            };
            assert!(params.expect("paginated parameters").cursor.is_none());
            // SDK decoding lifts wire params._meta into typed request extensions.
            assert_eq!(
                extensions
                    .get::<RequestMetaObject>()
                    .expect("protocol metadata")
                    .protocol_version(),
                Some(ProtocolVersion::V_2026_07_28)
            );
        }
        assert!(request(&client, &endpoint, Method::HttpPost).is_err());
        Ok(())
    }
}
