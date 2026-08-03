use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;

use super::output_checked;

const SIMULATION_VIEW_SERVICE: &str = "simulation-view-mcp";
const SIMULATION_VIEW_RUNTIME_PATH: &str = "/simulation-view/runtimez";

#[derive(Debug, Deserialize)]
struct KubernetesService {
    spec: KubernetesServiceSpec,
}

#[derive(Debug, Deserialize)]
struct KubernetesServiceSpec {
    ports: Vec<KubernetesServicePort>,
}

#[derive(Debug, Deserialize)]
struct KubernetesServicePort {
    name: String,
    port: u16,
}

struct PortForward {
    child: Child,
}

impl Drop for PortForward {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn verify_simulation_view_runtime(
    context: &str,
    namespace: &str,
    timeout: Duration,
) -> Result<()> {
    let service = simulation_view_service(context, namespace)?;
    let remote_port = service
        .spec
        .ports
        .iter()
        .find(|port| port.name == "http")
        .map(|port| port.port)
        .context("Simulation View Service omits its named HTTP port")?;
    let local_port = reserve_loopback_port()?;
    let mapping = format!("{local_port}:{remote_port}");
    let child = Command::new("kubectl")
        .args([
            "--context",
            context,
            "--namespace",
            namespace,
            "port-forward",
            "service/simulation-view-mcp",
            mapping.as_str(),
            "--address=127.0.0.1",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("starting Simulation View runtime readiness port-forward")?;
    let mut forward = PortForward { child };
    let deadline = Instant::now() + timeout;
    let mut last_response = None;
    loop {
        if let Some(status) = forward
            .child
            .try_wait()
            .context("checking Simulation View readiness port-forward")?
        {
            bail!(
                "Simulation View readiness port-forward exited with {status} before runtime convergence"
            );
        }
        if let Ok(response) = get_runtime_readiness(local_port, remote_port) {
            if response.status == 200 {
                println!("Simulation View runtime is durably reconciled");
                return Ok(());
            }
            last_response = Some(response);
        }
        if Instant::now() >= deadline {
            let diagnostic = last_response
                .map(|response| {
                    let body = response.body.chars().take(4096).collect::<String>();
                    format!("HTTP {}: {body}", response.status)
                })
                .unwrap_or_else(|| "the readiness endpoint was unreachable".to_owned());
            bail!(
                "Simulation View runtime did not become durably reconciled within {} seconds: {diagnostic}",
                timeout.as_secs()
            );
        }
        thread::sleep(Duration::from_millis(250));
    }
}

fn simulation_view_service(context: &str, namespace: &str) -> Result<KubernetesService> {
    let output = output_checked(
        "kubectl",
        [
            "--context",
            context,
            "--namespace",
            namespace,
            "get",
            "service",
            SIMULATION_VIEW_SERVICE,
            "-o",
            "json",
        ],
        None,
    )?;
    serde_json::from_slice(&output).context("decoding Simulation View Service")
}

fn reserve_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
        .context("reserving a loopback port for Simulation View readiness")?;
    Ok(listener.local_addr()?.port())
}

struct HttpResponse {
    status: u16,
    body: String,
}

fn get_runtime_readiness(local_port: u16, remote_port: u16) -> Result<HttpResponse> {
    let mut stream = TcpStream::connect_timeout(
        &SocketAddrV4::new(Ipv4Addr::LOCALHOST, local_port).into(),
        Duration::from_secs(1),
    )?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    write!(
        stream,
        "GET {SIMULATION_VIEW_RUNTIME_PATH} HTTP/1.1\r\nHost: {SIMULATION_VIEW_SERVICE}:{remote_port}\r\nConnection: close\r\n\r\n"
    )?;
    stream.flush()?;
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    let response = String::from_utf8(bytes).context("decoding runtime readiness response")?;
    let (head, body) = response
        .split_once("\r\n\r\n")
        .context("runtime readiness returned an invalid HTTP response")?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_ascii_whitespace().nth(1))
        .context("runtime readiness response omitted its HTTP status")?
        .parse::<u16>()
        .context("runtime readiness returned an invalid HTTP status")?;
    ensure!(
        (100..=599).contains(&status),
        "runtime readiness returned an invalid HTTP status {status}"
    );
    Ok(HttpResponse {
        status,
        body: body.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runtime_readiness_without_an_http_client_dependency() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                let mut chunk = [0_u8; 128];
                let length = stream.read(&mut chunk).unwrap();
                assert!(length > 0, "readiness request ended before its headers");
                request.extend_from_slice(&chunk[..length]);
            }
            let request = std::str::from_utf8(&request).unwrap();
            assert!(
                request.starts_with("GET /simulation-view/runtimez HTTP/1.1\r\n"),
                "unexpected readiness request: {request:?}"
            );
            write!(
                stream,
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 15\r\nConnection: close\r\n\r\n{{\"ready\":false}}"
            )
            .unwrap();
        });

        let response = get_runtime_readiness(port, 8788).unwrap();

        assert_eq!(response.status, 503);
        assert_eq!(response.body, "{\"ready\":false}");
        server.join().unwrap();
    }
}
