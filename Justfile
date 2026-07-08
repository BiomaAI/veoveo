set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := true

compose := "docker compose -f compose.yaml -f compose.tunnel.yaml --profile dev --profile tunnel"
mcp-url := "http://localhost:8780/mcp/operator"
gateway-token-url := "http://localhost:8780/oauth/token"
gateway-admin-url := "http://localhost:8780/admin"
gateway-control-plane := "configs/gateway.bioma.json"
gateway-smoke-control-plane := "configs/gateway.smoke.json"
conformance := "cargo run -p veoveo-mcp-conformance --bin conformance --"
smoke := "DYLD_LIBRARY_PATH=\"$PWD/target/debug/deps${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}\" target/debug/smoke"
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

# Bridge a stdio MCP server (default: Rerun viewer MCP) to streamable HTTP.
stdio-bridge listen='127.0.0.1:8790' +child='rerun viewer-mcp':
    cargo run -p veoveo-mcp-stdio-bridge --bin bridge -- --listen {{listen}} \
        --allowed-host rerun-bridge:8790,localhost:8790,127.0.0.1:8790 -- {{child}}

# Validate the typed gateway control plane.
gateway-validate:
    cargo run -p veoveo-mcp-gateway --bin gateway -- validate --control-plane {{gateway-control-plane}}

# Validate typed self-hosted deployment profiles.
deployments-validate:
    {{conformance}} deployment-validate --file configs/deployments.json

# Smoke-test Compose edge routing and published-port shape.
smoke-compose-config:
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} compose-config

# Write JSON Schemas for external Rust/Python/TypeScript contract implementations.
contract-schemas output_dir='schemas':
    {{conformance}} contract-schemas --output-dir '{{output_dir}}'

# Smoke-test contract schema export for non-Rust implementations.
smoke-contract-schemas:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke
    {{smoke}} contract-schemas --conformance-bin target/debug/conformance

# Revoke one gateway JWT id for a target profile until its original token expiration.
gateway-revoke-jwt jwt_id expires_at issuer='https://veoveo.bioma.ai/oauth' admin_profile='admin' target_profile='operator' reason='operator_request':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --client-id admin-service --scope operator:use --scope admin:manage)"; payload="$(jq -n --arg profile '{{target_profile}}' --arg issuer '{{issuer}}' --arg jwt_id '{{jwt_id}}' --arg expires_at '{{expires_at}}' --arg reason '{{reason}}' '{profile: $profile, issuer: $issuer, jwt_id: $jwt_id, expires_at: $expires_at, reason: $reason}')"; curl -fsS -X POST -H "Authorization: Bearer ${token}" -H "Content-Type: application/json" --data "${payload}" "{{gateway-admin-url}}/{{admin_profile}}/jwt-revocations"

# Remove expired gateway JWT revocation entries.
gateway-prune-revoked-jwts profile='admin':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --client-id admin-service --scope operator:use --scope admin:manage)"; curl -fsS -X POST -H "Authorization: Bearer ${token}" "{{gateway-admin-url}}/{{profile}}/jwt-revocations/prune"

# Smoke-test gateway contract/control-plane behavior without external services.
smoke-gateway:
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} gateway-suite --control-plane {{gateway-control-plane}} --smoke-control-plane {{gateway-smoke-control-plane}}

# Smoke-test gateway control-plane seeding against real Postgres.
smoke-gateway-control-db:
    cargo build -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-control-db --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

# Smoke-test the gateway HTTP boundary and auth challenge.
smoke-gateway-http:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-http --conformance-bin target/debug/conformance --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

# Smoke-test OTLP HTTP log and trace export from the gateway.
smoke-otel:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} otel --conformance-bin target/debug/conformance --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

# Smoke-test gateway Vault KV v2 secret resolution.
smoke-gateway-vault-secrets:
    cargo build -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-vault-secrets --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

# Smoke-test the media MCP HTTP boundary and internal gateway assertion requirement.
smoke-media-mcp-auth:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server
    {{smoke}} media-mcp-auth --conformance-bin target/debug/conformance --media-bin target/debug/server

# Smoke-test direct hosted media task behavior without the gateway projection layer.
smoke-media-task-run:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server
    {{smoke}} media-task-run --conformance-bin target/debug/conformance --media-bin target/debug/server

# Smoke-test authenticated gateway-to-media MCP forwarding.
smoke-gateway-authenticated:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-authenticated --conformance-bin target/debug/conformance --media-bin target/debug/server --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

# Smoke-test one gateway profile routing to two hosted MCP servers.
smoke-gateway-two-servers:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-two-servers --conformance-bin target/debug/conformance --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

# Smoke-test a full gateway task run with webhook completion, artifact storage, and billing reconciliation.
smoke-gateway-task-run:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-task-run --conformance-bin target/debug/conformance --media-bin target/debug/server --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

# Smoke-test agent-kernel gateway prerequisites: optional-tool task calls and cross-session task continuity.
smoke-agent-gateway:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-duckdb-mcp --bin server -p veoveo-mcp-gateway --bin gateway -p veoveo-artifact-service --bin artifact-service
    {{smoke}} agent-gateway --conformance-bin target/debug/conformance --duckdb-bin target/debug/server --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}} --artifact-service-bin target/debug/artifact-service

# Smoke-test the agent kernel's durable task detach and resume across processes.
smoke-agent-kernel:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server -p veoveo-mcp-gateway --bin gateway -p veoveo-artifact-service --bin artifact-service -p veoveo-agent-kernel --bin agent
    {{smoke}} agent-kernel --conformance-bin target/debug/conformance --media-bin target/debug/server --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}} --artifact-service-bin target/debug/artifact-service --agent-bin target/debug/agent

# Smoke-test the agent kernel's scheduler: heartbeats, operator wakes, budgets, fail-closed manifests.
smoke-agent-kernel-scheduler:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server -p veoveo-mcp-gateway --bin gateway -p veoveo-artifact-service --bin artifact-service -p veoveo-agent-kernel --bin agent
    {{smoke}} agent-kernel-scheduler --conformance-bin target/debug/conformance --media-bin target/debug/server --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}} --artifact-service-bin target/debug/artifact-service --agent-bin target/debug/agent

# Smoke-test the Pilot agent's full mission loop over coordinates and optimization.
smoke-agent-pilot:
    cargo build -p veoveo-coordinates-mcp --bin server
    cp target/debug/server target/debug/coordinates-mcp-smoke
    cargo build -p veoveo-optimization-mcp --bin server
    cp target/debug/server target/debug/optimization-mcp-smoke
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway -p veoveo-artifact-service --bin artifact-service -p veoveo-agent-kernel --bin agent
    {{smoke}} agent-pilot --conformance-bin target/debug/conformance --coordinates-bin target/debug/coordinates-mcp-smoke --optimization-bin target/debug/optimization-mcp-smoke --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}} --artifact-service-bin target/debug/artifact-service --agent-bin target/debug/agent

# Run the real Pilot against the live local compose stack with Cloudflare credentials from .env.
agent-pilot-local data_dir="output/pilot-data":
    cargo build -p veoveo-agent-kernel --bin agent
    target/debug/agent run --manifest configs/agents/pilot/manifest.json --data-dir {{data_dir}} --viewer-tee rerun+http://127.0.0.1:9876/proxy

# Build MCP images.
compose-build:
    {{compose}} build media-mcp mcp-gateway mcp-gateway-seed

# Build and start RustFS, Postgres, media-mcp, mcp-gateway, and the managed Cloudflare tunnel.
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

# Print the hostname from PUBLIC_BASE_URL.
public-host:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${PUBLIC_BASE_URL:?set PUBLIC_BASE_URL}"
    host="${PUBLIC_BASE_URL#https://}"
    host="${host#http://}"
    host="${host%%/*}"
    printf '%s\n' "${host}"

# Configure the managed Cloudflare tunnel to route the public host to Compose edge.
tunnel-configure:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${CLOUDFLARE_ACCOUNT_ID:?set CLOUDFLARE_ACCOUNT_ID}"
    : "${CLOUDFLARE_API_TOKEN:?set CLOUDFLARE_API_TOKEN}"
    : "${CLOUDFLARED_TUNNEL_TOKEN:?set CLOUDFLARED_TUNNEL_TOKEN}"
    : "${PUBLIC_BASE_URL:?set PUBLIC_BASE_URL}"
    host="${PUBLIC_BASE_URL#https://}"
    host="${host#http://}"
    host="${host%%/*}"
    service="http://edge:8080"
    decode_token() {
        printf '%s' "${CLOUDFLARED_TUNNEL_TOKEN}" | base64 --decode 2>/dev/null \
            || printf '%s' "${CLOUDFLARED_TUNNEL_TOKEN}" | base64 -D
    }
    tunnel_id="$(decode_token | jq -r '.t')"
    config_url="https://api.cloudflare.com/client/v4/accounts/${CLOUDFLARE_ACCOUNT_ID}/cfd_tunnel/${tunnel_id}/configurations"
    tmp_current="$(mktemp -t veoveo-cf-config-current.XXXXXX.json)"
    tmp_payload="$(mktemp -t veoveo-cf-config-payload.XXXXXX.json)"
    cleanup() {
        rm -f "${tmp_current}" "${tmp_payload}"
    }
    trap cleanup EXIT
    curl -fsS -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" "${config_url}" >"${tmp_current}"
    current_service="$(jq -r --arg host "${host}" '.result.config.ingress[]? | select(.hostname == $host) | .service' "${tmp_current}" | head -n 1)"
    if [ "${current_service}" = "${service}" ]; then
        jq '{success, errors, tunnel_id: .result.tunnel_id, source: .result.source, version: .result.version, ingress: (.result.config.ingress // [] | map({hostname, path, service}))}' "${tmp_current}"
        exit 0
    fi
    jq --arg host "${host}" --arg service "${service}" '
        {
            config: (
                .result.config
                | .ingress = (
                    (.ingress // [])
                    | if any(.hostname == $host) then
                        map(if .hostname == $host then .service = $service else . end)
                    else
                        [{hostname: $host, service: $service}] + .
                    end
                )
            )
        }
    ' "${tmp_current}" >"${tmp_payload}"
    curl -fsS -X PUT \
        -H "Authorization: Bearer ${CLOUDFLARE_API_TOKEN}" \
        -H "Content-Type: application/json" \
        --data @"${tmp_payload}" \
        "${config_url}" \
        | jq '{success, errors, tunnel_id: .result.tunnel_id, source: .result.source, version: .result.version, ingress: (.result.config.ingress // [] | map({hostname, path, service}))}'

# Verify the public hostname reaches edge, not a direct MCP server.
tunnel-verify:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${PUBLIC_BASE_URL:?set PUBLIC_BASE_URL}"
    check() {
        path="$1"
        expected="$2"
        code="$(curl -sS -o /tmp/veoveo-tunnel-verify.out -w "%{http_code}" "${PUBLIC_BASE_URL}${path}")"
        if [ "${code}" != "${expected}" ]; then
            printf 'expected %s for %s, got %s\n' "${expected}" "${path}" "${code}" >&2
            head -c 400 /tmp/veoveo-tunnel-verify.out >&2 || true
            exit 1
        fi
        printf '%s %s\n' "${path}" "${code}"
    }
    check /healthz 200
    check /readyz 200
    check /media/healthz 200
    check /media/mcp 404

# Check local health and, optionally, public tunnel health.
health public_base_url='':
    curl -fsS http://localhost:8780/healthz
    @echo
    curl -fsS http://localhost:8788/healthz
    @echo
    curl -fsS http://localhost:8787/media/healthz
    @echo
    if [ -n '{{public_base_url}}' ]; then curl -fsS '{{public_base_url}}/healthz'; echo; fi

# Mint a configured service access token for the operator profile.
gateway-token scope='operator:use':
    {{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope '{{scope}}'

# Show gateway MCP server info and resource templates.
info:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} info

# List models through the gateway, optionally with a local query string.
models query='':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; if [ -n '{{query}}' ]; then env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} models '{{query}}'; else env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} models; fi

# Complete model ids by prefix through the gateway.
complete prefix:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} complete '{{prefix}}'

# Read one model schema through the gateway.
schema model:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} schema '{{model}}'

# Run an arbitrary model through the gateway with a raw JSON input object.
run model input output_dir='output':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} run '{{model}}' --tool-name media__run --input '{{input}}' --output-dir '{{output_dir}}'

# Run the default image edit e2e against the public base URL and save returned artifacts.
run-edit public_base_url output_dir='output/e2e':
    input="{\"prompt\":\"add a red wizard hat\",\"images\":[\"{{public_base_url}}/media/files/{{default-input-image}}\"]}"; token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} run '{{default-model}}' --tool-name media__run --input "$input" --output-dir '{{output_dir}}'

# Read one gateway task usage report.
usage task_id:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} usage '{{task_id}}'

# Read and save one artifact by sha256 through the gateway.
artifact sha256 output_dir='output':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_TOKEN_SECRET MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} artifact '{{sha256}}' --output-dir '{{output_dir}}'

# Start the stack, check health, print MCP info, and run the default edit task.
e2e public_base_url output_dir='output/e2e':
    just compose-up
    just health '{{public_base_url}}'
    just info
    just run-edit '{{public_base_url}}' '{{output_dir}}'
