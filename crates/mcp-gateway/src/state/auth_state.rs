use anyhow::Result;
use chrono::{DateTime, Utc};
use duckdb::{OptionalExt, params};
use veoveo_mcp_contract::{
    AuthorizationServerId, GatewayAuthorizationCodeRecord, GatewayAuthorizationRequest,
    GatewayJwtRevocation, GatewayProfileId, JwtId, OAuthAuthorizationCode, OAuthClientId,
    OAuthStateValue, TokenIssuer,
};

use super::GatewayState;

impl GatewayState {
    pub fn record_jwt_revocation(&self, revocation: &GatewayJwtRevocation) -> Result<()> {
        let revocation_json = serde_json::to_string(revocation)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO gateway_revoked_jwt_ids (
                profile, issuer, jwt_id, revoked_at, expires_at, revocation_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(profile, issuer, jwt_id) DO UPDATE SET
                revoked_at = excluded.revoked_at,
                expires_at = excluded.expires_at,
                revocation_json = excluded.revocation_json
            "#,
            params![
                revocation.profile.as_str(),
                revocation.issuer.as_str(),
                revocation.jwt_id.as_str(),
                revocation.revoked_at,
                revocation.expires_at,
                revocation_json,
            ],
        )?;
        Ok(())
    }

    pub fn jwt_revocation(
        &self,
        profile: &GatewayProfileId,
        issuer: &TokenIssuer,
        jwt_id: &JwtId,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayJwtRevocation>> {
        self.query_revocation(
            r#"
            SELECT revocation_json
            FROM gateway_revoked_jwt_ids
            WHERE profile = ?1 AND issuer = ?2 AND jwt_id = ?3 AND expires_at > ?4
            "#,
            params![profile.as_str(), issuer.as_str(), jwt_id.as_str(), now],
        )
    }

    pub fn prune_expired_jwt_revocations(&self, now: DateTime<Utc>) -> Result<u64> {
        let conn = self.conn.lock();
        let deleted = conn.execute(
            "DELETE FROM gateway_revoked_jwt_ids WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(u64::try_from(deleted)?)
    }

    pub fn record_client_assertion_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM gateway_client_assertion_jtis WHERE expires_at <= ?1",
            params![now],
        )?;
        let existing: Option<DateTime<Utc>> = conn
            .query_row(
                r#"
                SELECT expires_at
                FROM gateway_client_assertion_jtis
                WHERE authorization_server = ?1 AND client_id = ?2 AND jwt_id = ?3
                "#,
                params![
                    authorization_server.as_str(),
                    client_id.as_str(),
                    jwt_id.as_str()
                ],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some_and(|existing_expires_at| existing_expires_at > now) {
            return Ok(false);
        }
        conn.execute(
            r#"
            INSERT INTO gateway_client_assertion_jtis (
                authorization_server, client_id, jwt_id, seen_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(authorization_server, client_id, jwt_id) DO UPDATE SET
                seen_at = excluded.seen_at,
                expires_at = excluded.expires_at
            "#,
            params![
                authorization_server.as_str(),
                client_id.as_str(),
                jwt_id.as_str(),
                now,
                expires_at,
            ],
        )?;
        Ok(true)
    }

    pub fn record_id_jag_jti(
        &self,
        authorization_server: &AuthorizationServerId,
        client_id: &OAuthClientId,
        jwt_id: &JwtId,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM gateway_id_jag_jtis WHERE expires_at <= ?1",
            params![now],
        )?;
        let existing: Option<DateTime<Utc>> = conn
            .query_row(
                r#"
                SELECT expires_at
                FROM gateway_id_jag_jtis
                WHERE authorization_server = ?1 AND client_id = ?2 AND jwt_id = ?3
                "#,
                params![
                    authorization_server.as_str(),
                    client_id.as_str(),
                    jwt_id.as_str()
                ],
                |row| row.get(0),
            )
            .optional()?;
        if existing.is_some_and(|existing_expires_at| existing_expires_at > now) {
            return Ok(false);
        }
        conn.execute(
            r#"
            INSERT INTO gateway_id_jag_jtis (
                authorization_server, client_id, jwt_id, seen_at, expires_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(authorization_server, client_id, jwt_id) DO UPDATE SET
                seen_at = excluded.seen_at,
                expires_at = excluded.expires_at
            "#,
            params![
                authorization_server.as_str(),
                client_id.as_str(),
                jwt_id.as_str(),
                now,
                expires_at,
            ],
        )?;
        Ok(true)
    }

    pub fn record_authorization_request(
        &self,
        request: &GatewayAuthorizationRequest,
    ) -> Result<()> {
        let request_json = serde_json::to_string(request)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO gateway_authorization_requests (
                idp_state, profile, oauth_client_id, oidc_client, redirect_uri,
                created_at, expires_at, request_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            "#,
            params![
                request.idp_state.as_str(),
                request.profile.as_str(),
                request.oauth_client_id.as_str(),
                request.oidc_client.as_str(),
                request.redirect_uri.as_str(),
                request.created_at,
                request.expires_at,
                request_json,
            ],
        )?;
        Ok(())
    }

    pub fn consume_authorization_request(
        &self,
        idp_state: &OAuthStateValue,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationRequest>> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM gateway_authorization_requests WHERE expires_at <= ?1",
            params![now],
        )?;
        let request_json = conn
            .query_row(
                r#"
                SELECT request_json
                FROM gateway_authorization_requests
                WHERE idp_state = ?1 AND expires_at > ?2
                "#,
                params![idp_state.as_str(), now],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(request_json) = request_json else {
            return Ok(None);
        };
        conn.execute(
            "DELETE FROM gateway_authorization_requests WHERE idp_state = ?1",
            params![idp_state.as_str()],
        )?;
        Ok(Some(serde_json::from_str(&request_json)?))
    }

    pub fn record_authorization_code(&self, code: &GatewayAuthorizationCodeRecord) -> Result<()> {
        let code_json = serde_json::to_string(code)?;
        let conn = self.conn.lock();
        conn.execute(
            r#"
            INSERT INTO gateway_authorization_codes (
                code, profile, oauth_client_id, oidc_client, principal, redirect_uri,
                issued_at, expires_at, consumed_at, code_json
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                code.code.as_str(),
                code.profile.as_str(),
                code.oauth_client_id.as_str(),
                code.oidc_client.as_str(),
                code.principal.id.as_str(),
                code.redirect_uri.as_str(),
                code.issued_at,
                code.expires_at,
                code.consumed_at,
                code_json,
            ],
        )?;
        Ok(())
    }

    pub fn consume_authorization_code(
        &self,
        code: &OAuthAuthorizationCode,
        now: DateTime<Utc>,
    ) -> Result<Option<GatewayAuthorizationCodeRecord>> {
        let conn = self.conn.lock();
        conn.execute(
            "DELETE FROM gateway_authorization_codes WHERE expires_at <= ?1",
            params![now],
        )?;
        let code_json = conn
            .query_row(
                r#"
                SELECT code_json
                FROM gateway_authorization_codes
                WHERE code = ?1 AND expires_at > ?2 AND consumed_at IS NULL
                "#,
                params![code.as_str(), now],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(code_json) = code_json else {
            return Ok(None);
        };
        let mut record: GatewayAuthorizationCodeRecord = serde_json::from_str(&code_json)?;
        record.consumed_at = Some(now);
        let updated_json = serde_json::to_string(&record)?;
        let updated = conn.execute(
            r#"
            UPDATE gateway_authorization_codes
            SET consumed_at = ?2, code_json = ?3
            WHERE code = ?1 AND expires_at > ?2 AND consumed_at IS NULL
            "#,
            params![code.as_str(), now, updated_json],
        )?;
        if updated == 0 {
            return Ok(None);
        }
        Ok(Some(record))
    }

    pub fn prune_expired_authorization_records(&self, now: DateTime<Utc>) -> Result<u64> {
        let conn = self.conn.lock();
        let requests = conn.execute(
            "DELETE FROM gateway_authorization_requests WHERE expires_at <= ?1",
            params![now],
        )?;
        let codes = conn.execute(
            "DELETE FROM gateway_authorization_codes WHERE expires_at <= ?1",
            params![now],
        )?;
        Ok(u64::try_from(requests + codes)?)
    }

    fn query_revocation<P>(&self, sql: &str, params: P) -> Result<Option<GatewayJwtRevocation>>
    where
        P: duckdb::Params,
    {
        let conn = self.conn.lock();
        let revocation_json = conn
            .query_row(sql, params, |row| row.get::<_, String>(0))
            .optional()?;
        Ok(revocation_json
            .map(|json| serde_json::from_str(&json))
            .transpose()?)
    }
}
