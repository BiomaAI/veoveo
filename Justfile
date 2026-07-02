set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := true

compose := "docker compose -f compose.yaml -f compose.tunnel.yaml --profile dev --profile tunnel"
mcp-url := "http://localhost:8787/media/mcp"
gateway-control-plane := "configs/gateway.local.json"
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

# Smoke-test gateway contract/control-plane behavior without external services.
smoke-gateway:
    cargo test -p veoveo-mcp-contract -p veoveo-mcp-gateway
    just gateway-validate
    just smoke-gateway-http
    just smoke-media-mcp-auth

# Smoke-test the gateway HTTP boundary and auth challenge.
smoke-gateway-http:
    #!/usr/bin/env bash
    set -euo pipefail
    port=18799
    base="http://127.0.0.1:${port}"
    tmpdir="$(mktemp -d)"
    log="${tmpdir}/gateway.log"
    headers="${tmpdir}/headers"
    body="${tmpdir}/body"
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
    cargo run -p veoveo-mcp-gateway --bin gateway -- serve --port "${port}" --public-base-url https://veoveo.bioma.ai --control-plane {{gateway-control-plane}} --state-db "${state_db}" --internal-token-secret "${internal_secret}" >"${log}" 2>&1 &
    pid=$!
    trap cleanup EXIT
    for _ in {1..50}; do
        if curl -fsS "${base}/healthz" >/dev/null 2>&1; then
            break
        fi
        sleep 0.2
    done
    curl -fsS "${base}/readyz" | grep -F '"profiles":1'
    curl -fsS "${base}/.well-known/oauth-protected-resource/mcp/default" >"${body}"
    grep -F '"resource":"https://veoveo.bioma.ai/mcp/default"' "${body}" >/dev/null
    grep -F '"io.modelcontextprotocol/enterprise-managed-authorization":{}' "${body}" >/dev/null
    grep -F '"io.modelcontextprotocol/oauth-client-credentials":{}' "${body}" >/dev/null
    status="$(curl -sS -D "${headers}" -o "${body}" -w "%{http_code}" "${base}/mcp/default")"
    test "${status}" = "401"
    grep -Fi 'www-authenticate: Bearer resource_metadata="https://veoveo.bioma.ai/.well-known/oauth-protected-resource/mcp/default", scope="media:use"' "${headers}"
    kill "${pid}"
    wait "${pid}" 2>/dev/null || true
    pid=""
    cargo run -p veoveo-mcp-gateway --bin gateway -- audit-counts --state-db "${state_db}" | grep -F '"auth_events":1'

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
    MEDIA_PROVIDER_API_KEY=smoke AWS_ACCESS_KEY_ID=smoke AWS_SECRET_ACCESS_KEY=smoke cargo run -p veoveo-media-mcp --bin server -- --port "${port}" --public-base-url https://veoveo.bioma.ai --state-db "${state_db}" --artifact-endpoint http://127.0.0.1:9 --artifact-bucket smoke-artifacts --artifact-region us-east-1 --internal-token-secret "${internal_secret}" >"${log}" 2>&1 &
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
    {{conformance}} --url "${base}/media/mcp" --internal-token-secret "${internal_secret}" info >/dev/null

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
    {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" info

# List models, optionally with a local query string.
models query='':
    if [ -n '{{query}}' ]; then {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" models '{{query}}'; else {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" models; fi

# Complete model ids by prefix.
complete prefix:
    {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" complete '{{prefix}}'

# Read one model schema.
schema model:
    {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" schema '{{model}}'

# Run an arbitrary model with a raw JSON input object.
run model input output_dir='output':
    {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" run '{{model}}' --input '{{input}}' --output-dir '{{output_dir}}'

# Run the default image edit e2e against the public base URL and save returned artifacts.
run-edit public_base_url output_dir='output/e2e':
    input="{\"prompt\":\"add a red wizard hat\",\"images\":[\"{{public_base_url}}/media/files/{{default-input-image}}\"]}"; {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" run '{{default-model}}' --input "$input" --output-dir '{{output_dir}}'

# Read one task usage report.
usage task_id:
    {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" usage '{{task_id}}'

# Read and save one artifact by sha256.
artifact sha256 output_dir='output':
    {{conformance}} --url {{mcp-url}} --internal-token-secret "${VEOVEO_INTERNAL_TOKEN_SECRET:?set VEOVEO_INTERNAL_TOKEN_SECRET}" artifact '{{sha256}}' --output-dir '{{output_dir}}'

# Start the stack, check health, print MCP info, and run the default edit task.
e2e public_base_url output_dir='output/e2e':
    just compose-up
    just health '{{public_base_url}}'
    just info
    just run-edit '{{public_base_url}}' '{{output_dir}}'
