//! Canonical redacted viewer authorizations and immutable closed receipts.
use crate::contract::{LiveViewId, LiveViewOwner, LiveViewState};
use chrono::{DateTime, Utc};
use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::{Error, Kind, RecordId, SurrealValue, Value};
use veoveo_platform_store::PlatformStore;

#[veoveo_types::id(uuid(LiveViewIssuerProfile), fresh)]
pub(super) struct LiveViewIssuerId(uuid::Uuid);

#[derive(Debug, thiserror::Error)]
#[error("invalid private UAV issuing process")]
pub(super) struct IssuerError;
pub(super) struct LiveViewIssuerProfile;
impl veoveo_types::IdProfile for LiveViewIssuerProfile {
    type Error = IssuerError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::generated_uuid(
            veoveo_types::UuidGrammar::canonical(&[7]),
            |_, _, _| IssuerError,
        );
}

#[derive(Clone, Debug)]
pub(super) struct Document<T>(pub T);
impl<T: Serialize + DeserializeOwned> SurrealValue for Document<T> {
    fn kind_of() -> Kind {
        Kind::Object
    }
    fn into_value(self) -> Value {
        veoveo_platform_store::native_json_into_value(
            serde_json::to_value(self.0).expect("typed live view document"),
        )
    }
    fn from_value(value: Value) -> Result<Self, Error> {
        serde_json::from_value(veoveo_platform_store::native_json_from_value(value)?)
            .map(Self)
            .map_err(|_| Error::internal("invalid live view document".into()))
    }
}

#[derive(Clone, Debug, SurrealValue)]
pub(super) struct Grant {
    pub state: Document<LiveViewState>,
    pub issuer: uuid::Uuid,
    pub token_hash: Vec<u8>,
    pub generation: i64,
    pub expires_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}
impl Grant {
    pub fn validate(&self) -> anyhow::Result<()> {
        self.state.0.validate()?;
        anyhow::ensure!(
            self.state.0.resource_uri.as_str()
                == crate::uris::live_view(&self.state.0.session_id, &self.state.0.live_view_id)
                    .as_str(),
            "stored live view URI disagrees"
        );
        anyhow::ensure!(
            self.generation > 0 && self.token_hash.len() == 32,
            "invalid stored live view generation/digest"
        );
        anyhow::ensure!(
            self.expires_at == self.state.0.expires_at,
            "stored live view expiry disagrees"
        );
        anyhow::ensure!(
            self.closed_at.is_some()
                == (self.state.0.lifecycle == crate::contract::LiveViewLifecycle::Closed),
            "stored live view closure disagrees"
        );
        Ok(())
    }
}
#[derive(SurrealValue)]
pub(super) struct ProductUsage {
    pub product: String,
    pub active: i64,
    pub connected: i64,
}
#[derive(Clone)]
pub(super) struct LiveViewStore {
    pub store: PlatformStore,
}
impl LiveViewStore {
    pub fn new(store: PlatformStore) -> Self {
        Self { store }
    }
    fn key(id: &LiveViewId) -> RecordId {
        RecordId::new("uav_live_view", id.as_str())
    }
    pub async fn create(&self, grant: Grant) -> anyhow::Result<()> {
        grant.validate()?;
        self.store
            .client()
            .query(include_str!("live_view_store/queries/create.surql"))
            .bind(("key", Self::key(&grant.state.0.live_view_id)))
            .bind(("grant", grant))
            .await?
            .check()?;
        Ok(())
    }
    pub async fn get(&self, id: &LiveViewId) -> anyhow::Result<Option<Grant>> {
        let mut response = self
            .store
            .client()
            .query("SELECT * FROM ONLY $key;")
            .bind(("key", Self::key(id)))
            .await?
            .check()?;
        let grant: Option<Grant> = response.take(0)?;
        if let Some(grant) = &grant {
            grant.validate()?;
            anyhow::ensure!(
                &grant.state.0.live_view_id == id,
                "stored live view identity disagrees"
            );
        }
        Ok(grant)
    }
    pub async fn selected(
        &self,
        owner: &LiveViewOwner,
        actor: &veoveo_types::PrincipalId,
        session: Option<&crate::contract::LiveSessionId>,
        after: Option<&LiveViewId>,
    ) -> anyhow::Result<Vec<Grant>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("live_view_store/queries/page.surql"))
            .bind(("owner", Document(owner.clone())))
            .bind(("actor", actor.to_string()))
            .bind(("live_session", session.map(ToString::to_string)))
            .bind(("after", after.map(ToString::to_string)))
            .await?
            .check()?;
        let rows: Vec<Grant> = response.take(0)?;
        for row in &rows {
            row.validate()?;
        }
        Ok(rows)
    }
    pub async fn owned(
        &self,
        id: &LiveViewId,
        owner: &LiveViewOwner,
        actor: &veoveo_types::PrincipalId,
        session: Option<&crate::contract::LiveSessionId>,
        viewer: Option<&crate::contract::LiveViewerInstanceId>,
    ) -> anyhow::Result<Option<Grant>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("live_view_store/queries/owned.surql"))
            .bind(("key", Self::key(id)))
            .bind(("owner", Document(owner.clone())))
            .bind(("actor", actor.to_string()))
            .bind(("live_session", session.map(ToString::to_string)))
            .bind(("viewer", viewer.map(ToString::to_string)))
            .await?
            .check()?;
        let rows: Vec<Grant> = response.take(0)?;
        let row = rows.into_iter().next();
        if let Some(row) = &row {
            row.validate()?;
            anyhow::ensure!(
                &row.state.0.live_view_id == id,
                "stored live view identity disagrees"
            );
        }
        Ok(row)
    }
    pub async fn stable_owned(
        &self,
        id: &LiveViewId,
        owner: &LiveViewOwner,
        actor: &veoveo_types::PrincipalId,
        session: Option<&crate::contract::LiveSessionId>,
        viewer: Option<&crate::contract::LiveViewerInstanceId>,
    ) -> anyhow::Result<Option<Grant>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("live_view_store/queries/stable_owned.surql"))
            .bind(("key", Self::key(id)))
            .bind(("owner", Document(owner.clone())))
            .bind(("actor", actor.to_string()))
            .bind(("live_session", session.map(ToString::to_string)))
            .bind(("viewer", viewer.map(ToString::to_string)))
            .await?
            .check()?;
        let rows: Vec<Grant> = response.take(0)?;
        let row = rows.into_iter().next();
        if let Some(row) = &row {
            row.validate()?;
            anyhow::ensure!(
                &row.state.0.live_view_id == id,
                "stored live view identity disagrees"
            );
        }
        Ok(row)
    }
    pub async fn issuer_grants(&self, issuer: &LiveViewIssuerId) -> anyhow::Result<Vec<Grant>> {
        let mut response = self.store.client().query("SELECT * FROM uav_live_view WHERE issuer = $issuer AND closed_at = NONE LIMIT 10001;")
            .bind(("issuer",issuer.as_uuid())).await?.check()?;
        let rows: Vec<Grant> = response.take(0)?;
        anyhow::ensure!(rows.len() <= 10000, "live view issuer exceeds 10000 grants");
        for row in &rows {
            row.validate()?;
        }
        Ok(rows)
    }
    pub async fn replace(&self, original: &Grant, next: Grant) -> anyhow::Result<bool> {
        next.validate()?;
        let mut response = self
            .store
            .client()
            .query(include_str!("live_view_store/queries/replace.surql"))
            .bind(("key", Self::key(&original.state.0.live_view_id)))
            .bind(("generation", original.generation))
            .bind(("issuer", original.issuer))
            .bind(("grant", next))
            .await?
            .check()?;
        let rows: Vec<Grant> = response.take(0)?;
        Ok(rows.len() == 1)
    }
    pub async fn revoke(
        &self,
        original: &Grant,
        next: Grant,
        draft: veoveo_mcp_contract::audit::AuditDraft,
    ) -> anyhow::Result<bool> {
        next.validate()?;
        let audit = veoveo_platform_store::audit::AuditTransactionWrite::new(
            self.store.audit_targets(),
            draft,
        )?
        .into_binding();
        retry_revoke_transaction(|| async {
            let response = self
                .store
                .client()
                .query(include_str!("live_view_store/queries/revoke.surql"))
                .bind(("key", Self::key(&original.state.0.live_view_id)))
                .bind(("generation", original.generation))
                .bind(("issuer", original.issuer))
                .bind(("grant", next.clone()))
                .bind(audit.clone())
                .await?;
            let mut response = response;
            match veoveo_platform_store::primary_transaction_error(response.take_errors()) {
                Some(error) => Err(error),
                None => Ok(()),
            }
        })
        .await
        .map_err(Into::into)
    }

    pub async fn usage(
        &self,
        session: &crate::contract::SessionId,
        products: &[crate::contract::LiveStreamProductId],
    ) -> anyhow::Result<Vec<ProductUsage>> {
        let mut response = self
            .store
            .client()
            .query(include_str!("live_view_store/queries/usage.surql"))
            .bind(("live_session", session.to_string()))
            .bind((
                "products",
                products.iter().map(ToString::to_string).collect::<Vec<_>>(),
            ))
            .bind(("now", Utc::now()))
            .await?
            .check()?;
        Ok(response.take(0)?)
    }
    pub async fn prune(&self) -> anyhow::Result<()> {
        self.store
            .client()
            .query(include_str!("live_view_store/queries/prune.surql"))
            .bind(("now", Utc::now()))
            .await?
            .check()?;
        Ok(())
    }
}

// Surreal 3.3 confirms this transaction was aborted. The same generation and
// prepared audit identity may be attempted again; other failures are uncertain.
pub(super) fn retryable_transaction_abort(error: &surrealdb::Error) -> bool {
    matches!(
        error.query_details(),
        Some(surrealdb::types::QueryError::TransactionConflict)
    )
}

pub(super) async fn retry_revoke_transaction<F, Fut>(
    mut transaction: F,
) -> Result<bool, surrealdb::Error>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<(), surrealdb::Error>>,
{
    for attempt in 0..3 {
        let error = match transaction().await {
            Ok(()) => return Ok(true),
            Err(error) => error,
        };
        if error.is_thrown() && error.message().contains("uav_live_view_cas_conflict") {
            return Ok(false);
        }
        if attempt < 2 && retryable_transaction_abort(&error) {
            continue;
        }
        return Err(error);
    }
    unreachable!("three transaction attempts return or propagate an error")
}
