use std::{net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Mutex, Semaphore, watch},
};
use tokio_util::sync::CancellationToken;
use url::Url;
use veoveo_mcp_contract::{LiveViewId, SubscriptionHub};

use super::live_view::{LeaseSignal, LiveViewError, LiveViewService, SignalingAdmission};
use crate::uris;

const TOKEN_PROTOCOL_PREFIX: &str = "authorization.bearer.";
const SESSION_PROTOCOL_PREFIX: &str = "x-nv-sessionid.";
const LIVE_VIEW_QUERY: &str = "live_view_id";
const MAX_SIGNALING_HEADER_BYTES: usize = 64 * 1024;
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const WEBSOCKET_GOING_AWAY: u16 = 1001;

#[derive(Clone)]
pub(super) struct SignalingGate {
    service: Arc<LiveViewService>,
    subscriptions: Arc<SubscriptionHub>,
    upstream_host: String,
    upstream_port_base: u16,
    upstream_path: String,
    public_authority: String,
    public_path: String,
    public_media_port_base: u16,
    slots: Arc<Vec<Mutex<()>>>,
    connections: Arc<Semaphore>,
}

struct Admission {
    session_id: veoveo_mcp_contract::LiveSessionId,
    live_view_id: LiveViewId,
    session_protocol: String,
    upstream_target: String,
    upstream_port: u16,
    events: watch::Receiver<LeaseSignal>,
    kind: SignalingAdmission,
    slot: usize,
}

struct UpgradeRequest {
    headers: Vec<(String, String)>,
    session_protocol: String,
    token: String,
    live_view_id: LiveViewId,
    admission: SignalingAdmission,
    suffix: String,
    query: Vec<(String, String)>,
}

impl SignalingGate {
    pub(super) fn new(
        service: Arc<LiveViewService>,
        subscriptions: Arc<SubscriptionHub>,
        public: &str,
        upstream: &str,
        public_media_port_base: u16,
        viewer_slots: u32,
    ) -> anyhow::Result<Self> {
        let public = Url::parse(public)?;
        anyhow::ensure!(
            matches!(public.scheme(), "ws" | "wss")
                && public.host_str().is_some()
                && public.username().is_empty()
                && public.password().is_none()
                && public.query().is_none()
                && public.fragment().is_none(),
            "public signaling URL must be a credential-free ws or wss URL"
        );
        let public_authority = public
            .authority()
            .trim_end_matches(':')
            .to_ascii_lowercase();
        let public_path = public.path().trim_end_matches('/').to_owned();
        anyhow::ensure!(
            !public_path.is_empty() && !public_path.split('/').any(|part| part == ".."),
            "public signaling URL path is invalid"
        );

        let upstream = Url::parse(upstream)?;
        anyhow::ensure!(
            upstream.scheme() == "ws"
                && upstream.host_str().is_some()
                && upstream.username().is_empty()
                && upstream.password().is_none()
                && upstream.port().is_some()
                && upstream.query().is_none()
                && upstream.fragment().is_none(),
            "simulator signaling URL must be a credential-free internal ws URL with an explicit base port"
        );
        anyhow::ensure!(
            (1..=32).contains(&viewer_slots),
            "viewer slots must be 1-32"
        );
        let upstream_port_base = upstream.port().expect("validated explicit port");
        let last_slot = u16::try_from(viewer_slots - 1)?;
        anyhow::ensure!(
            upstream_port_base.checked_add(last_slot).is_some(),
            "native signaling port range exceeds 65535"
        );
        anyhow::ensure!(
            public_media_port_base.checked_add(last_slot).is_some(),
            "public media port range exceeds 65535"
        );
        let slot_count = usize::try_from(viewer_slots)?;
        Ok(Self {
            service,
            subscriptions,
            upstream_host: upstream.host_str().expect("validated host").to_owned(),
            upstream_port_base,
            upstream_path: upstream.path().trim_end_matches('/').to_owned(),
            public_authority,
            public_path,
            public_media_port_base,
            slots: Arc::new((0..slot_count).map(|_| Mutex::new(())).collect()),
            connections: Arc::new(Semaphore::new(slot_count * 4)),
        })
    }

    pub(super) async fn run(
        self,
        address: SocketAddr,
        shutdown: CancellationToken,
    ) -> anyhow::Result<()> {
        let listener = TcpListener::bind(address).await?;
        tracing::info!(%address, "UAV native signaling gate listening");
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => return Ok(()),
                accepted = listener.accept() => {
                    let (stream, peer) = accepted?;
                    let gate = self.clone();
                    tokio::spawn(async move {
                        let permit = match gate.connections.clone().acquire_owned().await {
                            Ok(permit) => permit,
                            Err(_) => return,
                        };
                        let _permit = permit;
                        if let Err(error) = gate.handle(stream).await {
                            tracing::warn!(%peer, %error, "native signaling gate rejected connection");
                        }
                    });
                }
            }
        }
    }

    async fn handle(&self, mut downstream: TcpStream) -> anyhow::Result<()> {
        let (head, request_tail) = match read_headers(&mut downstream).await {
            Ok(value) => value,
            Err(error) => {
                let _ = write_http_error(&mut downstream, 400).await;
                return Err(error);
            }
        };
        let request = match self.parse_request(&head) {
            Ok(request) => request,
            Err(error) => {
                let _ = write_http_error(&mut downstream, error.status()).await;
                return Err(anyhow::anyhow!(error));
            }
        };
        let admission = match self.admit(&request).await {
            Ok(admission) => admission,
            Err(error) => {
                let _ = write_http_error(&mut downstream, signaling_status(&error)).await;
                return Err(anyhow::anyhow!(error));
            }
        };
        let _slot = self.slots[admission.slot].lock().await;
        let result = self
            .bridge(&request, &admission, request_tail, downstream)
            .await;
        if result.is_err() {
            self.service
                .cancel_signaling_admission(&admission.live_view_id, admission.kind)
                .await;
        }
        self.notify(&admission.session_id, &admission.live_view_id)
            .await;
        result
    }

    fn parse_request(&self, head: &[u8]) -> Result<UpgradeRequest, RequestError> {
        let text = std::str::from_utf8(head).map_err(|_| RequestError::BadRequest)?;
        let mut lines = text
            .strip_suffix("\r\n\r\n")
            .ok_or(RequestError::BadRequest)?
            .split("\r\n");
        let mut request_line = lines
            .next()
            .ok_or(RequestError::BadRequest)?
            .split_ascii_whitespace();
        if request_line.next() != Some("GET") {
            return Err(RequestError::MethodNotAllowed);
        }
        let target = request_line.next().ok_or(RequestError::BadRequest)?;
        if request_line.next() != Some("HTTP/1.1") || request_line.next().is_some() {
            return Err(RequestError::BadRequest);
        }
        let (path, raw_query) = target.split_once('?').unwrap_or((target, ""));
        let suffix = path
            .strip_prefix(&self.public_path)
            .filter(|suffix| suffix.is_empty() || suffix.starts_with('/'))
            .ok_or(RequestError::NotFound)?;
        if suffix.split('/').any(|part| part == "..") {
            return Err(RequestError::NotFound);
        }
        let mut headers = Vec::new();
        for line in lines {
            let (name, value) = line.split_once(':').ok_or(RequestError::BadRequest)?;
            if name.is_empty() || !name.bytes().all(header_name_byte) {
                return Err(RequestError::BadRequest);
            }
            headers.push((name.to_owned(), value.trim().to_owned()));
        }
        let host = unique_header(&headers, "host").ok_or(RequestError::BadRequest)?;
        if host.to_ascii_lowercase() != self.public_authority {
            return Err(RequestError::Forbidden);
        }
        if !header_has_token(&headers, "connection", "upgrade")
            || unique_header(&headers, "upgrade")
                .is_none_or(|value| !value.eq_ignore_ascii_case("websocket"))
        {
            return Err(RequestError::BadRequest);
        }
        let protocols = unique_header(&headers, "sec-websocket-protocol")
            .ok_or(RequestError::Unauthorized)?
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        let token = protocols
            .iter()
            .find_map(|value| value.strip_prefix(TOKEN_PROTOCOL_PREFIX))
            .filter(|value| !value.is_empty())
            .ok_or(RequestError::Unauthorized)?
            .to_owned();
        let session_protocol = protocols
            .iter()
            .find(|value| value.starts_with(SESSION_PROTOCOL_PREFIX))
            .ok_or(RequestError::Unauthorized)?
            .to_string();
        let query = unique_query(raw_query)?;
        let live_view_id = query
            .iter()
            .find(|(name, _)| name == LIVE_VIEW_QUERY)
            .map(|(_, value)| value.parse())
            .transpose()
            .map_err(|_| RequestError::Unauthorized)?
            .ok_or(RequestError::Unauthorized)?;
        let admission = if query
            .iter()
            .any(|(name, value)| name == "reconnect" && value == "1")
        {
            SignalingAdmission::Reconnect
        } else {
            SignalingAdmission::Initial
        };
        Ok(UpgradeRequest {
            headers,
            session_protocol,
            token,
            live_view_id,
            admission,
            suffix: suffix.to_owned(),
            query,
        })
    }

    async fn admit(&self, request: &UpgradeRequest) -> Result<Admission, LiveViewError> {
        let authorized = self
            .service
            .authorize_signaling(&request.live_view_id, &request.token, request.admission)
            .await?;
        let result = (|| {
            let slot = usize::from(authorized.state.capacity_slot);
            if slot >= self.slots.len()
                || self
                    .public_media_port_base
                    .checked_add(authorized.state.capacity_slot)
                    != Some(authorized.state.endpoint.media_port)
            {
                return Err(LiveViewError::Contract);
            }
            let expected_protocol = format!(
                "{SESSION_PROTOCOL_PREFIX}{}",
                authorized.state.stream_product_id
            );
            if request.session_protocol != expected_protocol {
                return Err(LiveViewError::Access);
            }
            let upstream_port = self
                .upstream_port_base
                .checked_add(authorized.state.capacity_slot)
                .ok_or(LiveViewError::Contract)?;
            let mut query = request
                .query
                .iter()
                .filter(|(name, _)| name != LIVE_VIEW_QUERY && name != "pairing_id")
                .cloned()
                .collect::<Vec<_>>();
            query.push((
                "pairing_id".to_owned(),
                authorized.state.stream_product_id.to_string(),
            ));
            let encoded = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(query)
                .finish();
            let path = format!("{}{}", self.upstream_path, request.suffix);
            Ok(Admission {
                session_id: authorized.state.session_id,
                live_view_id: request.live_view_id.clone(),
                session_protocol: request.session_protocol.clone(),
                upstream_target: if encoded.is_empty() {
                    path
                } else {
                    format!("{path}?{encoded}")
                },
                upstream_port,
                events: authorized.events,
                kind: authorized.admission,
                slot,
            })
        })();
        if result.is_err() {
            self.service
                .cancel_signaling_admission(&request.live_view_id, request.admission)
                .await;
        }
        result
    }

    async fn bridge(
        &self,
        request: &UpgradeRequest,
        admission: &Admission,
        request_tail: Vec<u8>,
        mut downstream: TcpStream,
    ) -> anyhow::Result<()> {
        let mut upstream = tokio::time::timeout(
            CONNECT_TIMEOUT,
            TcpStream::connect((self.upstream_host.as_str(), admission.upstream_port)),
        )
        .await??;
        upstream
            .write_all(&request.upstream_head(
                &admission.upstream_target,
                &self.upstream_host,
                admission.upstream_port,
            ))
            .await?;
        upstream.write_all(&request_tail).await?;
        upstream.flush().await?;
        let (response, response_tail) = read_headers(&mut upstream).await?;
        validate_upgrade_response(&response, &admission.session_protocol)?;
        downstream.write_all(&response).await?;
        downstream.write_all(&response_tail).await?;
        downstream.flush().await?;
        tracing::info!(
            live_view_id = %admission.live_view_id,
            slot = admission.slot,
            "accepted byte-transparent native signaling"
        );

        let (mut downstream_read, mut downstream_write) = downstream.into_split();
        let (mut upstream_read, mut upstream_write) = upstream.into_split();
        let mut downstream_to_upstream =
            Box::pin(tokio::io::copy(&mut downstream_read, &mut upstream_write));
        let mut upstream_to_downstream =
            Box::pin(tokio::io::copy(&mut upstream_read, &mut downstream_write));
        let mut lease_end = Box::pin(wait_for_lease_end(admission.events.clone()));
        tokio::select! {
            result = &mut downstream_to_upstream => { result?; }
            result = &mut upstream_to_downstream => { result?; }
            _ = &mut lease_end => {
                drop(downstream_to_upstream);
                drop(upstream_to_downstream);
                close_websocket_session(&mut upstream_write, &mut downstream_write).await?;
                return Ok(());
            }
        }
        Ok(())
    }

    async fn notify(
        &self,
        session_id: &veoveo_mcp_contract::LiveSessionId,
        live_view_id: &LiveViewId,
    ) {
        self.subscriptions
            .notify_resource_updated(uris::live_view(session_id, live_view_id))
            .await;
        self.subscriptions
            .notify_resource_updated(uris::live_views(session_id))
            .await;
    }
}

impl UpgradeRequest {
    fn upstream_head(&self, target: &str, host: &str, port: u16) -> Vec<u8> {
        let mut output = format!("GET {target} HTTP/1.1\r\n");
        for (name, value) in &self.headers {
            if matches!(
                name.to_ascii_lowercase().as_str(),
                "authorization" | "host" | "sec-websocket-protocol"
            ) {
                continue;
            }
            output.push_str(name);
            output.push_str(": ");
            output.push_str(value);
            output.push_str("\r\n");
        }
        output.push_str(&format!("Host: {host}:{port}\r\n"));
        output.push_str(&format!(
            "Sec-WebSocket-Protocol: {}\r\n\r\n",
            self.session_protocol
        ));
        output.into_bytes()
    }
}

async fn read_headers(stream: &mut TcpStream) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    tokio::time::timeout(HEADER_TIMEOUT, async {
        let mut input = Vec::with_capacity(4096);
        let end = loop {
            if input.len() >= MAX_SIGNALING_HEADER_BYTES {
                anyhow::bail!("signaling headers exceed the size limit");
            }
            let mut buffer = [0_u8; 4096];
            let read = stream.read(&mut buffer).await?;
            anyhow::ensure!(read != 0, "signaling peer closed before headers completed");
            input.extend_from_slice(&buffer[..read]);
            if let Some(position) = input.windows(4).position(|value| value == b"\r\n\r\n") {
                break position + 4;
            }
        };
        let tail = input.split_off(end);
        Ok((input, tail))
    })
    .await?
}

fn validate_upgrade_response(response: &[u8], protocol: &str) -> anyhow::Result<()> {
    let text = std::str::from_utf8(response)?;
    let mut lines = text.split("\r\n");
    anyhow::ensure!(
        matches!(lines.next(), Some("HTTP/1.1 101 Switching Protocols")),
        "native signaling rejected the WebSocket upgrade"
    );
    let headers = lines
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name.to_owned(), value.trim().to_owned()))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| anyhow::anyhow!("native signaling returned malformed headers"))?;
    anyhow::ensure!(
        unique_header(&headers, "sec-websocket-protocol") == Some(protocol),
        "simulator selected an unexpected signaling protocol"
    );
    Ok(())
}

async fn wait_for_lease_end(mut events: watch::Receiver<LeaseSignal>) {
    loop {
        let current = events.borrow().clone();
        if !current.active || current.expires_at <= chrono::Utc::now() {
            return;
        }
        let remaining = (current.expires_at - chrono::Utc::now())
            .to_std()
            .unwrap_or_default();
        tokio::select! {
            _ = tokio::time::sleep(remaining) => return,
            changed = events.changed() => if changed.is_err() { return; },
        }
    }
}

async fn close_websocket_session<U, D>(upstream: &mut U, downstream: &mut D) -> anyhow::Result<()>
where
    U: AsyncWrite + Unpin,
    D: AsyncWrite + Unpin,
{
    let payload = WEBSOCKET_GOING_AWAY.to_be_bytes();
    let mut mask = [0_u8; 4];
    getrandom::fill(&mut mask)?;
    let mut client_close = vec![0x88, 0x82];
    client_close.extend_from_slice(&mask);
    client_close.extend(
        payload
            .iter()
            .enumerate()
            .map(|(index, byte)| byte ^ mask[index]),
    );
    let server_close = [0x88, 0x02, payload[0], payload[1]];
    upstream.write_all(&client_close).await?;
    upstream.flush().await?;
    downstream.write_all(&server_close).await?;
    downstream.flush().await?;
    Ok(())
}

async fn write_http_error(stream: &mut TcpStream, status: u16) -> std::io::Result<()> {
    let reason = match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        503 => "Service Unavailable",
        _ => "Bad Gateway",
    };
    let response =
        format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    stream.write_all(response.as_bytes()).await
}

fn unique_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    let mut values = headers
        .iter()
        .filter(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str());
    let value = values.next()?;
    values.next().is_none().then_some(value)
}

fn header_has_token(headers: &[(String, String)], name: &str, token: &str) -> bool {
    unique_header(headers, name).is_some_and(|value| {
        value
            .split(',')
            .any(|value| value.trim().eq_ignore_ascii_case(token))
    })
}

fn header_name_byte(value: u8) -> bool {
    value.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&value)
}

fn unique_query(raw: &str) -> Result<Vec<(String, String)>, RequestError> {
    let mut result: Vec<(String, String)> = Vec::new();
    for (name, value) in url::form_urlencoded::parse(raw.as_bytes()) {
        if result.iter().any(|(prior, _)| prior == &name) {
            return Err(RequestError::BadRequest);
        }
        result.push((name.into_owned(), value.into_owned()));
    }
    Ok(result)
}

fn signaling_status(error: &LiveViewError) -> u16 {
    match error {
        LiveViewError::ViewNotFound(_) => 404,
        LiveViewError::Access => 403,
        _ => 503,
    }
}

#[derive(Debug, thiserror::Error)]
enum RequestError {
    #[error("malformed signaling request")]
    BadRequest,
    #[error("signaling host is forbidden")]
    Forbidden,
    #[error("signaling authentication is required")]
    Unauthorized,
    #[error("signaling route was not found")]
    NotFound,
    #[error("signaling method is not allowed")]
    MethodNotAllowed,
}

impl RequestError {
    fn status(&self) -> u16 {
        match self {
            Self::BadRequest => 400,
            Self::Unauthorized => 401,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::MethodNotAllowed => 405,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_parser_rejects_duplicate_authority_parameters() {
        assert!(unique_query("live_view_id=one&live_view_id=two").is_err());
        assert_eq!(
            unique_query("live_view_id=one&reconnect=1").unwrap(),
            vec![
                ("live_view_id".to_owned(), "one".to_owned()),
                ("reconnect".to_owned(), "1".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn revocation_close_frames_use_the_roles_required_by_websocket() {
        let (mut upstream_write, mut upstream_read) = tokio::io::duplex(64);
        let (mut downstream_write, mut downstream_read) = tokio::io::duplex(64);
        close_websocket_session(&mut upstream_write, &mut downstream_write)
            .await
            .unwrap();
        let mut client = [0_u8; 8];
        upstream_read.read_exact(&mut client).await.unwrap();
        assert_eq!(&client[..2], &[0x88, 0x82]);
        assert_eq!(client[6] ^ client[2], WEBSOCKET_GOING_AWAY.to_be_bytes()[0]);
        assert_eq!(client[7] ^ client[3], WEBSOCKET_GOING_AWAY.to_be_bytes()[1]);
        let mut server = [0_u8; 4];
        downstream_read.read_exact(&mut server).await.unwrap();
        assert_eq!(server, [0x88, 0x02, 0x03, 0xe9]);
    }
}
