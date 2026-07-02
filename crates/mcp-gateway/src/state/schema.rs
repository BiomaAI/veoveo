use anyhow::Result;

use super::GatewayState;

impl GatewayState {
    pub(super) fn initialize(&self) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS gateway_task_mappings (
                gateway_task_id TEXT PRIMARY KEY,
                upstream_server TEXT NOT NULL,
                upstream_task_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                owner TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL,
                mapping_json TEXT NOT NULL
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_gateway_task_mappings_upstream
            ON gateway_task_mappings(upstream_server, upstream_task_id);

            CREATE INDEX IF NOT EXISTS idx_gateway_task_mappings_owner
            ON gateway_task_mappings(profile, owner, upstream_server);

            CREATE TABLE IF NOT EXISTS gateway_resource_subscriptions (
                profile TEXT NOT NULL,
                owner TEXT NOT NULL,
                upstream_server TEXT NOT NULL,
                resource_uri TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                updated_at TIMESTAMP NOT NULL,
                subscription_json TEXT NOT NULL,
                PRIMARY KEY(profile, owner, upstream_server, resource_uri)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_resource_subscriptions_owner
            ON gateway_resource_subscriptions(profile, owner, upstream_server);

            CREATE TABLE IF NOT EXISTS gateway_revoked_jwt_ids (
                profile TEXT NOT NULL,
                issuer TEXT NOT NULL,
                jwt_id TEXT NOT NULL,
                revoked_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                revocation_json TEXT NOT NULL,
                PRIMARY KEY(profile, issuer, jwt_id)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_revoked_jwt_ids_expires
            ON gateway_revoked_jwt_ids(expires_at);

            CREATE TABLE IF NOT EXISTS gateway_client_assertion_jtis (
                authorization_server TEXT NOT NULL,
                client_id TEXT NOT NULL,
                jwt_id TEXT NOT NULL,
                seen_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                PRIMARY KEY(authorization_server, client_id, jwt_id)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_client_assertion_jtis_expires
            ON gateway_client_assertion_jtis(expires_at);

            CREATE TABLE IF NOT EXISTS gateway_id_jag_jtis (
                authorization_server TEXT NOT NULL,
                client_id TEXT NOT NULL,
                jwt_id TEXT NOT NULL,
                seen_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                PRIMARY KEY(authorization_server, client_id, jwt_id)
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_id_jag_jtis_expires
            ON gateway_id_jag_jtis(expires_at);

            CREATE TABLE IF NOT EXISTS gateway_authorization_requests (
                idp_state TEXT PRIMARY KEY,
                profile TEXT NOT NULL,
                oauth_client_id TEXT NOT NULL,
                oidc_client TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                created_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                request_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_requests_expires
            ON gateway_authorization_requests(expires_at);

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_requests_client
            ON gateway_authorization_requests(profile, oauth_client_id);

            CREATE TABLE IF NOT EXISTS gateway_authorization_codes (
                code TEXT PRIMARY KEY,
                profile TEXT NOT NULL,
                oauth_client_id TEXT NOT NULL,
                oidc_client TEXT NOT NULL,
                principal TEXT NOT NULL,
                redirect_uri TEXT NOT NULL,
                issued_at TIMESTAMP NOT NULL,
                expires_at TIMESTAMP NOT NULL,
                consumed_at TIMESTAMP,
                code_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_codes_expires
            ON gateway_authorization_codes(expires_at);

            CREATE INDEX IF NOT EXISTS idx_gateway_authorization_codes_client
            ON gateway_authorization_codes(profile, oauth_client_id, principal);

            CREATE TABLE IF NOT EXISTS gateway_control_plane_revisions (
                revision_id TEXT PRIMARY KEY,
                sha256 TEXT NOT NULL,
                source TEXT NOT NULL,
                applied_at TIMESTAMP NOT NULL,
                applied_by TEXT NOT NULL,
                tenant TEXT,
                revision_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_revisions_applied
            ON gateway_control_plane_revisions(applied_at);

            CREATE INDEX IF NOT EXISTS idx_gateway_control_plane_revisions_sha256
            ON gateway_control_plane_revisions(sha256);

            CREATE TABLE IF NOT EXISTS gateway_audit_events (
                event_id TEXT PRIMARY KEY,
                trace_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                method TEXT NOT NULL,
                action TEXT NOT NULL,
                principal TEXT,
                tenant TEXT,
                token_issuer TEXT,
                timestamp TIMESTAMP NOT NULL,
                latency_ms UBIGINT,
                target_json TEXT NOT NULL,
                decision_json TEXT NOT NULL,
                event_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_audit_profile_time
            ON gateway_audit_events(profile, timestamp);

            CREATE INDEX IF NOT EXISTS idx_gateway_audit_principal_time
            ON gateway_audit_events(principal, timestamp);

            CREATE TABLE IF NOT EXISTS gateway_auth_audit_events (
                event_id TEXT PRIMARY KEY,
                trace_id TEXT NOT NULL,
                profile TEXT NOT NULL,
                protected_resource TEXT NOT NULL,
                outcome TEXT NOT NULL,
                reason TEXT NOT NULL,
                method TEXT NOT NULL,
                principal TEXT,
                tenant TEXT,
                token_issuer TEXT,
                token_subject TEXT,
                jwt_id TEXT,
                timestamp TIMESTAMP NOT NULL,
                latency_ms UBIGINT,
                event_json TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_gateway_auth_audit_profile_time
            ON gateway_auth_audit_events(profile, timestamp);

            CREATE INDEX IF NOT EXISTS idx_gateway_auth_audit_principal_time
            ON gateway_auth_audit_events(principal, timestamp);
            "#,
        )?;
        Ok(())
    }
}
