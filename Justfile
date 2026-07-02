set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := true

compose := "docker compose -f compose.yaml -f compose.tunnel.yaml --profile dev --profile tunnel"
mcp-url := "http://localhost:8788/mcp/default"
gateway-token-url := "http://localhost:8788/oauth/default/token"
gateway-admin-url := "http://localhost:8788/admin"
gateway-control-plane := "configs/gateway.local.json"
gateway-smoke-control-plane := "configs/gateway.smoke.json"
conformance := "cargo run -p veoveo-mcp-contract --bin conformance --"
default-model := "openai/gpt-image-2/edit"
default-input-image := "gol-real-roblox.jpeg"

# List available recipes.
default:
    @just --list

# Format all Rust crates.
fmt:
    cargo fmt --all

# Run all workspace tests.
test:
    cargo test --workspace

# Format-check and test the workspace.
check:
    cargo fmt --all --check
    cargo test --workspace

# Validate the typed gateway control plane.
gateway-validate:
    cargo run -p veoveo-mcp-gateway --bin gateway -- validate --control-plane {{gateway-control-plane}}

# Validate typed self-hosted deployment profiles.
deployments-validate:
    {{conformance}} deployment-validate --file configs/deployments.json

# Revoke one gateway JWT id until its original token expiration.
gateway-revoke-jwt jwt_id expires_at issuer='https://veoveo.bioma.ai/oauth/default' profile='default' reason='operator_request':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use --scope gateway:admin)"; payload="$(jq -n --arg issuer '{{issuer}}' --arg jwt_id '{{jwt_id}}' --arg expires_at '{{expires_at}}' --arg reason '{{reason}}' '{issuer: $issuer, jwt_id: $jwt_id, expires_at: $expires_at, reason: $reason}')"; curl -fsS -X POST -H "Authorization: Bearer ${token}" -H "Content-Type: application/json" --data "${payload}" "{{gateway-admin-url}}/{{profile}}/jwt-revocations"

# Remove expired gateway JWT revocation entries.
gateway-prune-revoked-jwts profile='default':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use --scope gateway:admin)"; curl -fsS -X POST -H "Authorization: Bearer ${token}" "{{gateway-admin-url}}/{{profile}}/jwt-revocations/prune"

# Smoke-test gateway contract/control-plane behavior without external services.
smoke-gateway:
    cargo test -p veoveo-mcp-contract -p veoveo-mcp-gateway
    just gateway-validate
    just deployments-validate
    cargo run -p veoveo-mcp-gateway --bin gateway -- validate --control-plane {{gateway-smoke-control-plane}}
    just smoke-gateway-http
    just smoke-otel
    just smoke-media-mcp-auth
    just smoke-media-task-run
    just smoke-gateway-authenticated
    just smoke-gateway-task-run

# Smoke-test the gateway HTTP boundary and auth challenge.
smoke-gateway-http:
    #!/usr/bin/env bash
    set -euo pipefail
    port=18799
    base="http://127.0.0.1:${port}"
    idp_port=18803
    idp_base="https://127.0.0.1:${idp_port}"
    tmpdir="$(mktemp -d)"
    log="${tmpdir}/gateway.log"
    idp_log="${tmpdir}/idp.log"
    state_db="${tmpdir}/state.duckdb"
    control_plane="${tmpdir}/gateway.smoke.json"
    idp_cert="${tmpdir}/idp-cert.pem"
    idp_key="${tmpdir}/idp-key.pem"
    idp_ready="${tmpdir}/idp.ready"
    internal_secret="local-smoke-internal-token-secret-32-bytes-minimum"
    oidc_secret="local-smoke-oidc-client-secret"
    auth_private_key="$({{conformance}} gateway-private-key-der-b64)"
    pid=""
    idp_pid=""
    cleanup() {
        if [ -n "${pid}" ]; then
            kill "${pid}" 2>/dev/null || true
            wait "${pid}" 2>/dev/null || true
        fi
        if [ -n "${idp_pid}" ]; then
            kill "${idp_pid}" 2>/dev/null || true
            wait "${idp_pid}" 2>/dev/null || true
        fi
        rm -rf "${tmpdir}"
    }
    VEOVEO_IDP_OIDC_CLIENT_SECRET="${oidc_secret}" {{conformance}} gateway-fake-oidc-idp --port "${idp_port}" --cert-pem "${idp_cert}" --key-pem "${idp_key}" --ready-file "${idp_ready}" >"${idp_log}" 2>&1 &
    idp_pid=$!
    trap cleanup EXIT
    for _ in {1..150}; do
        if [ -f "${idp_ready}" ] && curl --cacert "${idp_cert}" -fsS "${idp_base}/.well-known/jwks.json" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl --cacert "${idp_cert}" -fsS "${idp_base}/.well-known/jwks.json" | grep -q -F '"kid":"test-key"'
    {{conformance}} gateway-smoke-control-plane --base {{gateway-smoke-control-plane}} --output "${control_plane}" --idp-base-url "${idp_base}" --trusted-ca-path "${idp_cert}"
    VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" VEOVEO_IDP_OIDC_CLIENT_SECRET="${oidc_secret}" VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64="${auth_private_key}" cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${port}" --public-base-url https://veoveo.bioma.ai --control-plane "${control_plane}" --state-db "${state_db}" >"${log}" 2>&1 &
    pid=$!
    for _ in {1..150}; do
        if curl -fsS "${base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${base}/readyz" | grep -F '"profiles":1'
    grep -E '^\{' "${log}" | jq -e 'select(.message == "listening" and .service == "veoveo-mcp-gateway" and .server_count == 1 and .profile_count == 1)' >/dev/null
    grep -E '^\{' "${log}" | jq -e 'select(.message == "gateway retention gc completed")' >/dev/null
    {{conformance}} --url "${base}/mcp/default" auth-discovery \
        --metadata-url "${base}/.well-known/oauth-protected-resource/mcp/default" \
        --authorization-server-metadata-url "${base}/.well-known/oauth-authorization-server/oauth/default" \
        --authorization-server-jwks-url "${base}/oauth/default/jwks.json" \
        --required-scope media:use \
        --required-extension io.modelcontextprotocol/enterprise-managed-authorization \
        --required-extension io.modelcontextprotocol/oauth-client-credentials \
        --required-jwks-key-id test-key \
        --required-grant-type authorization_code \
        --required-grant-type client_credentials \
        --required-grant-type urn:ietf:params:oauth:grant-type:jwt-bearer \
        --required-grant-profile urn:ietf:params:oauth:grant-profile:id-jag \
        --required-token-auth-method none \
        --required-token-auth-method private_key_jwt
    code_verifier="smoke-browser-pkce-verifier-0123456789abcdef0123456789abcdef"
    code_challenge="X9AgXux1PHu8RKlqHF9FuDYoLL6yjPFGS5je8BbaBF8"
    authorize_result="$(curl -sS -o /dev/null -w "%{http_code} %{redirect_url}" "${base}/oauth/default/authorize?response_type=code&client_id=veoveo-browser&redirect_uri=https%3A%2F%2Fveoveo.bioma.ai%2Foauth%2Fcallback&scope=media%3Ause&code_challenge=${code_challenge}&code_challenge_method=S256&state=smoke-state")"
    test "${authorize_result%% *}" = "302"
    authorize_location="${authorize_result#* }"
    case "${authorize_location}" in
        "${idp_base}"/oauth2/authorize*) ;;
        *) echo "unexpected authorize redirect: ${authorize_location}" >&2; exit 1 ;;
    esac
    idp_result="$(curl --cacert "${idp_cert}" -sS -o /dev/null -w "%{http_code} %{redirect_url}" "${authorize_location}")"
    test "${idp_result%% *}" = "302"
    idp_callback="${idp_result#* }"
    case "${idp_callback}" in
        https://veoveo.bioma.ai/oauth/default/callback*) ;;
        *) echo "unexpected IdP callback redirect: ${idp_callback}" >&2; exit 1 ;;
    esac
    callback_query="${idp_callback#*\?}"
    callback_result="$(curl -sS -o /dev/null -w "%{http_code} %{redirect_url}" "${base}/oauth/default/callback?${callback_query}")"
    test "${callback_result%% *}" = "302"
    client_redirect="${callback_result#* }"
    case "${client_redirect}" in
        https://veoveo.bioma.ai/oauth/callback*) ;;
        *) echo "unexpected browser client redirect: ${client_redirect}" >&2; exit 1 ;;
    esac
    gateway_code="$(printf '%s\n' "${client_redirect}" | sed -n 's/.*[?&]code=\([^&]*\).*/\1/p')"
    test -n "${gateway_code}"
    token_response="$(curl -fsS -X POST "${base}/oauth/default/token" \
        --data-urlencode grant_type=authorization_code \
        --data-urlencode client_id=veoveo-browser \
        --data-urlencode code="${gateway_code}" \
        --data-urlencode redirect_uri=https://veoveo.bioma.ai/oauth/callback \
        --data-urlencode code_verifier="${code_verifier}")"
    printf '%s' "${token_response}" | grep -q -F '"token_type":"Bearer"'
    replay_status="$(curl -sS -o /dev/null -w "%{http_code}" -X POST "${base}/oauth/default/token" \
        --data-urlencode grant_type=authorization_code \
        --data-urlencode client_id=veoveo-browser \
        --data-urlencode code="${gateway_code}" \
        --data-urlencode redirect_uri=https://veoveo.bioma.ai/oauth/callback \
        --data-urlencode code_verifier="${code_verifier}")"
    test "${replay_status}" = "400"
    callback_replay_status="$(curl -sS -o /dev/null -w "%{http_code}" "${base}/oauth/default/callback?${callback_query}")"
    test "${callback_replay_status}" = "400"
    status="$(curl -sS -o /dev/null -w "%{http_code}" -X POST "${base}/admin/default/reload-control-plane")"
    test "${status}" = "401"
    admin_token="$({{conformance}} gateway-token-exchange --token-url "${base}/oauth/default/token" --scope media:use --scope gateway:admin)"
    revocation_payload="$(jq -n --arg issuer 'https://veoveo.bioma.ai/oauth/default' --arg jwt_id 'smoke-jwt' --arg expires_at '2999-01-01T00:00:00Z' --arg reason 'smoke' '{issuer: $issuer, jwt_id: $jwt_id, expires_at: $expires_at, reason: $reason}')"
    revocation_result="$(curl -fsS -X POST -H "Authorization: Bearer ${admin_token}" -H "Content-Type: application/json" --data "${revocation_payload}" "${base}/admin/default/jwt-revocations")"
    echo "${revocation_result}" | jq -e '.status == "revoked" and .revocation.jwt_id == "smoke-jwt"' >/dev/null
    prune_result="$(curl -fsS -X POST -H "Authorization: Bearer ${admin_token}" "${base}/admin/default/jwt-revocations/prune")"
    echo "${prune_result}" | jq -e '.status == "pruned" and .deleted == 0' >/dev/null
    kill "${pid}"
    wait "${pid}" 2>/dev/null || true
    pid=""
    audit_counts="$(cargo run -q -p veoveo-mcp-gateway --bin gateway -- audit-counts --state-db "${state_db}")"
    echo "${audit_counts}" | grep -E '"auth_events":[1-9][0-9]*'
    echo "${audit_counts}" | grep -E '"policy_events":[1-9][0-9]*'
    audit_summary="$(cargo run -q -p veoveo-mcp-gateway --bin gateway -- audit-method-summary --state-db "${state_db}")"
    echo "${audit_summary}" | jq -e '.[] | select(.method == "admin/jwt-revocations" and .allow_events == 1)' >/dev/null
    echo "${audit_summary}" | jq -e '.[] | select(.method == "admin/jwt-revocations/prune" and .allow_events == 1)' >/dev/null

# Smoke-test OTLP HTTP log and trace export from the gateway.
smoke-otel:
    #!/usr/bin/env bash
    set -euo pipefail
    gateway_port=18804
    otlp_port=18805
    gateway_base="http://127.0.0.1:${gateway_port}"
    otlp_base="http://127.0.0.1:${otlp_port}"
    tmpdir="$(mktemp -d)"
    gateway_log="${tmpdir}/gateway.log"
    otlp_log="${tmpdir}/otlp.log"
    otlp_ready="${tmpdir}/otlp.ready"
    otlp_hits="${tmpdir}/otlp.hits"
    state_db="${tmpdir}/gateway-state.duckdb"
    internal_secret="local-smoke-internal-token-secret-32-bytes-minimum"
    auth_private_key="$({{conformance}} gateway-private-key-der-b64)"
    gateway_pid=""
    otlp_pid=""
    cleanup() {
        if [ -n "${gateway_pid}" ]; then
            kill -INT "${gateway_pid}" 2>/dev/null || true
            wait "${gateway_pid}" 2>/dev/null || true
        fi
        if [ -n "${otlp_pid}" ]; then
            kill "${otlp_pid}" 2>/dev/null || true
            wait "${otlp_pid}" 2>/dev/null || true
        fi
        rm -rf "${tmpdir}"
    }
    trap cleanup EXIT
    {{conformance}} otlp-http-sink --port "${otlp_port}" --ready-file "${otlp_ready}" --hits-file "${otlp_hits}" >"${otlp_log}" 2>&1 &
    otlp_pid=$!
    for _ in {1..150}; do
        if [ -f "${otlp_ready}" ]; then
            break
        fi
        sleep 0.2
    done
    test -f "${otlp_ready}"
    OTEL_EXPORTER_OTLP_ENDPOINT="${otlp_base}" VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64="${auth_private_key}" cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${gateway_port}" --public-base-url https://veoveo.bioma.ai --control-plane {{gateway-smoke-control-plane}} --state-db "${state_db}" >"${gateway_log}" 2>&1 &
    gateway_pid=$!
    for _ in {1..150}; do
        if curl -fsS "${gateway_base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${gateway_base}/readyz" | grep -F '"profiles":1'
    curl -fsS "${gateway_base}/healthz" >/dev/null
    for _ in {1..80}; do
        if grep -q '^logs ' "${otlp_hits}" && grep -q '^traces ' "${otlp_hits}"; then
            break
        fi
        sleep 0.25
    done
    grep -q '^logs ' "${otlp_hits}"
    grep -q '^traces ' "${otlp_hits}"

# Smoke-test the media MCP HTTP boundary and internal gateway assertion requirement.
smoke-media-mcp-auth:
    #!/usr/bin/env bash
    set -euo pipefail
    port=18800
    base="http://127.0.0.1:${port}"
    tmpdir="$(mktemp -d)"
    log="${tmpdir}/media.log"
    state_db="${tmpdir}/state.duckdb"
    internal_secret="local-smoke-internal-token-secret-32-bytes-minimum"
    cleanup() {
        kill "${pid}" 2>/dev/null || true
        wait "${pid}" 2>/dev/null || true
        rm -rf "${tmpdir}"
    }
    MEDIA_PROVIDER_API_KEY=smoke AWS_ACCESS_KEY_ID=smoke AWS_SECRET_ACCESS_KEY=smoke VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" cargo run -p veoveo-media-mcp --bin server -- --port "${port}" --public-base-url https://veoveo.bioma.ai --state-db "${state_db}" --artifact-endpoint http://127.0.0.1:9 --artifact-bucket smoke-artifacts --artifact-region us-east-1 >"${log}" 2>&1 &
    pid=$!
    trap cleanup EXIT
    for _ in {1..150}; do
        if curl -fsS "${base}/media/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${base}/media/healthz" | grep -F 'ok'
    grep -E '^\{' "${log}" | jq -e 'select(.message == "listening" and .service == "veoveo-media-mcp" and .mcp_path == "/media/mcp")' >/dev/null
    grep -E '^\{' "${log}" | jq -e 'select(.message == "media retention gc completed")' >/dev/null
    status="$(curl -sS -o /dev/null -w "%{http_code}" "${base}/media/mcp")"
    test "${status}" = "401"
    status="$(curl -sS -o /dev/null -w "%{http_code}" "${base}/media/artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")"
    test "${status}" = "401"
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" {{conformance}} --url "${base}/media/mcp" info >/dev/null

# Smoke-test direct hosted media task behavior without the gateway projection layer.
smoke-media-task-run:
    #!/usr/bin/env bash
    set -euo pipefail
    media_port=18807
    provider_port=18808
    media_base="http://127.0.0.1:${media_port}"
    provider_base="http://127.0.0.1:${provider_port}"
    tmpdir="$(mktemp -d)"
    provider_log="${tmpdir}/provider.log"
    media_log="${tmpdir}/media.log"
    provider_ready="${tmpdir}/provider.ready"
    media_state_db="${tmpdir}/media-state.duckdb"
    output_dir="${tmpdir}/outputs"
    internal_secret="local-smoke-internal-token-secret-32-bytes-minimum"
    provider_pid=""
    media_pid=""
    cleanup() {
        if [ -n "${media_pid}" ]; then
            kill "${media_pid}" 2>/dev/null || true
            wait "${media_pid}" 2>/dev/null || true
        fi
        if [ -n "${provider_pid}" ]; then
            kill "${provider_pid}" 2>/dev/null || true
            wait "${provider_pid}" 2>/dev/null || true
        fi
        rm -rf "${tmpdir}"
    }
    trap cleanup EXIT
    {{conformance}} fake-media-provider --port "${provider_port}" --ready-file "${provider_ready}" >"${provider_log}" 2>&1 &
    provider_pid=$!
    for _ in {1..150}; do
        if [ -f "${provider_ready}" ] && curl -fsS "${provider_base}/api/v3/models" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    test -f "${provider_ready}"
    MEDIA_PROVIDER_WEBHOOK_SECRET= MEDIA_PROVIDER_API_KEY=smoke VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" cargo run -p veoveo-media-mcp --bin server -- --port "${media_port}" --public-base-url "${media_base}" --state-db "${media_state_db}" --artifact-store memory --provider-base-url "${provider_base}" >"${media_log}" 2>&1 &
    media_pid=$!
    for _ in {1..150}; do
        if curl -fsS "${media_base}/media/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${media_base}/media/healthz" | grep -F 'ok'
    cancel_output="$(env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" {{conformance}} --url "${media_base}/media/mcp" run fake/image --input '{"prompt":"cancel"}' --cancel)"
    printf '%s\n' "${cancel_output}"
    printf '%s\n' "${cancel_output}" | grep -E '^cancelled task [^ ]+ \(status Cancelled\)$' >/dev/null
    complete_output="$(env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" {{conformance}} --url "${media_base}/media/mcp" complete fake)"
    printf '%s\n' "${complete_output}" | grep -F 'fake/image'
    run_output="$(env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" {{conformance}} --url "${media_base}/media/mcp" run fake/image --input '{"prompt":"smoke"}' --output-dir "${output_dir}")"
    printf '%s\n' "${run_output}"
    task_id="$(printf '%s\n' "${run_output}" | sed -n 's/^task \([^ ]*\) created.*/\1/p')"
    test -n "${task_id}"
    structured_json="$(printf '%s\n' "${run_output}" | sed -n 's/^structured: //p')"
    test -n "${structured_json}"
    printf '%s\n' "${structured_json}" | jq -e --arg task_id "${task_id}" 'all(.artifacts[]; .metadata.task_id == $task_id)' >/dev/null
    find "${output_dir}" -type f -name '*.png' -size +0c | grep -q .
    usage_json=""
    for _ in {1..90}; do
        usage_json="$(env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" {{conformance}} --url "${media_base}/media/mcp" usage "${task_id}" 2>/dev/null || true)"
        if printf '%s\n' "${usage_json}" | jq -e '.records[]? | select(.kind == "actual" and .amount == 0.01 and .currency == "USD")' >/dev/null; then
            break
        fi
        sleep 0.25
    done
    printf '%s\n' "${usage_json}" | jq -e --arg task_id "${task_id}" '.task_id == $task_id and .usage_uri == ("media://usage/task/" + $task_id) and all(.records[]; .task_id == $task_id)' >/dev/null
    printf '%s\n' "${usage_json}" | jq -e '.records[] | select(.kind == "estimate" and .amount == 0.01 and .currency == "USD")' >/dev/null
    printf '%s\n' "${usage_json}" | jq -e '.records[] | select(.kind == "actual" and .amount == 0.01 and .currency == "USD")' >/dev/null

# Smoke-test authenticated gateway-to-media MCP forwarding.
smoke-gateway-authenticated:
    #!/usr/bin/env bash
    set -euo pipefail
    media_port=18801
    gateway_port=18802
    media_base="http://127.0.0.1:${media_port}"
    gateway_base="http://127.0.0.1:${gateway_port}"
    tmpdir="$(mktemp -d)"
    media_log="${tmpdir}/media.log"
    gateway_log="${tmpdir}/gateway.log"
    media_state_db="${tmpdir}/media-state.duckdb"
    gateway_state_db="${tmpdir}/gateway-state.duckdb"
    internal_secret="local-smoke-internal-token-secret-32-bytes-minimum"
    auth_private_key="$({{conformance}} gateway-private-key-der-b64)"
    media_pid=""
    gateway_pid=""
    cleanup() {
        if [ -n "${gateway_pid}" ]; then
            kill "${gateway_pid}" 2>/dev/null || true
            wait "${gateway_pid}" 2>/dev/null || true
        fi
        if [ -n "${media_pid}" ]; then
            kill "${media_pid}" 2>/dev/null || true
            wait "${media_pid}" 2>/dev/null || true
        fi
        rm -rf "${tmpdir}"
    }
    trap cleanup EXIT
    MEDIA_PROVIDER_API_KEY=smoke AWS_ACCESS_KEY_ID=smoke AWS_SECRET_ACCESS_KEY=smoke VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" cargo run -p veoveo-media-mcp --bin server -- --port "${media_port}" --public-base-url https://veoveo.bioma.ai --state-db "${media_state_db}" --artifact-endpoint http://127.0.0.1:9 --artifact-bucket smoke-artifacts --artifact-region us-east-1 >"${media_log}" 2>&1 &
    media_pid=$!
    for _ in {1..150}; do
        if curl -fsS "${media_base}/media/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${media_base}/media/healthz" | grep -F 'ok'
    grep -E '^\{' "${media_log}" | jq -e 'select(.message == "listening" and .service == "veoveo-media-mcp" and .mcp_path == "/media/mcp")' >/dev/null
    grep -E '^\{' "${media_log}" | jq -e 'select(.message == "media retention gc completed")' >/dev/null
    VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64="${auth_private_key}" cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${gateway_port}" --public-base-url https://veoveo.bioma.ai --control-plane {{gateway-smoke-control-plane}} --state-db "${gateway_state_db}" >"${gateway_log}" 2>&1 &
    gateway_pid=$!
    for _ in {1..150}; do
        if curl -fsS "${gateway_base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${gateway_base}/readyz" | grep -F '"profiles":1'
    grep -E '^\{' "${gateway_log}" | jq -e 'select(.message == "listening" and .service == "veoveo-mcp-gateway" and .server_count == 1 and .profile_count == 1)' >/dev/null
    grep -E '^\{' "${gateway_log}" | jq -e 'select(.message == "gateway retention gc completed")' >/dev/null
    token_endpoint="${gateway_base}/oauth/default/token"
    admin_token="$({{conformance}} gateway-token-exchange --token-url "${token_endpoint}" --scope media:use --scope gateway:admin)"
    status="$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${admin_token}" -X POST "${gateway_base}/admin/default/reload-control-plane")"
    test "${status}" = "200"
    control_status="$(curl -fsS -H "Authorization: Bearer ${admin_token}" "${gateway_base}/admin/default/control-plane")"
    echo "${control_status}" | jq -e '.status == "ok" and .servers == 1 and .profiles == 1' >/dev/null
    control_apply="$(curl -fsS -X PUT -H "Authorization: Bearer ${admin_token}" -H "Content-Type: application/json" --data-binary @{{gateway-smoke-control-plane}} "${gateway_base}/admin/default/control-plane")"
    echo "${control_apply}" | jq -e '.status == "applied" and .servers == 1 and .profiles == 1' >/dev/null
    revision_id="$(echo "${control_apply}" | jq -r '.revision_id')"
    test -n "${revision_id}"
    test "${revision_id}" != "null"
    control_status="$(curl -fsS -H "Authorization: Bearer ${admin_token}" "${gateway_base}/admin/default/control-plane")"
    test "$(echo "${control_status}" | jq -r '.revision_id')" = "${revision_id}"
    kill "${gateway_pid}"
    wait "${gateway_pid}" 2>/dev/null || true
    gateway_pid=""
    VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64="${auth_private_key}" cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${gateway_port}" --public-base-url https://veoveo.bioma.ai --control-plane {{gateway-smoke-control-plane}} --state-db "${gateway_state_db}" >>"${gateway_log}" 2>&1 &
    gateway_pid=$!
    for _ in {1..150}; do
        if curl -fsS "${gateway_base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${gateway_base}/readyz" | grep -F '"profiles":1'
    control_status="$(curl -fsS -H "Authorization: Bearer ${admin_token}" "${gateway_base}/admin/default/control-plane")"
    test "$(echo "${control_status}" | jq -r '.revision_id')" = "${revision_id}"
    token="$({{conformance}} gateway-token-exchange --token-url "${token_endpoint}" --scope media:use)"
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" info >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" resource media://usage >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" prompts >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" prompt media-model-select --arguments '{"goal":"choose an image generation model for a product render","media_type":"image","budget":"low"}' >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" tasks >/dev/null
    revoked_token="$({{conformance}} gateway-token-exchange --token-url "${token_endpoint}" --scope media:use)"
    revoked_jti="$(jq -nr --arg token "${revoked_token}" '$token | split(".")[1] | gsub("-"; "+") | gsub("_"; "/") | . + (["","===","==","="][length % 4]) | @base64d | fromjson | .jti')"
    revocation_payload="$(jq -n --arg issuer 'https://veoveo.bioma.ai/oauth/default' --arg jwt_id "${revoked_jti}" --arg expires_at '2999-01-01T00:00:00Z' --arg reason 'smoke' '{issuer: $issuer, jwt_id: $jwt_id, expires_at: $expires_at, reason: $reason}')"
    revocation_result="$(curl -fsS -X POST -H "Authorization: Bearer ${admin_token}" -H "Content-Type: application/json" --data "${revocation_payload}" "${gateway_base}/admin/default/jwt-revocations")"
    echo "${revocation_result}" | jq -e --arg jwt_id "${revoked_jti}" '.status == "revoked" and .revocation.jwt_id == $jwt_id' >/dev/null
    if env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${revoked_token}" {{conformance}} --url "${gateway_base}/mcp/default" info >/dev/null 2>&1; then
        echo "revoked gateway token was unexpectedly authorized" >&2
        exit 1
    fi
    ema_token="$({{conformance}} gateway-id-jag-token-exchange --token-url "${token_endpoint}" --id-jag-scope media:use --group engineering --role operator --data-label cui)"
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${ema_token}" {{conformance}} --url "${gateway_base}/mcp/default" info >/dev/null
    cui_control_plane="${tmpdir}/gateway.cui.json"
    jq '(.policies[] | select(.version == "2026-07-02") | .rules[] | select(.id == "allow_media_profile_use") | .required_data_labels) = ["cui"]' {{gateway-smoke-control-plane}} >"${cui_control_plane}"
    cui_apply="$(curl -fsS -X PUT -H "Authorization: Bearer ${admin_token}" -H "Content-Type: application/json" --data-binary @"${cui_control_plane}" "${gateway_base}/admin/default/control-plane")"
    echo "${cui_apply}" | jq -e '.status == "applied" and .servers == 1 and .profiles == 1' >/dev/null
    if env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" resource media://usage >/dev/null 2>&1; then
        echo "missing-data-label gateway token was unexpectedly authorized" >&2
        exit 1
    fi
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${ema_token}" {{conformance}} --url "${gateway_base}/mcp/default" resource media://usage >/dev/null
    replay_jti="smoke-id-jag-replay"
    {{conformance}} gateway-id-jag-token-exchange --token-url "${token_endpoint}" --id-jag-scope media:use --jwt-id "${replay_jti}" >/dev/null
    if {{conformance}} gateway-id-jag-token-exchange --token-url "${token_endpoint}" --id-jag-scope media:use --jwt-id "${replay_jti}" >/dev/null 2>&1; then
        echo "replayed ID-JAG was unexpectedly accepted" >&2
        exit 1
    fi
    denied_token="$({{conformance}} gateway-token-exchange --token-url "${token_endpoint}" --scope gateway:admin)"
    if env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${denied_token}" {{conformance}} --url "${gateway_base}/mcp/default" info >/dev/null 2>&1; then
        echo "missing-scope gateway token was unexpectedly authorized" >&2
        exit 1
    fi
    kill "${gateway_pid}"
    wait "${gateway_pid}" 2>/dev/null || true
    gateway_pid=""
    audit_counts="$(cargo run -q -p veoveo-mcp-gateway --bin gateway -- audit-counts --state-db "${gateway_state_db}")"
    echo "${audit_counts}" | grep -E '"auth_events":[1-9][0-9]*'
    echo "${audit_counts}" | grep -E '"policy_events":[1-9][0-9]*'
    audit_reasons="$(cargo run -q -p veoveo-mcp-gateway --bin gateway -- audit-reason-summary --state-db "${gateway_state_db}")"
    echo "${audit_reasons}" | jq -e '.[] | select(.reason == "missing_data_label" and .events >= 1)' >/dev/null

# Smoke-test a full gateway task run with webhook completion, artifact storage, and billing reconciliation.
smoke-gateway-task-run:
    #!/usr/bin/env bash
    set -euo pipefail
    media_port=18801
    gateway_port=18802
    provider_port=18806
    media_base="http://127.0.0.1:${media_port}"
    gateway_base="http://127.0.0.1:${gateway_port}"
    provider_base="http://127.0.0.1:${provider_port}"
    tmpdir="$(mktemp -d)"
    provider_log="${tmpdir}/provider.log"
    media_log="${tmpdir}/media.log"
    gateway_log="${tmpdir}/gateway.log"
    provider_ready="${tmpdir}/provider.ready"
    media_state_db="${tmpdir}/media-state.duckdb"
    gateway_state_db="${tmpdir}/gateway-state.duckdb"
    output_dir="${tmpdir}/outputs"
    internal_secret="local-smoke-internal-token-secret-32-bytes-minimum"
    auth_private_key="$({{conformance}} gateway-private-key-der-b64)"
    provider_pid=""
    media_pid=""
    gateway_pid=""
    cleanup() {
        if [ -n "${gateway_pid}" ]; then
            kill "${gateway_pid}" 2>/dev/null || true
            wait "${gateway_pid}" 2>/dev/null || true
        fi
        if [ -n "${media_pid}" ]; then
            kill "${media_pid}" 2>/dev/null || true
            wait "${media_pid}" 2>/dev/null || true
        fi
        if [ -n "${provider_pid}" ]; then
            kill "${provider_pid}" 2>/dev/null || true
            wait "${provider_pid}" 2>/dev/null || true
        fi
        rm -rf "${tmpdir}"
    }
    trap cleanup EXIT
    {{conformance}} fake-media-provider --port "${provider_port}" --ready-file "${provider_ready}" >"${provider_log}" 2>&1 &
    provider_pid=$!
    for _ in {1..150}; do
        if [ -f "${provider_ready}" ] && curl -fsS "${provider_base}/api/v3/models" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    test -f "${provider_ready}"
    MEDIA_PROVIDER_WEBHOOK_SECRET= MEDIA_PROVIDER_API_KEY=smoke VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" cargo run -p veoveo-media-mcp --bin server -- --port "${media_port}" --public-base-url "${media_base}" --state-db "${media_state_db}" --artifact-store memory --provider-base-url "${provider_base}" >"${media_log}" 2>&1 &
    media_pid=$!
    for _ in {1..150}; do
        if curl -fsS "${media_base}/media/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${media_base}/media/healthz" | grep -F 'ok'
    VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" VEOVEO_AUTHORIZATION_SERVER_PRIVATE_KEY_DER_B64="${auth_private_key}" cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${gateway_port}" --public-base-url https://veoveo.bioma.ai --control-plane {{gateway-smoke-control-plane}} --state-db "${gateway_state_db}" >"${gateway_log}" 2>&1 &
    gateway_pid=$!
    for _ in {1..150}; do
        if curl -fsS "${gateway_base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${gateway_base}/readyz" | grep -F '"profiles":1'
    token="$({{conformance}} gateway-id-jag-token-exchange --token-url "${gateway_base}/oauth/default/token" --id-jag-scope media:use --group engineering --role operator --data-label cui)"
    cancel_output="$(env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" run fake/image --tool-name media__run --input '{"prompt":"cancel"}' --cancel)"
    printf '%s\n' "${cancel_output}"
    printf '%s\n' "${cancel_output}" | grep -E '^cancelled task [^ ]+ \(status Cancelled\)$' >/dev/null
    complete_output="$(env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" complete fake)"
    printf '%s\n' "${complete_output}" | grep -F 'fake/image'
    run_output="$(env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" run fake/image --tool-name media__run --input '{"prompt":"smoke"}' --output-dir "${output_dir}")"
    printf '%s\n' "${run_output}"
    task_id="$(printf '%s\n' "${run_output}" | sed -n 's/^task \([^ ]*\) created.*/\1/p')"
    test -n "${task_id}"
    structured_json="$(printf '%s\n' "${run_output}" | sed -n 's/^structured: //p')"
    test -n "${structured_json}"
    printf '%s\n' "${structured_json}" | jq -e --arg task_id "${task_id}" 'all(.artifacts[]; .metadata.task_id == $task_id)' >/dev/null
    printf '%s\n' "${structured_json}" | jq -e 'all(.artifacts[]; .compliance.tenant_id == "tenant-a" and (.compliance.data_labels | index("cui")))' >/dev/null
    find "${output_dir}" -type f -name '*.png' -size +0c | grep -q .
    usage_json=""
    for _ in {1..90}; do
        usage_json="$(env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" usage "${task_id}" 2>/dev/null || true)"
        if printf '%s\n' "${usage_json}" | jq -e '.records[]? | select(.kind == "actual" and .amount == 0.01 and .currency == "USD")' >/dev/null; then
            break
        fi
        sleep 0.25
    done
    printf '%s\n' "${usage_json}" | jq -e --arg task_id "${task_id}" '.task_id == $task_id and .usage_uri == ("media://usage/task/" + $task_id) and all(.records[]; .task_id == $task_id)' >/dev/null
    printf '%s\n' "${usage_json}" | jq -e '.records[] | select(.kind == "estimate" and .amount == 0.01 and .currency == "USD")' >/dev/null
    printf '%s\n' "${usage_json}" | jq -e '.records[] | select(.kind == "actual" and .amount == 0.01 and .currency == "USD")' >/dev/null
    kill "${gateway_pid}"
    wait "${gateway_pid}" 2>/dev/null || true
    gateway_pid=""
    audit_summary="$(cargo run -q -p veoveo-mcp-gateway --bin gateway -- audit-method-summary --state-db "${gateway_state_db}")"
    echo "${audit_summary}" | jq -e 'all(.[]; .deny_events == 0)' >/dev/null
    echo "${audit_summary}" | jq -e '
        map({key: .method, value: .allow_events}) | from_entries as $methods |
        (($methods["completion/complete"] // 0) >= 1) and
        (($methods["tools/call"] // 0) >= 2) and
        (($methods["tasks/cancel"] // 0) >= 1) and
        (($methods["tasks/get"] // 0) >= 2) and
        (($methods["tasks/result"] // 0) >= 2) and
        (($methods["resources/subscribe"] // 0) >= 1) and
        (($methods["resources/unsubscribe"] // 0) >= 1) and
        (($methods["resources/read"] // 0) >= 2)
    ' >/dev/null

# Build MCP images.
compose-build:
    {{compose}} build media-mcp mcp-gateway

# Build and start RustFS, media-mcp, mcp-gateway, and the managed Cloudflare tunnel.
compose-up:
    {{compose}} up --build -d

# Stop the Compose stack.
compose-down:
    {{compose}} down --remove-orphans

# Stop the Compose stack and remove its volumes.
compose-down-volumes:
    {{compose}} down --remove-orphans --volumes

# Show Compose service status.
compose-ps:
    {{compose}} ps

# Follow logs for one service.
logs service='mcp-gateway':
    {{compose}} logs -f --tail=200 {{service}}

# Check local health and, optionally, public tunnel health.
health public_base_url='':
    curl -fsS http://localhost:8788/healthz
    @echo
    curl -fsS http://localhost:8787/media/healthz
    @echo
    if [ -n '{{public_base_url}}' ]; then curl -fsS '{{public_base_url}}/healthz'; echo; fi

# Mint a local gateway access token for the default profile.
gateway-token scope='media:use':
    {{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope '{{scope}}'

# Show gateway MCP server info and resource templates.
info:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} info

# List models through the gateway, optionally with a local query string.
models query='':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; if [ -n '{{query}}' ]; then env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} models '{{query}}'; else env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} models; fi

# Complete model ids by prefix through the gateway.
complete prefix:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} complete '{{prefix}}'

# Read one model schema through the gateway.
schema model:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} schema '{{model}}'

# Run an arbitrary model through the gateway with a raw JSON input object.
run model input output_dir='output':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} run '{{model}}' --tool-name media__run --input '{{input}}' --output-dir '{{output_dir}}'

# Run the default image edit e2e against the public base URL and save returned artifacts.
run-edit public_base_url output_dir='output/e2e':
    input="{\"prompt\":\"add a red wizard hat\",\"images\":[\"{{public_base_url}}/media/files/{{default-input-image}}\"]}"; token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} run '{{default-model}}' --tool-name media__run --input "$input" --output-dir '{{output_dir}}'

# Read one gateway task usage report.
usage task_id:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} usage '{{task_id}}'

# Read and save one artifact by sha256 through the gateway.
artifact sha256 output_dir='output':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope media:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} artifact '{{sha256}}' --output-dir '{{output_dir}}'

# Start the stack, check health, print MCP info, and run the default edit task.
e2e public_base_url output_dir='output/e2e':
    just compose-up
    just health '{{public_base_url}}'
    just info
    just run-edit '{{public_base_url}}' '{{output_dir}}'
