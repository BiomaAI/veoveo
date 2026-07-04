use super::*;

pub(super) const CONFORMANCE_KEY_ID: &str = "test-key";
const CLIENT_ASSERTION_TYPE_JWT_BEARER: &str =
    "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
// Public conformance keypair for deterministic local smoke tokens; never deployment material.
const CONFORMANCE_RSA_PRIVATE_KEY_DER_B64: &str = r#"
MIIEpAIBAAKCAQEAvCUS6tGS9/VE3pGzncb1rDsZt/V/LkPHl2QO9jDlaO/jAEdfPOtCSsSyv7dY
+nmY61GpXedIpqg6U7gcU/TcOVar0APPbKZ3OERrvrX9w5/oTJyqK42Lwybl9vmFApcRDIexmSQ8
HBdc1tQPqdkSCHS2csfZVxAQ64PLh48017Q+w8L1UuXYOxD8QdpQx2R1TD3bOiSeaZRs2Utww6rb
ex0/Gn6kkYJw3kr+rQgqmmmOoZuEi7p3qSg6KXvKf3hcfugKQlRIamdP8FOz/3sM2vf2jzUV9BUM
xtOF/yj2GzLmUYHxPtn+K46QDTcGpFyYN6gAPaiGBKkxxZDIaHgosQIDAQABAoIBAAl/bB7tRTht
+ePr8ker2m1PPvc/xgOzgX0BnLU+JuiXGowiLjs8q5graZQeyPe9AXSYpt6CDVN3cNlW1RxCY0ck
OlBqDtOu7BwLrS4/kO/KD9+lNXx1HOn1Odzvv/CPaHmL1JH057Fp1wKTyjYiaoQBg0/USaMY4SfI
e5LsbmgYn71s03MXf9/TgKErBRXiIYPW9aKvpKlfCQ8pGV1/i/rTy+Sj87rk+8+fU+fPVyKUWsjA
gNHm+FmhCPPPVm4qh6Vw/NmuOpfRf1mzfVi7rBq0t5ehHkmW3KVSWY9+v3EttoXjC9iXFIr1OXp5
aoaZZIXpjw3vAlaKwXbuu7lUZhkCgYEA3PGDT2UgWCFjEJjpi2fQzCBfVQC3lgJ8Xwz3EOeNhe+M
mrKb358iDp5o+WgU+S4HJJcGK9uptGgN9GYrf303GPMwmWOvC8xH5fV8WDBYGqMeEi+xFHlS8ymt
MmiWpAkW8/rEjDJama58qzjyEcq+fuW4BJcxOydFHgACSOZIbVkCgYEA2f9RJ7+tOajthShh6LbV
lhSNDjAeauBj5pcg8bZhLaCNWKCUBE2ob+YXvTL6mzx30faY5nutMdJfOI2Au7YqQgx8HeCBkCUi
D5Ngx9yjQ2/vnNQSRjIY2mjj0/tzTlVNGJDxbwUr8DGug8BD6Wz+L1l+s8F3aqAFljp7HLMq8xkC
gYEAsoobgSoH9A+uvPfEKdnPmVRDlS4KLJd/p1OTxz5GV8gXB99zJEa0v7l0vK5F3II8VW4RF5nf
TiCTvj5dwh0OTAQg7qLmDhOauhIg1Cbk20mbADk30IKl7EduZQCtUorh2HB5KY17NxsQNVDEFGqQ
e3zoshT3PITkTnTVY9FrD6kCgYEAwZa5JBpUo6q/Wwu0fuu2mvOfG+VhbbndHY5CBETY4aL9QqI/
L98i4FQt6qeV4zt8kGlz+OIFuQO/6cHHe2rW9haONh4EENTY/Yn8XSAzoBSMbfHqVInyhiq1f6+C
AyM/NryomtW14jTMbFXWOTnANJ4+JTV+baKzs2g1ohP95SkCgYB7RzFmdbiY1ASdGO/vWqc/wLnT
hHID7qgdXU4DP84HMmOX/QG5iV8GtQPTfNJm+m1PEnkg4W24DOqg2gJ3/q7wTROOLwQlJtOmizkC
XVKygdRdax3xMB3Eld5rlIDwzX09ARHrm8badXtrF0NhQPYZVbax8rpJGcgEFPgXEJJ71w==
"#;

#[derive(Debug, Serialize)]
struct ClientAssertionClaims {
    iss: String,
    sub: String,
    aud: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
}

#[derive(Debug, Serialize)]
struct IdJagClaims {
    iss: String,
    sub: String,
    aud: String,
    resource: String,
    client_id: String,
    exp: u64,
    nbf: u64,
    iat: u64,
    jti: String,
    scope: String,
    groups: Vec<String>,
    roles: Vec<String>,
    tenant: String,
    data_labels: Vec<String>,
    principal_assurances: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenEndpointResponse {
    access_token: String,
    token_type: String,
    expires_in: u64,
    scope: String,
}

pub(super) struct ClientAssertionInput {
    pub(super) client_id: String,
    pub(super) audience: String,
    pub(super) jwt_id: Option<String>,
    pub(super) ttl_minutes: i64,
}

pub(super) struct TokenExchangeInput {
    pub(super) token_url: String,
    pub(super) client_assertion: ClientAssertionInput,
    pub(super) resource: Option<String>,
    pub(super) scopes: Vec<String>,
}

pub(super) struct IdJagInput {
    pub(super) issuer: String,
    pub(super) audience: String,
    pub(super) resource: String,
    pub(super) client_id: String,
    pub(super) subject: String,
    pub(super) scopes: Vec<String>,
    pub(super) tenant: String,
    pub(super) groups: Vec<String>,
    pub(super) roles: Vec<String>,
    pub(super) data_labels: Vec<String>,
    pub(super) principal_assurances: Vec<String>,
    pub(super) jwt_id: Option<String>,
    pub(super) ttl_minutes: i64,
}

pub(super) struct IdJagTokenExchangeInput {
    pub(super) token_url: String,
    pub(super) id_jag: IdJagInput,
    pub(super) requested_scopes: Vec<String>,
}

fn build_client_assertion(input: &ClientAssertionInput) -> Result<String> {
    if input.ttl_minutes <= 0 {
        return Err(anyhow!("ttl_minutes must be greater than zero"));
    }
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::minutes(input.ttl_minutes))
        .ok_or_else(|| anyhow!("ttl_minutes produces an invalid expiration timestamp"))?;
    let jwt_id = input
        .jwt_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let claims = ClientAssertionClaims {
        iss: input.client_id.clone(),
        sub: input.client_id.clone(),
        aud: input.audience.clone(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        jti: jwt_id,
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(encode(&header, &claims, &conformance_encoding_key()?)?)
}

pub(super) fn cmd_gateway_jwks() -> Result<()> {
    println!("{}", serde_json::to_string_pretty(&conformance_jwks()?)?);
    Ok(())
}

pub(super) fn cmd_gateway_private_key_der_b64() {
    println!(
        "{}",
        CONFORMANCE_RSA_PRIVATE_KEY_DER_B64
            .lines()
            .collect::<String>()
    );
}

pub(super) fn cmd_gateway_client_assertion(input: ClientAssertionInput) -> Result<()> {
    println!("{}", build_client_assertion(&input)?);
    Ok(())
}

pub(super) async fn cmd_gateway_token_exchange(input: TokenExchangeInput) -> Result<()> {
    if input.scopes.is_empty() {
        return Err(anyhow!("at least one --scope is required"));
    }
    let assertion = build_client_assertion(&input.client_assertion)?;
    let scope = input.scopes.join(" ");
    let client_id = input.client_assertion.client_id.clone();
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("grant_type", "client_credentials")
        .append_pair("client_id", &client_id)
        .append_pair("scope", &scope)
        .append_pair("client_assertion_type", CLIENT_ASSERTION_TYPE_JWT_BEARER)
        .append_pair("client_assertion", &assertion);
    if let Some(resource) = &input.resource {
        serializer.append_pair("resource", resource);
    }
    let form_body = serializer.finish();
    let response = reqwest::Client::new()
        .post(&input.token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(form_body)
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!("token endpoint returned {status}: {body}"));
    }
    let token_response: TokenEndpointResponse = serde_json::from_str(&body)?;
    if token_response.token_type != "Bearer" {
        return Err(anyhow!(
            "token endpoint returned token_type `{}`",
            token_response.token_type
        ));
    }
    if token_response.access_token.is_empty() {
        return Err(anyhow!("token endpoint returned an empty access_token"));
    }
    if token_response.expires_in == 0 {
        return Err(anyhow!("token endpoint returned expires_in=0"));
    }
    if token_response.scope.is_empty() {
        return Err(anyhow!("token endpoint returned an empty scope"));
    }
    println!("{}", token_response.access_token);
    Ok(())
}

fn build_id_jag(input: &IdJagInput) -> Result<String> {
    if input.ttl_minutes <= 0 {
        return Err(anyhow!("ttl_minutes must be greater than zero"));
    }
    if input.scopes.is_empty() {
        return Err(anyhow!("at least one ID-JAG scope is required"));
    }
    let now = Utc::now();
    let expires_at = now
        .checked_add_signed(TimeDelta::minutes(input.ttl_minutes))
        .ok_or_else(|| anyhow!("ttl_minutes produces an invalid expiration timestamp"))?;
    let jwt_id = input
        .jwt_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let claims = IdJagClaims {
        iss: input.issuer.clone(),
        sub: input.subject.clone(),
        aud: input.audience.clone(),
        resource: input.resource.clone(),
        client_id: input.client_id.clone(),
        exp: unix_seconds(expires_at.timestamp())?,
        nbf: unix_seconds(now.timestamp())?,
        iat: unix_seconds(now.timestamp())?,
        jti: jwt_id,
        scope: input.scopes.join(" "),
        groups: input.groups.clone(),
        roles: input.roles.clone(),
        tenant: input.tenant.clone(),
        data_labels: input.data_labels.clone(),
        principal_assurances: input.principal_assurances.clone(),
    };
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(encode(&header, &claims, &conformance_encoding_key()?)?)
}

pub(super) fn cmd_gateway_id_jag(input: IdJagInput) -> Result<()> {
    println!("{}", build_id_jag(&input)?);
    Ok(())
}

pub(super) async fn cmd_gateway_id_jag_token_exchange(
    input: IdJagTokenExchangeInput,
) -> Result<()> {
    let assertion = build_id_jag(&input.id_jag)?;
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer
        .append_pair("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer")
        .append_pair("client_id", &input.id_jag.client_id)
        .append_pair("assertion", &assertion)
        .append_pair("resource", &input.id_jag.resource);
    if !input.requested_scopes.is_empty() {
        serializer.append_pair("scope", &input.requested_scopes.join(" "));
    }
    let response = reqwest::Client::new()
        .post(&input.token_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .body(serializer.finish())
        .send()
        .await?;
    let status = response.status();
    let body = response.text().await?;
    if !status.is_success() {
        return Err(anyhow!("token endpoint returned {status}: {body}"));
    }
    let token_response: TokenEndpointResponse = serde_json::from_str(&body)?;
    if token_response.token_type != "Bearer" {
        return Err(anyhow!(
            "token endpoint returned token_type `{}`",
            token_response.token_type
        ));
    }
    if token_response.access_token.is_empty() {
        return Err(anyhow!("token endpoint returned an empty access_token"));
    }
    if token_response.expires_in == 0 {
        return Err(anyhow!("token endpoint returned expires_in=0"));
    }
    if token_response.scope.is_empty() {
        return Err(anyhow!("token endpoint returned an empty scope"));
    }
    println!("{}", token_response.access_token);
    Ok(())
}

pub(super) fn conformance_jwks() -> Result<JwkSet> {
    let mut jwk = Jwk::from_encoding_key(&conformance_encoding_key()?, Algorithm::RS256)?;
    jwk.common.key_id = Some(CONFORMANCE_KEY_ID.to_string());
    Ok(JwkSet { keys: vec![jwk] })
}

pub(super) fn conformance_encoding_key() -> Result<EncodingKey> {
    let der_text = CONFORMANCE_RSA_PRIVATE_KEY_DER_B64
        .lines()
        .collect::<String>();
    let der = BASE64_STANDARD.decode(der_text)?;
    Ok(EncodingKey::from_rsa_der(&der))
}

pub(super) fn unix_seconds(value: i64) -> Result<u64> {
    u64::try_from(value).map_err(|_| anyhow!("timestamp before Unix epoch"))
}
