//! Synthetic edge admission with the production bounded relay. The real gateway
//! and BFF authentication/cookie boundary keeps its own integration qualification.
use axum::{
    Router,
    extract::{Path, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use veoveo_computers_transport::{Client, UpstreamRequest};

pub async fn start(
    upstream: String,
    stop: CancellationToken,
) -> (String, tokio::task::JoinHandle<()>) {
    let client = Client::new(reqwest::Client::builder().no_proxy()).unwrap();
    let token = stop.clone();
    let router = Router::new().route(
        "/computers/{id}/terminal",
        get(
            move |Path(id): Path<Uuid>, headers: HeaderMap, ws: WebSocketUpgrade| {
                let client = client.clone();
                let upstream = upstream.clone();
                let stop = token.clone();
                async move {
                    let (Some(authorization), Some(origin)) =
                        (headers.get("authorization"), headers.get("origin"))
                    else {
                        return StatusCode::UNAUTHORIZED.into_response();
                    };
                    let upstream = client
                        .connect(UpstreamRequest {
                            url: format!("{upstream}/computers/{id}/terminal")
                                .parse()
                                .unwrap(),
                            authorization: authorization.clone(),
                            host: None,
                            origin: origin.clone(),
                        })
                        .await;
                    let Ok(upstream) = upstream else {
                        return StatusCode::BAD_GATEWAY.into_response();
                    };
                    let response: Response = ws
                        .max_message_size(65536)
                        .max_frame_size(65536)
                        .write_buffer_size(0)
                        .max_write_buffer_size(131072)
                        .on_upgrade(move |socket| async move {
                            let _ =
                                veoveo_computers_transport::relay(socket, upstream, id, stop).await;
                        });
                    response
                }
            },
        ),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let job = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stop.cancelled_owned())
            .await
            .unwrap();
    });
    (format!("http://{address}"), job)
}
