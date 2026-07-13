use super::*;

pub(crate) async fn wait_for_file_and_http(file: &Path, url: &str) -> Result<()> {
    for _ in 0..150 {
        if file.exists() && http_ok(url).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url} and {}", file.display());
}

pub(crate) async fn wait_for_file(file: &Path) -> Result<()> {
    for _ in 0..150 {
        if file.exists() {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {}", file.display());
}

pub(crate) async fn wait_for_file_text(file: &Path, expected: &str) -> Result<()> {
    for _ in 0..100 {
        if let Ok(contents) = fs::read_to_string(file)
            && contents.contains(expected)
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let contents = fs::read_to_string(file).unwrap_or_default();
    bail!(
        "timed out waiting for `{expected}` in {}\ncontents:\n{contents}",
        file.display()
    );
}

pub(crate) async fn wait_for_file_contains(file: &Path, first: &str, second: &str) -> Result<()> {
    for _ in 0..80 {
        if let Ok(contents) = fs::read_to_string(file)
            && contents
                .lines()
                .any(|line| line.starts_with(first.trim_end()))
            && contents
                .lines()
                .any(|line| line.starts_with(second.trim_end()))
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let contents = fs::read_to_string(file).unwrap_or_default();
    bail!(
        "timed out waiting for `{first}` and `{second}` in {}\ncontents:\n{contents}",
        file.display()
    );
}

pub(crate) async fn wait_for_http(url: &str) -> Result<()> {
    for _ in 0..150 {
        if http_ok(url).await? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url}");
}

pub(crate) async fn wait_for_http_client(
    client: &reqwest::Client,
    url: &str,
    expected: StatusCode,
) -> Result<()> {
    for _ in 0..150 {
        if let Ok(response) = client.get(url).send().await
            && response.status() == expected
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    bail!("timed out waiting for {url} to return {expected}");
}

pub(crate) async fn http_ok(url: &str) -> Result<bool> {
    let response = reqwest::get(url).await;
    Ok(matches!(response, Ok(response) if response.status() == StatusCode::OK))
}

pub(crate) async fn assert_http_status(url: &str, expected: StatusCode) -> Result<()> {
    let status = reqwest::get(url).await?.status();
    if status == expected {
        Ok(())
    } else {
        bail!("expected {expected} from {url}, got {status}");
    }
}

pub(crate) async fn assert_http_get_status(
    url: &str,
    bearer_token: Option<&str>,
    expected: StatusCode,
) -> Result<()> {
    let client = reqwest::Client::new();
    let mut request = client.get(url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let status = request.send().await?.status();
    if status == expected {
        Ok(())
    } else {
        bail!("expected GET {url} to return {expected}, got {status}");
    }
}

pub(crate) async fn assert_ready_profiles(gateway_base: &str, expected: u64) -> Result<()> {
    let ready: Value = reqwest::get(format!("{gateway_base}/readyz"))
        .await?
        .error_for_status()?
        .json()
        .await?;
    if ready.get("profiles").and_then(Value::as_u64) == Some(expected) {
        Ok(())
    } else {
        bail!("gateway readyz did not report {expected} profile(s): {ready}");
    }
}

pub(crate) fn https_client_with_ca(cert_path: &Path) -> Result<reqwest::Client> {
    let cert = reqwest::Certificate::from_pem(&fs::read(cert_path)?)?;
    Ok(reqwest::Client::builder()
        .add_root_certificate(cert)
        .redirect(Policy::none())
        .build()?)
}

pub(crate) fn redirect_location(
    response: reqwest::Response,
    expected: StatusCode,
) -> Result<String> {
    let status = response.status();
    if status != expected {
        bail!("expected redirect status {expected}, got {status}");
    }
    let location = response
        .headers()
        .get(LOCATION)
        .ok_or_else(|| anyhow!("redirect response had no Location header"))?
        .to_str()?
        .to_string();
    Ok(location)
}

pub(crate) struct GatewayBrowserAuthorization<'a> {
    pub gateway_base: &'a str,
    pub idp_base: &'a str,
    pub client_id: &'a str,
    pub redirect_uri: &'a str,
    pub code_challenge: &'a str,
    pub client_state: &'a str,
}

pub(crate) async fn gateway_browser_authorization_code(
    http: &reqwest::Client,
    idp_client: &reqwest::Client,
    authorization: GatewayBrowserAuthorization<'_>,
) -> Result<(String, String)> {
    let GatewayBrowserAuthorization {
        gateway_base,
        idp_base,
        client_id,
        redirect_uri,
        code_challenge,
        client_state,
    } = authorization;
    let operator_resource = format!("{PUBLIC_BASE_URL}/mcp/operator");
    let authorize_query = form_urlencoded(&[
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("scope", "operator:use"),
        ("resource", &operator_resource),
        ("code_challenge", code_challenge),
        ("code_challenge_method", "S256"),
        ("state", client_state),
    ]);
    let authorize = http
        .get(format!("{gateway_base}/oauth/authorize?{authorize_query}"))
        .send()
        .await?;
    let authorize_location = redirect_location(authorize, StatusCode::FOUND)?;
    if !authorize_location.starts_with(&format!("{idp_base}/oauth2/authorize")) {
        bail!("unexpected authorize redirect: {authorize_location}");
    }

    let idp_authorize = idp_client.get(&authorize_location).send().await?;
    let idp_callback = redirect_location(idp_authorize, StatusCode::FOUND)?;
    if !idp_callback.starts_with(&format!("{PUBLIC_BASE_URL}/oauth/callback")) {
        bail!("unexpected IdP callback redirect: {idp_callback}");
    }
    let callback_query = idp_callback
        .split_once('?')
        .map(|(_, query)| query.to_string())
        .ok_or_else(|| anyhow!("IdP callback had no query string: {idp_callback}"))?;
    let gateway_callback = http
        .get(format!("{gateway_base}/oauth/callback?{callback_query}"))
        .send()
        .await?;
    let client_redirect = redirect_location(gateway_callback, StatusCode::FOUND)?;
    if !client_redirect.starts_with(redirect_uri) {
        bail!("unexpected browser client redirect: {client_redirect}");
    }
    let gateway_code = url_query_value(&client_redirect, "code")?;
    Ok((gateway_code, callback_query))
}

pub(crate) fn url_query_value(url: &str, key: &str) -> Result<String> {
    let url = reqwest::Url::parse(url)?;
    url.query_pairs()
        .find_map(|(query_key, value)| (query_key == key).then(|| value.into_owned()))
        .ok_or_else(|| anyhow!("URL had no `{key}` query value: {url}"))
}

pub(crate) fn form_urlencoded(fields: &[(&str, &str)]) -> String {
    let mut serializer = url::form_urlencoded::Serializer::new(String::new());
    serializer.extend_pairs(fields.iter().copied());
    serializer.finish()
}

pub(crate) async fn post_json(
    client: &reqwest::Client,
    url: &str,
    bearer_token: Option<&str>,
    body: Value,
) -> Result<Value> {
    let mut request = client.post(url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    if !body.is_null() {
        request = request.json(&body);
    }
    Ok(request.send().await?.error_for_status()?.json().await?)
}

pub(crate) async fn get_json(
    client: &reqwest::Client,
    url: &str,
    bearer_token: Option<&str>,
) -> Result<Value> {
    let mut request = client.get(url);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    Ok(request.send().await?.error_for_status()?.json().await?)
}

pub(crate) async fn put_json_file(
    client: &reqwest::Client,
    url: &str,
    bearer_token: Option<&str>,
    path: &Path,
) -> Result<Value> {
    let mut request = client
        .put(url)
        .header(CONTENT_TYPE, "application/json")
        .body(fs::read(path)?);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    Ok(request.send().await?.error_for_status()?.json().await?)
}
