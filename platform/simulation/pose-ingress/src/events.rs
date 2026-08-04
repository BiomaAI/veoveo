use std::time::Duration;

use tokio_util::sync::CancellationToken;
use url::Url;
use uuid::Uuid;

pub(crate) async fn announce_runtime_generation(
    mut url: Url,
    token: String,
    cancellation: CancellationToken,
) {
    let generation = Uuid::new_v4();
    let path = format!("{}/{}", url.path().trim_end_matches('/'), generation);
    url.set_path(&path);
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            tracing::error!(%error, "pose-ingress runtime event client initialization failed");
            return;
        }
    };
    let mut delay = Duration::from_millis(250);
    loop {
        match client.post(url.clone()).bearer_auth(&token).send().await {
            Ok(response) if response.status() == reqwest::StatusCode::NO_CONTENT => {
                tracing::info!(%generation, "pose-ingress runtime generation announced");
                return;
            }
            Ok(response) if !response.status().is_server_error() => {
                tracing::error!(
                    status = %response.status(),
                    "pose-ingress runtime generation was rejected"
                );
                return;
            }
            Ok(_) | Err(_) => {}
        }
        tokio::select! {
            _ = cancellation.cancelled() => return,
            _ = tokio::time::sleep(delay) => {},
        }
        delay = delay.saturating_mul(2).min(Duration::from_secs(30));
    }
}
