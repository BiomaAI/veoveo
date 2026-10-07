//! Installed sharing through MCP and anonymous HTTP on the installation origin.
use anyhow::{Context, Result, ensure};
use chrono::{TimeDelta, Utc};
use reqwest::{Client, Method, StatusCode, Url};
use rmcp::{Peer, RoleClient};
use serde::{Deserialize, Serialize};
use std::{fs, io::Write, path::PathBuf, time::Duration};
use veoveo_artifact_mcp::contract::{
    ArtifactId, ArtifactMetadata, ArtifactMetadataOutput, ArtifactMutationOutput,
    ArtifactReleaseState, ArtifactResource, ArtifactShareLink, ArtifactShareOutput,
    CreateArtifactShareRequest, RevokeArtifactShareRequest, SetArtifactReleaseRequest,
    ShareLinkOptions,
};
use veoveo_types::AccessSubject;

// This domain test uses the shared transport/input subset, not its source report.
#[allow(dead_code)]
use veoveo_testing_support::installed::knowledge as installed;
use veoveo_testing_support::installed::tools;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Input {
    installation: installed::InstalledSource,
    artifact: ArtifactId,
    owner: AccessSubject,
    body_file: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum SharingCheck {
    PrivateFixtureOwnershipAndRetention,
    ReadOnlyAnonymousBytes,
    DownloadLimit,
    ExplicitRevocation,
    Expiry,
    ParentReleaseState,
    PrivateStateAndLinkCleanup,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: &'static str,
    artifact: ArtifactId,
    checks: Vec<SharingCheck>,
    failure: Option<String>,
    cleanup_failures: Vec<String>,
}

struct OwnedLink {
    link: ArtifactShareLink,
    revoked: bool,
}

struct Sharing {
    peer: Peer<RoleClient>,
    client: Client,
    origin: Url,
    artifact: ArtifactId,
    body: Vec<u8>,
    links: Vec<OwnedLink>,
    checks: Vec<SharingCheck>,
}

#[tokio::test]
#[ignore = "requires installed Artifact, a private admin token and an explicitly disposable private Artifact"]
async fn anonymous_sharing_respects_limits_expiry_revocation_and_release() -> Result<()> {
    let input: Input = installed::input_from("VEOVEO_ARTIFACT_SHARING_INPUT")?;
    let target = input.installation.validate()?;
    ensure!(
        input.installation.deployment == "artifact-mcp",
        "sharing fixture must select Artifact MCP"
    );
    ensure!(
        input.body_file.is_absolute(),
        "fixture body path must be absolute"
    );
    let metadata = fs::metadata(&input.body_file)?;
    ensure!(
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= 64 * 1024,
        "fixture body must be a nonempty regular file no larger than 64 KiB"
    );
    let body = fs::read(&input.body_file)?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut output = options.open(&input.installation.output)?;
    let caller = match input.installation.caller().await {
        Ok(caller) => caller,
        Err(error) => {
            return finish_report(&mut output, &input, Vec::new(), Err(error), Vec::new());
        }
    };
    let mut sharing = Sharing {
        peer: caller.peer().clone(),
        client: Client::builder()
            .user_agent("veoveo-installed-artifact-sharing/1")
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        origin: target.public_base_url,
        artifact: input.artifact,
        body,
        links: Vec::new(),
        checks: Vec::new(),
    };
    // Finish read-only preflight before permitting release or share mutations.
    let preflight = sharing.preflight(&input.owner).await;
    let (result, mut cleanup_failures) = if preflight.is_ok() {
        let result = tokio::time::timeout(Duration::from_secs(180), sharing.exercise())
            .await
            .context("sharing checks exceeded three minutes")
            .and_then(|result| result);
        (result, sharing.cleanup().await)
    } else {
        (preflight, Vec::new())
    };
    if let Err(error) = installed::close(caller).await {
        cleanup_failures.push(format!("close MCP connection: {error:#}"));
    }
    finish_report(
        &mut output,
        &input,
        sharing.checks,
        result,
        cleanup_failures,
    )
}

fn finish_report(
    output: &mut fs::File,
    input: &Input,
    checks: Vec<SharingCheck>,
    result: Result<()>,
    cleanup_failures: Vec<String>,
) -> Result<()> {
    let report = Report {
        schema_version: "veoveo.ai/artifact-sharing-acceptance/v1",
        artifact: input.artifact,
        checks,
        failure: result.as_ref().err().map(|error| format!("{error:#}")),
        cleanup_failures,
    };
    serde_json::to_writer_pretty(&mut *output, &report)?;
    output.write_all(b"\n")?;
    output.sync_all()?;
    ensure!(
        report.failure.is_none() && report.cleanup_failures.is_empty(),
        "installed sharing failed; inspect {}",
        input.installation.output.display()
    );
    Ok(())
}

impl Sharing {
    async fn metadata(&self) -> Result<ArtifactMetadata> {
        installed::read(
            &self.peer,
            &ArtifactResource::Metadata(self.artifact).to_uri(),
        )
        .await
    }

    async fn preflight(&mut self, owner: &AccessSubject) -> Result<()> {
        let metadata = self.metadata().await?;
        ensure!(
            metadata.artifact_id() == self.artifact
                && metadata.compliance.owner.as_ref() == Some(owner)
                && metadata.release_state == ArtifactReleaseState::Private
                && metadata.byte_len == self.body.len() as u64,
            "selected fixture must match the explicit owner, private state and expected byte length"
        );
        ensure!(
            metadata
                .compliance
                .retention_expires_at
                .is_none_or(|expiry| { expiry > Utc::now() + TimeDelta::minutes(10) }),
            "fixture retention must cover checks and cleanup"
        );
        self.checks
            .push(SharingCheck::PrivateFixtureOwnershipAndRetention);
        Ok(())
    }

    async fn release(&self, release_state: ArtifactReleaseState) -> Result<()> {
        let receipt: ArtifactMetadataOutput = tools::call(
            &self.peer,
            "artifact__set_release_state".parse()?,
            &SetArtifactReleaseRequest {
                artifact_id: self.artifact,
                release_state,
            },
        )
        .await?;
        ensure!(
            receipt.artifact.artifact_id() == self.artifact
                && receipt.artifact.release_state == release_state,
            "release receipt does not match the selected Artifact and requested state"
        );
        ensure!(
            self.metadata().await?.release_state == release_state,
            "release readback differs"
        );
        Ok(())
    }

    async fn create(&mut self, seconds: i64, max_downloads: Option<u64>) -> Result<usize> {
        let expires_at = Utc::now() + TimeDelta::seconds(seconds);
        let receipt: ArtifactShareOutput = tools::call(
            &self.peer,
            "artifact__create_share_link".parse()?,
            &CreateArtifactShareRequest {
                artifact_id: self.artifact,
                options: ShareLinkOptions {
                    expires_at: Some(expires_at),
                    max_downloads,
                },
            },
        )
        .await?;
        let index = self.links.len();
        // Track returned identity before assertions so cleanup owns every receipt.
        self.links.push(OwnedLink {
            link: receipt.share_link,
            revoked: false,
        });
        let link = &self.links[index].link;
        ensure!(
            link.artifact_id == self.artifact
                && link.expires_at == expires_at
                && link.max_downloads.map(|limit| limit.get()) == max_downloads,
            "share receipt does not match the requested Artifact, expiry or download limit"
        );
        let url = Url::parse(&link.url).context("invalid share URL")?;
        ensure!(
            url.origin() == self.origin.origin()
                && url.username().is_empty()
                && url.password().is_none()
                && url.query().is_none()
                && url.fragment().is_none(),
            "share must use the installation origin without embedded credentials, query or fragment"
        );
        Ok(index)
    }

    async fn request(&self, index: usize, method: Method, expected: StatusCode) -> Result<()> {
        let url = Url::parse(&self.links[index].link.url).context("invalid share URL")?;
        ensure!(
            url.origin() == self.origin.origin()
                && url.username().is_empty()
                && url.password().is_none(),
            "share request must stay on the installation origin"
        );
        let mut response = self
            .client
            .request(method, url)
            .send()
            .await
            .map_err(reqwest::Error::without_url)?;
        ensure!(
            response.status() == expected,
            "anonymous share returned {}, expected {expected}",
            response.status()
        );
        if expected == StatusCode::OK {
            let mut body = Vec::new();
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(reqwest::Error::without_url)?
            {
                ensure!(
                    body.len() + chunk.len() <= self.body.len(),
                    "share exceeded fixture size"
                );
                body.extend_from_slice(&chunk);
            }
            ensure!(
                body == self.body,
                "anonymous share bytes differ from fixture"
            );
        }
        Ok(())
    }

    async fn revoke(&mut self, index: usize) -> Result<()> {
        let receipt: ArtifactMutationOutput = tools::call(
            &self.peer,
            "artifact__revoke_share_link".parse()?,
            &RevokeArtifactShareRequest {
                artifact_id: self.artifact,
                link_id: self.links[index].link.link_id,
            },
        )
        .await?;
        ensure!(
            receipt.artifact_id == self.artifact && receipt.changed,
            "share revocation receipt does not match the selected Artifact"
        );
        self.links[index].revoked = true;
        self.request(index, Method::GET, StatusCode::NOT_FOUND)
            .await
    }

    async fn exercise(&mut self) -> Result<()> {
        self.release(ArtifactReleaseState::Releasable).await?;
        let limited = self.create(120, Some(1)).await?;
        self.request(limited, Method::POST, StatusCode::METHOD_NOT_ALLOWED)
            .await?;
        self.request(limited, Method::GET, StatusCode::OK).await?;
        self.checks.push(SharingCheck::ReadOnlyAnonymousBytes);
        self.request(limited, Method::GET, StatusCode::NOT_FOUND)
            .await?;
        self.checks.push(SharingCheck::DownloadLimit);

        let revocable = self.create(120, None).await?;
        self.request(revocable, Method::GET, StatusCode::OK).await?;
        self.revoke(revocable).await?;
        self.checks.push(SharingCheck::ExplicitRevocation);

        let expiring = self.create(10, None).await?;
        self.request(expiring, Method::GET, StatusCode::OK).await?;
        let remaining = (self.links[expiring].link.expires_at + TimeDelta::seconds(1) - Utc::now())
            .to_std()
            .unwrap_or_default();
        tokio::time::sleep(remaining).await;
        self.request(expiring, Method::GET, StatusCode::NOT_FOUND)
            .await?;
        self.checks.push(SharingCheck::Expiry);

        let parent = self.create(120, None).await?;
        self.request(parent, Method::GET, StatusCode::OK).await?;
        self.release(ArtifactReleaseState::Private).await?;
        self.request(parent, Method::GET, StatusCode::NOT_FOUND)
            .await?;
        self.release(ArtifactReleaseState::Released).await?;
        self.request(parent, Method::GET, StatusCode::OK).await?;
        self.checks.push(SharingCheck::ParentReleaseState);
        Ok(())
    }

    async fn cleanup(&mut self) -> Vec<String> {
        let mut failures = Vec::new();
        // Restore private first, including when a create response was lost and its
        // link ID is unknown. Never replay a create operation after a timeout.
        if let Err(error) = self.release(ArtifactReleaseState::Private).await {
            failures.push(format!("restore private release state: {error:#}"));
        }
        for index in 0..self.links.len() {
            if !self.links[index].revoked
                && let Err(error) = self.revoke(index).await
            {
                failures.push(format!(
                    "revoke owned link {}: {error:#}",
                    self.links[index].link.link_id
                ));
            }
        }
        if failures.is_empty() {
            self.checks.push(SharingCheck::PrivateStateAndLinkCleanup);
        }
        failures
    }
}
