set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := true

compose := "docker compose -f compose.yaml -f compose.tunnel.yaml --profile dev --profile tunnel"
mcp-url := "http://localhost:8787/media/mcp"
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

# Revoke one gateway JWT id until its original token expiration.
gateway-revoke-jwt jwt_id expires_at issuer='https://veoveo.bioma.ai/oauth/default' profile='default' reason='operator_request':
    mkdir -p data/gateway
    cargo run -p veoveo-mcp-gateway --bin gateway -- revoke-jwt --state-db data/gateway/state.duckdb --profile '{{profile}}' --issuer '{{issuer}}' --jwt-id '{{jwt_id}}' --expires-at '{{expires_at}}' --reason '{{reason}}'

# Remove expired gateway JWT revocation entries.
gateway-prune-revoked-jwts:
    mkdir -p data/gateway
    cargo run -p veoveo-mcp-gateway --bin gateway -- prune-revoked-jwts --state-db data/gateway/state.duckdb

# Smoke-test gateway contract/control-plane behavior without external services.
smoke-gateway:
    cargo test -p veoveo-mcp-contract -p veoveo-mcp-gateway
    just gateway-validate
    cargo run -p veoveo-mcp-gateway --bin gateway -- validate --control-plane {{gateway-smoke-control-plane}}
    just smoke-gateway-http
    just smoke-media-mcp-auth
    just smoke-gateway-authenticated

# Smoke-test the gateway HTTP boundary and auth challenge.
smoke-gateway-http:
    #!/usr/bin/env bash
    set -euo pipefail
    port=18799
    base="http://127.0.0.1:${port}"
    tmpdir="$(mktemp -d)"
    log="${tmpdir}/gateway.log"
    state_db="${tmpdir}/state.duckdb"
    internal_secret="local-smoke-internal-token-secret-32-bytes-minimum"
    pid=""
    cleanup() {
        if [ -n "${pid}" ]; then
            kill "${pid}" 2>/dev/null || true
            wait "${pid}" 2>/dev/null || true
        fi
        rm -rf "${tmpdir}"
    }
    VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${port}" --public-base-url https://veoveo.bioma.ai --control-plane {{gateway-control-plane}} --state-db "${state_db}" >"${log}" 2>&1 &
    pid=$!
    trap cleanup EXIT
    for _ in {1..50}; do
        if curl -fsS "${base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${base}/readyz" | grep -F '"profiles":1'
    {{conformance}} --url "${base}/mcp/default" auth-discovery \
        --metadata-url "${base}/.well-known/oauth-protected-resource/mcp/default" \
        --authorization-server-metadata-url "${base}/.well-known/oauth-authorization-server/oauth/default" \
        --required-scope media:use \
        --required-extension io.modelcontextprotocol/enterprise-managed-authorization \
        --required-extension io.modelcontextprotocol/oauth-client-credentials \
        --required-grant-type authorization_code \
        --required-grant-type client_credentials \
        --required-grant-type urn:ietf:params:oauth:grant-type:jwt-bearer \
        --required-grant-profile urn:ietf:params:oauth:grant-profile:id-jag \
        --required-token-auth-method none \
        --required-token-auth-method private_key_jwt
    status="$(curl -sS -o /dev/null -w "%{http_code}" -X POST "${base}/admin/default/reload-control-plane")"
    test "${status}" = "401"
    kill "${pid}"
    wait "${pid}" 2>/dev/null || true
    pid=""
    cargo run -p veoveo-mcp-gateway --bin gateway -- audit-counts --state-db "${state_db}" | grep -F '"auth_events":2'
    cargo run -p veoveo-mcp-gateway --bin gateway -- revoke-jwt --state-db "${state_db}" --profile default --issuer https://veoveo.bioma.ai/oauth/default --jwt-id smoke-jwt --expires-at 2999-01-01T00:00:00Z --reason smoke >/dev/null
    test "$(cargo run -q -p veoveo-mcp-gateway --bin gateway -- prune-revoked-jwts --state-db "${state_db}")" = "0"

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
    for _ in {1..50}; do
        if curl -fsS "${base}/media/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${base}/media/healthz" | grep -F 'ok'
    status="$(curl -sS -o /dev/null -w "%{http_code}" "${base}/media/mcp")"
    test "${status}" = "401"
    status="$(curl -sS -o /dev/null -w "%{http_code}" "${base}/media/artifacts/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")"
    test "${status}" = "401"
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" {{conformance}} --url "${base}/media/mcp" info >/dev/null

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
    for _ in {1..50}; do
        if curl -fsS "${media_base}/media/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${media_base}/media/healthz" | grep -F 'ok'
    VEOVEO_INTERNAL_TOKEN_SECRET="${internal_secret}" cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${gateway_port}" --public-base-url https://veoveo.bioma.ai --control-plane {{gateway-smoke-control-plane}} --state-db "${gateway_state_db}" >"${gateway_log}" 2>&1 &
    gateway_pid=$!
    for _ in {1..50}; do
        if curl -fsS "${gateway_base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${gateway_base}/readyz" | grep -F '"profiles":1'
    admin_token="$({{conformance}} gateway-token --scope media:use --scope gateway:admin --jwt-id smoke-gateway-admin)"
    status="$(curl -sS -o /dev/null -w "%{http_code}" -H "Authorization: Bearer ${admin_token}" -X POST "${gateway_base}/admin/default/reload-control-plane")"
    test "${status}" = "200"
    token="$({{conformance}} gateway-token --scope media:use --jwt-id smoke-gateway-authenticated)"
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" info >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" resource media://usage >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" prompts >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" prompt media-model-select --arguments '{"goal":"choose an image generation model for a product render","media_type":"image","budget":"low"}' >/dev/null
    env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="${token}" {{conformance}} --url "${gateway_base}/mcp/default" tasks >/dev/null
    denied_token="$({{conformance}} gateway-token --scope gateway:admin --jwt-id smoke-gateway-denied)"
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

# Build MCP images.
compose-build:
    {{compose}} build media-mcp mcp-gateway

# Build and start RustFS, media-mcp, and the managed Cloudflare tunnel.
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
logs service='media-mcp':
    {{compose}} logs -f --tail=200 {{service}}

# Check local health and, optionally, public tunnel health.
health public_base_url='':
    curl -fsS http://localhost:8787/media/healthz
    @echo
    if [ -n '{{public_base_url}}' ]; then curl -fsS '{{public_base_url}}/media/healthz'; echo; fi

# Show MCP server info and resource templates.
info:
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} info

# List models, optionally with a local query string.
models query='':
    if [ -n '{{query}}' ]; then env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} models '{{query}}'; else env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} models; fi

# Complete model ids by prefix.
complete prefix:
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} complete '{{prefix}}'

# Read one model schema.
schema model:
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} schema '{{model}}'

# Run an arbitrary model with a raw JSON input object.
run model input output_dir='output':
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} run '{{model}}' --input '{{input}}' --output-dir '{{output_dir}}'

# Run the default image edit e2e against the public base URL and save returned artifacts.
run-edit public_base_url output_dir='output/e2e':
    input="{\"prompt\":\"add a red wizard hat\",\"images\":[\"{{public_base_url}}/media/files/{{default-input-image}}\"]}"; env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} run '{{default-model}}' --input "$input" --output-dir '{{output_dir}}'

# Read one task usage report.
usage task_id:
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} usage '{{task_id}}'

# Read and save one artifact by sha256.
artifact sha256 output_dir='output':
    env -u MCP_BEARER_TOKEN VEOVEO_INTERNAL_TOKEN_SECRET="${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" {{conformance}} --url {{mcp-url}} artifact '{{sha256}}' --output-dir '{{output_dir}}'

# Start the stack, check health, print MCP info, and run the default edit task.
e2e public_base_url output_dir='output/e2e':
    just compose-up
    just health '{{public_base_url}}'
    just info
    just run-edit '{{public_base_url}}' '{{output_dir}}'
