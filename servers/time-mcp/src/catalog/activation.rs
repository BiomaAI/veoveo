//! A preflight draft keeps the observed database selection until publication.
use super::*;
use crate::{
    contract::{AuthorityReleaseId, TimeVersion, TimeWriteGuard},
    persistence::{ActiveAuthoritySnapshot, AuthorityActivation, TimeAuthorityReleaseRecord},
};

pub(crate) struct ActivationDraft {
    pub(crate) release: AuthorityRelease,
    candidate: TimeAuthorityReleaseRecord,
    snapshot: ActiveAuthoritySnapshot,
    expected_release: TimeVersion,
    expected_pointer: TimeWriteGuard,
}

impl ActivationDraft {
    pub(crate) fn active_releases(&self) -> Result<Vec<AuthorityRelease>> {
        self.snapshot.releases().map(release_from_record).collect()
    }
}

impl TimeCatalog {
    pub(crate) async fn prepare_activation(
        &self,
        scope: &TimeAccessContext,
        id: &AuthorityReleaseId,
        expected_release: TimeVersion,
        expected_pointer: TimeWriteGuard,
    ) -> Result<ActivationDraft> {
        let candidate = self
            .persistence
            .time_authority_release(scope.identity.tenant_id, id)
            .await?
            .context("unknown authority release")?;
        let release = release_from_record(candidate.clone())?;
        if release.state != AuthorityReleaseState::Staged
            || release.record_version != expected_release.get()
        {
            bail!("authority activation requires the expected staged release version");
        }
        expected_release.checked_next()?;
        expected_pointer.next_version()?;
        let snapshot = self
            .persistence
            .active_authority_snapshot(scope.identity.tenant_id)
            .await?;
        let draft = ActivationDraft {
            release,
            candidate,
            snapshot,
            expected_release,
            expected_pointer,
        };
        // Admit every retained release before any file loading or mutation.
        draft.active_releases()?;
        Ok(draft)
    }

    pub(crate) async fn commit_activation(
        &self,
        scope: &TimeAccessContext,
        draft: ActivationDraft,
    ) -> Result<AuthorityRelease> {
        let ActivationDraft {
            mut release,
            candidate,
            snapshot,
            expected_release,
            expected_pointer,
        } = draft;
        release.state = AuthorityReleaseState::Active;
        release.record_version = expected_release.checked_next()?.get();
        let canonical_json = serde_json::to_string(&release)?;
        let record = self
            .persistence
            .commit_time_authority_release(
                &scope.identity,
                AuthorityActivation {
                    candidate,
                    snapshot,
                    expected_release,
                    expected_pointer,
                    canonical_json,
                },
            )
            .await?;
        release_from_record(record)
    }

    /// Native catalog fixtures exercise persistence without authority files.
    #[cfg(test)]
    pub(crate) async fn activate_release(
        &self,
        scope: &TimeAccessContext,
        id: &AuthorityReleaseId,
        expected_release: TimeVersion,
        expected_pointer: TimeWriteGuard,
    ) -> Result<AuthorityRelease> {
        let draft = self
            .prepare_activation(scope, id, expected_release, expected_pointer)
            .await?;
        self.commit_activation(scope, draft).await
    }
}
