//! HTTP/1.1 transport probes through maintained curl and owned process cleanup.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;
use veoveo_testing_support::process::{AsyncChild, spawn_async};
#[derive(Default)]
pub(super) struct Curl {
    child: Option<AsyncChild>,
}
#[derive(Clone, Copy)]
enum Host<'a> {
    Missing,
    Wrong,
    Admitted(&'a str),
}
fn command(
    url: &url::Url,
    host: Host<'_>,
    post: bool,
    body: &Path,
    status: File,
) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("curl");
    command.env_clear().env("PATH", "/usr/bin:/bin");
    command.args([
        "-q",
        "--http1.1",
        "--silent",
        "--noproxy",
        "*",
        "--proxy",
        "",
        "--proto",
        "=http",
        "--max-redirs",
        "0",
        "--max-time",
        "15",
        "--connect-timeout",
        "3",
        "--max-filesize",
        "65536",
        "--output",
    ]);
    command
        .arg(body)
        .args(["--write-out", "%{http_code}", "--header"]);
    match host {
        Host::Missing => {
            command.arg("Host:");
        }
        Host::Wrong => {
            command.arg("Host: outside.invalid");
        }
        Host::Admitted(host) => {
            command.arg(format!("Host: {host}"));
        }
    }
    if post {
        command.args([
            "--request",
            "POST",
            "--header",
            "Content-Type: application/json",
            "--data-binary",
            "{}",
        ]);
    }
    command
        .arg(url.as_str())
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .stdout(status);
    command
}
struct ExpectedResponse<'a> {
    status: u16,
    body: Option<&'a [u8]>,
}
impl Curl {
    async fn probe(
        &mut self,
        server: &SelectedServer,
        segments: &[&str],
        host: Host<'_>,
        post: bool,
        expected: ExpectedResponse<'_>,
        evidence: &mut Evidence,
    ) -> Result<()> {
        ensure!(self.child.is_none(), "prior curl process requires cleanup");
        let mut url = server.origin.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow!("direct origin cannot hold path"))?
            .clear()
            .extend(server.mount_path.as_str().trim_matches('/').split('/'))
            .extend(segments.iter().copied());
        let method = if post {
            Method::HttpPost
        } else {
            Method::HttpGet
        };
        let index = evidence.begin_http(Phase::Transport, method, url.clone(), expected.status)?;
        let mut body = tempfile::NamedTempFile::new()?;
        let mut status = tempfile::NamedTempFile::new()?;
        self.child = Some(spawn_async(command(
            &url,
            host,
            post,
            body.path(),
            status.reopen()?,
        ))?);
        let wait = tokio::time::timeout(
            Duration::from_secs(15),
            self.child.as_mut().expect("owned child").wait(),
        )
        .await;
        let exit_success = match wait {
            Ok(Ok(exit)) => {
                self.child.take();
                exit.success()
            }
            Ok(Err(_)) => {
                evidence.transport_failed(index, false)?;
                bail!("curl process observation failed; see receipt");
            }
            Err(_) => {
                evidence.transport_failed(index, true)?;
                bail!("curl request deadline; see receipt");
            }
        };
        let mut code = String::new();
        status.as_file_mut().take(4).read_to_string(&mut code)?;
        let status: u16 = code
            .parse()
            .map_err(|_| anyhow!("curl status admission failed"))?;
        if !exit_success || !(100..=599).contains(&status) {
            if (100..=599).contains(&status) {
                evidence.http_status(index, status)?;
            }
            evidence.transport_failed(index, false)?;
            bail!("curl transport failed; see receipt");
        }
        let mut bytes = Vec::new();
        body.as_file_mut().take(65537).read_to_end(&mut bytes)?;
        let digest = Sha256Digest::from_bytes(Sha256::digest(&bytes).into());
        evidence.http_observed(index, status, digest)?;
        ensure!(
            bytes.len() <= 65536 && status == expected.status,
            "curl response differed; see receipt"
        );
        if let Some(expected) = expected.body {
            ensure!(
                bytes == expected,
                "curl response message differed; see receipt"
            );
        }
        Ok(())
    }
    pub(super) async fn run(
        &mut self,
        servers: &[SelectedServer],
        evidence: &mut Evidence,
    ) -> Result<()> {
        for server in servers {
            self.probe(
                server,
                &["healthz"],
                Host::Missing,
                false,
                ExpectedResponse {
                    status: 400,
                    body: None,
                },
                evidence,
            )
            .await?;
            self.probe(
                server,
                &["healthz"],
                Host::Wrong,
                false,
                ExpectedResponse {
                    status: 421,
                    body: None,
                },
                evidence,
            )
            .await?;
            for route in ["healthz", "readyz"] {
                self.probe(
                    server,
                    &[route],
                    Host::Admitted(&server.allowed_host),
                    false,
                    ExpectedResponse {
                        status: 200,
                        body: None,
                    },
                    evidence,
                )
                .await?;
            }
            if server.owner == SelectedOwner::Media {
                let task = veoveo_types::TaskId::new().to_string();
                self.probe(
                    server,
                    &["webhooks", task.as_str()],
                    Host::Admitted(&server.allowed_host),
                    true,
                    ExpectedResponse {
                        status: 401,
                        body: Some(b"invalid signature"),
                    },
                    evidence,
                )
                .await?;
            }
        }
        Ok(())
    }
    pub(super) async fn close(&mut self) -> bool {
        let Some(mut child) = self.child.take() else {
            return true;
        };
        let _ = child.start_kill();
        matches!(
            tokio::time::timeout(Duration::from_secs(5), child.wait()).await,
            Ok(Ok(_))
        )
    }
}

#[cfg(test)]
mod protocol_transport_tests {
    use super::*;
    #[test]
    fn curl_commands_suppress_host_and_confine_transport_without_credentials() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let body = directory.path().join("body");
        let status = File::create(directory.path().join("status"))?;
        let url = url::Url::parse("http://127.0.0.1:18001/media/healthz")?;
        let command = command(&url, Host::Missing, false, &body, status);
        let arguments = command
            .as_std()
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--header", "Host:"])
        );
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--proto", "=http"])
        );
        assert!(arguments.windows(2).any(|pair| pair == ["--proxy", ""]));
        assert!(
            arguments
                .windows(2)
                .any(|pair| pair == ["--max-time", "15"])
        );
        assert!(
            !arguments
                .iter()
                .any(|argument| argument == "--location" || argument.contains("Authorization"))
        );
        assert_eq!(
            command.as_std().get_envs().collect::<Vec<_>>(),
            vec![(
                std::ffi::OsStr::new("PATH"),
                Some(std::ffi::OsStr::new("/usr/bin:/bin"))
            )]
        );
        Ok(())
    }
}
