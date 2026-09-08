//! Metadata-only observation of the native clients through kubectl's local proxy.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub(super) enum Method {
    Get,
    Head,
    Options,
    Post,
    Put,
    Patch,
    Delete,
    Connect,
}
impl Method {
    pub fn writes(&self) -> bool {
        !matches!(self, Self::Get | Self::Head | Self::Options)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Request {
    pub method: Method,
    pub uri: String,
}

pub(super) struct Proxy {
    child: Child,
    reader: Option<thread::JoinHandle<()>>,
    requests: Arc<Mutex<Vec<Request>>>,
    errors: Arc<Mutex<Vec<String>>>,
    pub config: PathBuf,
    pub context: String,
    barrier: u64,
}

impl Proxy {
    pub fn start(context: &str, name: &str, directory: &Path) -> Result<Self> {
        let mut child = Command::new("kubectl")
            .args([
                "--context",
                context,
                "proxy",
                "--address=127.0.0.1",
                "--port=0",
                "--v=6",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().context("proxy stdout missing")?;
        let stderr = child.stderr.take().context("proxy stderr missing")?;
        let (send, receive) = mpsc::channel();
        thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
            let _ = send.send(result);
        });
        let requests = Arc::new(Mutex::new(Vec::new()));
        let errors = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let failures = errors.clone();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stderr).lines() {
                match line {
                    Ok(line) => match parse(&line) {
                        Ok(Some(request)) => seen.lock().unwrap().push(request),
                        Ok(None) => {}
                        Err(error) => failures.lock().unwrap().push(error.to_string()),
                    },
                    Err(error) => {
                        failures.lock().unwrap().push(error.to_string());
                        break;
                    }
                }
            }
        });
        let mut proxy = Self {
            child,
            reader: Some(reader),
            requests,
            errors,
            config: directory.join("proxy.json"),
            context: name.into(),
            barrier: 0,
        };
        let line = receive
            .recv_timeout(Duration::from_secs(10))
            .context("proxy startup timed out")??;
        let address = line
            .trim()
            .strip_prefix("Starting to serve on 127.0.0.1:")
            .context("unknown proxy startup output")?;
        let port: u16 = address.parse()?;
        ensure!(port != 0, "proxy has no bound port");
        fs::write(
            &proxy.config,
            serde_json::to_vec_pretty(&serde_json::json!({
                "apiVersion":"v1", "kind":"Config", "clusters":[{"name":name,"cluster":{"server":format!("http://127.0.0.1:{port}")}}],
                "contexts":[{"name":name,"context":{"cluster":name,"user":name}}],
                "users":[{"name":name,"user":{}}], "current-context":name
            }))?,
        )?;
        proxy.checkpoint()?;
        Ok(proxy)
    }

    pub fn kubectl(&self) -> Command {
        let mut command = Command::new("kubectl");
        command
            .env("KUBECONFIG", &self.config)
            .args(["--context", &self.context]);
        command
    }

    pub fn checkpoint(&mut self) -> Result<usize> {
        ensure!(self.child.try_wait()?.is_none(), "API observer stopped");
        self.barrier += 1;
        let uri = format!("/version?veoveoScopeBarrier={}", self.barrier);
        let response = self.kubectl().args(["get", "--raw", &uri]).output()?;
        ensure!(response.status.success(), "API observer barrier failed");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            ensure!(
                self.errors.lock().unwrap().is_empty(),
                "API observer rejected a request or encountered unsupported log output"
            );
            let requests = self.requests.lock().unwrap();
            if requests.iter().any(|request| request.uri == uri) {
                return Ok(requests.len());
            }
            drop(requests);
            ensure!(
                Instant::now() < deadline,
                "API observer did not record the barrier"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub fn since(&mut self, start: usize) -> Result<Vec<Request>> {
        let end = self.checkpoint()?;
        Ok(self.requests.lock().unwrap()[start..end].to_vec())
    }
}

impl Drop for Proxy {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn parse(line: &str) -> Result<Option<Request>> {
    ensure!(
        !line.contains("Filter rejecting"),
        "API proxy rejected an attempted request"
    );
    let Some((_, fields)) = line.split_once("\"Response\" ") else {
        return Ok(None);
    };
    let field = |name: &str| -> Result<&str> {
        fields
            .split_once(&format!("{name}=\""))
            .and_then(|(_, value)| value.split_once('"').map(|(value, _)| value))
            .context("unknown response-request metadata format")
    };
    let method = serde_json::from_value(serde_json::Value::String(field("verb")?.into()))?;
    let url = field("url")?;
    let address = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .context("observer URL has no HTTP scheme")?;
    let (_, path) = address
        .split_once('/')
        .context("observer URL has no API path")?;
    Ok(Some(Request {
        method,
        uri: format!("/{path}"),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observer_recognizes_writes_and_rejects_ambiguous_metadata() {
        let request = parse(r#"I123 round_trippers.go] "Response" verb="PATCH" url="https://127.0.0.1:123/api/v1/namespaces/test/configmaps/platform" status="200 OK""#).unwrap().unwrap();
        assert!(request.method.writes());
        assert!(parse(r#""Response" verb="GET" status="200 OK""#).is_err());
        assert!(
            parse("Filter rejecting POST /api/v1/namespaces/test/pods/extension/exec").is_err()
        );
    }
}
