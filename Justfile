set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := true

k3d := "k3d"
kubectl := "kubectl"
helm := "helm"
sumo-kube-context := "k3d-veoveo-sumo"
bioma-kube-context := "k3d-veoveo-bioma"
mcp-url := "http://localhost:8780/mcp/operator"
gateway-token-url := "http://localhost:8780/oauth/token"
gateway-admin-url := "http://localhost:8780/admin"
gateway-control-plane := "configs/gateway.local.json"
gateway-smoke-control-plane := "configs/gateway.smoke.json"
conformance := "cargo run -p veoveo-mcp-conformance --bin conformance --"
smoke := "LD_LIBRARY_PATH=\"$PWD/target/debug/deps${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}\" DYLD_LIBRARY_PATH=\"$PWD/target/debug/deps${DYLD_LIBRARY_PATH:+:$DYLD_LIBRARY_PATH}\" target/debug/smoke"
default-model := "openai/gpt-image-2/edit"
default-input-image := "gol-real-roblox.jpeg"
architecture-python := "uv run --project docs/architecture --locked python"
bioma-images := "veoveo/mcp-gateway:0.1.0 veoveo/artifact-service:0.1.0 veoveo/recording-hub:0.1.0 veoveo/recording-mcp:0.1.0 veoveo/console-bff:0.1.0 veoveo/artifact-mcp:0.1.0 veoveo/media-mcp:0.1.0 veoveo/perception-mcp:0.1.0 veoveo/timeseries-mcp:0.1.0 veoveo/duckdb-mcp:0.1.0 veoveo/optimization-mcp:0.1.0 veoveo/frames-mcp:0.1.0 veoveo/map-mcp:0.1.0 veoveo/view-mcp:0.1.0 veoveo/time-mcp:0.1.0 veoveo/datasheet-mcp:0.1.0 veoveo/chart-mcp:0.1.0 veoveo/mcp-stdio-bridge:0.1.0"

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

# Validate and render the canonical Helm installation.
helm-check:
    {{helm}} lint deploy/helm/veoveo
    {{helm}} lint showcase/sumo/deploy/helm
    {{helm}} template veoveo deploy/helm/veoveo -f deploy/local/k3d/values.yaml >/dev/null
    {{helm}} template bioma deploy/helm/veoveo -f examples/bioma/values.yaml -f examples/bioma/k3d-values.yaml >/dev/null
    {{helm}} template sumo showcase/sumo/deploy/helm >/dev/null

# Create a content-verified offline installation bundle.
offline-bundle output='output/veoveo-offline-0.1.0.tar.gz' platform='linux/amd64':
    deploy/offline/create-bundle.sh --output '{{output}}' --platform '{{platform}}'

# Verify and install an offline bundle into Docker or containerd.
offline-load bundle runtime='docker' install_dir='veoveo-offline':
    deploy/offline/load-bundle.sh --bundle '{{bundle}}' --runtime '{{runtime}}' --install-dir '{{install_dir}}'

# Smoke-test Helm and k3d local deployment rendering.
smoke-helm-config:
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} helm-config

# Run all live SurrealDB 3.2 integration targets in an isolated container.
smoke-surreal:
    cargo run -p veoveo-smoke -- surreal-integration

# Unit + integration tests for the Recording Hub (spooler, sensor-sim, query).
test-hub:
    cargo test -p veoveo-recording-hub

# Perception contracts, runner protocol, and task/server unit tests.
test-perception:
    cargo test -p veoveo-perception-mcp --all-targets

# DeepStream 9 / NVDEC / TensorRT / Recording Hub / final MCP task smoke.
smoke-perception-gpu env_file='.env' work_dir='output/perception/work':
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} perception-gpu --env-file '{{env_file}}' --work-dir '{{work_dir}}'

# Recording Hub durable-spool smoke: kill -9 + restart-resume + QueryEngine.
smoke-hub-spool:
    cargo run -p veoveo-recording-hub --bin hub-smoke -- restart-kill

# Recording Hub catalog rebuild rejects corruption and fails closed.
smoke-hub-catalog:
    cargo run -p veoveo-recording-hub --bin hub-smoke -- catalog-rebuild

# Real H.264 VideoStream proxy/restart/cross-segment/remux/decode smoke.
smoke-hub-video:
    cargo test -p veoveo-recording-hub --test spool_roundtrip h264_video_extracts_across_restart_segment_boundary -- --nocapture

# Recording Hub agent+world smoke: two producers, one hub, dataset routing.
smoke-hub-agent-world:
    cargo run -p veoveo-recording-hub --bin hub-smoke -- agent-world

# Recording Hub performance bench: burst fleet, assert spooler counters.
bench-hub messages='1500':
    cargo run -p veoveo-recording-hub --bin hub-smoke -- rollover-burst --messages {{messages}}

# All Recording Hub checks: crate tests + all process smokes.
smoke-hub: test-hub smoke-hub-spool smoke-hub-agent-world smoke-hub-catalog smoke-hub-video

# Write JSON Schemas for external Rust/Python/TypeScript contract implementations.
contract-schemas output_dir='schemas':
    {{conformance}} contract-schemas --output-dir '{{output_dir}}'

# Unit and integration tests for the Python platform package and the datasheet template.
test-python:
    uv sync --project sdk --all-extras
    uv run --project sdk pytest sdk/python/tests
    uv sync --project templates/python-mcp --all-extras
    uv run --project templates/python-mcp pytest templates/python-mcp/tests

# Datasheet Python template smoke: auth boundary, MCP surface, final task run, artifacts, usage.
smoke-datasheet:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-artifact-service --bin artifact-service
    {{smoke}} datasheet-mcp --conformance-bin target/debug/conformance --artifact-service-bin target/debug/artifact-service

# Render the whitepaper and harness PDFs from their canonical *-print.html sources.
docs-pdf chrome='google-chrome':
    '{{chrome}}' --headless --disable-gpu --no-pdf-header-footer --print-to-pdf=docs/veoveo-whitepaper.pdf docs/veoveo-whitepaper-print.html
    '{{chrome}}' --headless --disable-gpu --no-pdf-header-footer --print-to-pdf=docs/autonomy-harness.pdf docs/autonomy-harness-print.html

# Install the locked formal-architecture Python toolchain.
architecture-sync:
    uv sync --project docs/architecture --locked

# Regenerate the generic formal model, diagrams, and portal.
architecture-render:
    {{architecture-python}} docs/architecture/tools/render.py

# Validate the generic architecture and lint every architecture tool.
architecture-check:
    uv run --project docs/architecture --locked ruff format --check docs/architecture/tools
    uv run --project docs/architecture --locked ruff check docs/architecture/tools
    {{architecture-python}} docs/architecture/tools/validate.py

# Render architecture PDF pages and contact sheets with pinned PDFium.
architecture-qa:
    {{architecture-python}} docs/architecture/tools/qa.py --clean

# Smoke-test governed Map acquisition, activation, and spatial MCP queries in the all-in-one image.
smoke-map-mcp:
    docker build -f servers/map-mcp/Dockerfile -t veoveo/map-mcp:0.1.0 .
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-artifact-service --bin artifact-service
    {{smoke}} map-mcp --conformance-bin target/debug/conformance --artifact-service-bin target/debug/artifact-service --map-image veoveo/map-mcp:0.1.0

# Exercise two isolated views through the production NVIDIA/Vulkan image and MCP task boundary.
smoke-view-mcp:
    docker build -f servers/view-mcp/Dockerfile -t veoveo/view-mcp:0.1.0 .
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} view-mcp --view-image veoveo/view-mcp:0.1.0

# Run billed live Google 3D Tiles acceptance through the production View MCP boundary.
smoke-view-google output='/tmp/veoveo-view-proof/statue-of-liberty.jpg':
    docker build -f servers/view-mcp/Dockerfile -t veoveo/view-mcp:0.1.0 .
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} view-google-live --view-image veoveo/view-mcp:0.1.0 --output '{{output}}'

# Smoke-test contract schema export for non-Rust implementations.
smoke-contract-schemas:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke
    {{smoke}} contract-schemas --conformance-bin target/debug/conformance

# Revoke one gateway JWT id for a target profile until its original token expiration.
gateway-revoke-jwt jwt_id expires_at issuer='https://veoveo.enterprise.example/oauth' admin_profile='admin' target_profile='operator' reason='operator_request':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --client-id admin-service --scope operator:use --scope admin:manage)"; payload="$(jq -n --arg profile '{{target_profile}}' --arg issuer '{{issuer}}' --arg jwt_id '{{jwt_id}}' --arg expires_at '{{expires_at}}' --arg reason '{{reason}}' '{profile: $profile, issuer: $issuer, jwt_id: $jwt_id, expires_at: $expires_at, reason: $reason}')"; curl -fsS -X POST -H "Authorization: Bearer ${token}" -H "Content-Type: application/json" --data "${payload}" "{{gateway-admin-url}}/{{admin_profile}}/jwt-revocations"

# Remove expired gateway JWT revocation entries.
gateway-prune-revoked-jwts profile='admin':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --client-id admin-service --scope operator:use --scope admin:manage)"; curl -fsS -X POST -H "Authorization: Bearer ${token}" "{{gateway-admin-url}}/{{profile}}/jwt-revocations/prune"

# Smoke-test gateway contract/control-plane behavior without external services.
smoke-gateway:
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} gateway-suite --control-plane {{gateway-control-plane}} --smoke-control-plane {{gateway-smoke-control-plane}}

# Smoke-test gateway bootstrap against the real platform store.
smoke-gateway-platform-store:
    cargo build -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-platform-store --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

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

# Smoke-test gateway projection for server-owned chart resources.
smoke-gateway-chart-projection:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-chart-projection --conformance-bin target/debug/conformance --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

smoke-gateway-console-stream:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway
    {{smoke}} gateway-console-stream --conformance-bin target/debug/conformance --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}}

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

# Smoke-test the Pilot agent's full mission loop over frames and optimization.
smoke-agent-pilot:
    cargo build -p veoveo-frames-mcp --bin server
    cp target/debug/server target/debug/frames-mcp-smoke
    cargo build -p veoveo-optimization-mcp --bin server
    cp target/debug/server target/debug/optimization-mcp-smoke
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-mcp-gateway --bin gateway -p veoveo-artifact-service --bin artifact-service -p veoveo-agent-kernel --bin agent
    {{smoke}} agent-pilot --conformance-bin target/debug/conformance --frames-bin target/debug/frames-mcp-smoke --optimization-bin target/debug/optimization-mcp-smoke --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}} --artifact-service-bin target/debug/artifact-service --agent-bin target/debug/agent

# Run the real Pilot against the active local k3d stack with Cloudflare credentials from .env.
agent-pilot-local data_dir="output/pilot-data":
    cargo build -p veoveo-agent-kernel --bin agent
    PILOT_GATEWAY_URL="${PILOT_GATEWAY_URL:-http://localhost:8780}" target/debug/agent run --manifest configs/agents/pilot/manifest.json --data-dir {{data_dir}} --viewer-tee rerun+http://127.0.0.1:9877/proxy

# Smoke-test a continuously-running agent sleeping on a long gateway task and waking from its completion push.
smoke-agent-sleep-wake:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server -p veoveo-mcp-gateway --bin gateway -p veoveo-artifact-service --bin artifact-service -p veoveo-agent-kernel --bin agent
    {{smoke}} agent-sleep-wake --conformance-bin target/debug/conformance --media-bin target/debug/server --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}} --artifact-service-bin target/debug/artifact-service --agent-bin target/debug/agent

# The real deal: the sleep/wake smoke with the REAL model from CLOUDFLARE_ACCOUNT_ID/CLOUDFLARE_API_TOKEN (override model with AGENT_LIVE_MODEL).
smoke-agent-live:
    cargo build -p veoveo-mcp-conformance --bin conformance -p veoveo-smoke --bin smoke -p veoveo-media-mcp --bin server -p veoveo-mcp-gateway --bin gateway -p veoveo-artifact-service --bin artifact-service -p veoveo-agent-kernel --bin agent
    {{smoke}} agent-sleep-wake --live --conformance-bin target/debug/conformance --media-bin target/debug/server --gateway-bin target/debug/gateway --control-plane {{gateway-smoke-control-plane}} --artifact-service-bin target/debug/artifact-service --agent-bin target/debug/agent

# Build the current GPU-enabled K3s node image used by k3d.
k3d-node-build:
    source deploy/local/k3d/versions.env; docker build --build-arg K3S_VERSION="$K3S_VERSION" --build-arg CUDA_VERSION="$CUDA_VERSION" --build-arg NVIDIA_CONTAINER_TOOLKIT_VERSION="$NVIDIA_CONTAINER_TOOLKIT_VERSION" -t "$VEOVEO_K3D_NODE_IMAGE" deploy/local/k3d/node

# Create the loopback-only SUMO development cluster.
sumo-k3d-create:
    {{k3d}} cluster create --config deploy/local/k3d/cluster.yaml

# Delete the SUMO development cluster and its Kubernetes state.
sumo-k3d-delete:
    {{k3d}} cluster delete veoveo-sumo

# Show the SUMO cluster and workload status.
sumo-k3d-status:
    {{k3d}} cluster list
    {{kubectl}} --context {{sumo-kube-context}} -n veoveo get pods,services

# Follow one Kubernetes deployment.
logs workload='mcp-gateway' context=sumo-kube-context:
    {{kubectl}} --context '{{context}}' -n veoveo logs -f deployment/{{workload}} --all-containers --tail=200

# Create the Bioma cluster.
bioma-k3d-create:
    {{k3d}} cluster create --config examples/bioma/k3d.yaml

# Delete the Bioma cluster and its Kubernetes state.
bioma-k3d-delete:
    {{k3d}} cluster delete veoveo-bioma

# Show both isolated clusters and their workloads.
clusters-status:
    {{k3d}} cluster list
    {{kubectl}} --context {{sumo-kube-context}} -n veoveo get pods
    {{kubectl}} --context {{bioma-kube-context}} -n veoveo get pods

# Build the complete Veoveo installation used by Bioma.
bioma-build:
    docker buildx bake bioma

# Import the complete Veoveo installation into the Bioma cluster.
bioma-import:
    {{k3d}} image import --cluster veoveo-bioma {{bioma-images}}

# Apply Bioma's control plane and environment-backed Kubernetes Secrets.
bioma-resources:
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} bioma-resources --context {{bioma-kube-context}}

# Install the Bioma-owned platform release in its isolated cluster.
bioma-platform-up:
    {{helm}} --kube-context {{bioma-kube-context}} upgrade --install veoveo deploy/helm/veoveo --namespace veoveo --create-namespace --values examples/bioma/values.yaml --values examples/bioma/k3d-values.yaml --wait --timeout 12m

# Install Bioma with canonical-host TLS for direct LAN and split-horizon DNS access.
bioma-platform-up-lan:
    {{helm}} --kube-context {{bioma-kube-context}} upgrade --install veoveo deploy/helm/veoveo --namespace veoveo --create-namespace --values examples/bioma/values.yaml --values examples/bioma/k3d-values.yaml --values examples/bioma/lan-values.yaml --wait --timeout 12m

# Connect the Bioma cluster to its remote-managed Cloudflare Tunnel.
bioma-tunnel-up:
    {{kubectl}} --context {{bioma-kube-context}} -n veoveo apply -f examples/bioma/tunnel.yaml
    {{kubectl}} --context {{bioma-kube-context}} -n veoveo rollout status deployment/cloudflared --timeout=5m

# Verify the Bioma installation and its authoritative public edge.
bioma-verify:
    cargo build -p veoveo-smoke --bin smoke
    {{smoke}} bioma-verify --context {{bioma-kube-context}}

# Check local health and, optionally, the operator-owned public edge.
health public_base_url='':
    curl -fsS http://localhost:8780/healthz
    @echo
    if [ -n '{{public_base_url}}' ]; then curl -fsS '{{public_base_url}}/healthz'; echo; fi

# Mint a configured service access token for the operator profile.
gateway-token scope='operator:use':
    {{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope '{{scope}}'

# Show gateway MCP server info and resource templates.
info:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} info

# List models through the gateway, optionally with a local query string.
models query='':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; if [ -n '{{query}}' ]; then env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} models '{{query}}'; else env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} models; fi

# Complete model ids by prefix through the gateway.
complete prefix:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} complete '{{prefix}}'

# Read one model schema through the gateway.
schema model:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} schema '{{model}}'

# Run an arbitrary model through the gateway with a raw JSON input object.
run model input output_dir='output':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} run '{{model}}' --tool-name media__run --input '{{input}}' --output-dir '{{output_dir}}'

# Run the default image edit e2e against the public base URL and save returned artifacts.
run-edit public_base_url output_dir='output/e2e':
    input="{\"prompt\":\"add a red wizard hat\",\"images\":[\"{{public_base_url}}/media/files/{{default-input-image}}\"]}"; token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} run '{{default-model}}' --tool-name media__run --input "$input" --output-dir '{{output_dir}}'

# Read one gateway task usage report.
usage task_id:
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} usage '{{task_id}}'

# Read and save one artifact occurrence through the gateway.
artifact artifact_id output_dir='output':
    token="$({{conformance}} gateway-token-exchange --token-url {{gateway-token-url}} --scope operator:use)"; env -u VEOVEO_INTERNAL_SIGNING_KEY_DER_B64 MCP_BEARER_TOKEN="$token" {{conformance}} --url {{mcp-url}} artifact '{{artifact_id}}' --output-dir '{{output_dir}}'

# Check the active stack, print MCP info, and run the default edit task.
e2e public_base_url output_dir='output/e2e':
    just health '{{public_base_url}}'
    just info
    just run-edit '{{public_base_url}}' '{{output_dir}}'

# Unit-test the SUMO showcase MCP server (fake driver, no SUMO needed).
showcase-sumo-test:
    cargo test -p veoveo-sumo-mcp

# Push smoke: SUMO sim (fake driver) pushes world state into the real hub.
showcase-sumo-smoke:
    cargo run -p veoveo-smoke -- sumo-push

# Build the platform and SUMO images for the showcase profile.
showcase-sumo-build:
    docker buildx bake sumo-showcase

# Import the showcase images into the active k3d node.
showcase-sumo-import:
    {{k3d}} image import --cluster veoveo-sumo veoveo/mcp-gateway:0.1.0 veoveo/artifact-service:0.1.0 veoveo/recording-hub:0.1.0 veoveo/recording-mcp:0.1.0 veoveo/console-bff:0.1.0
    docker save veoveo/sumo-sim:1.27.1 veoveo/sumo-mcp:0.1.0 | docker exec -i k3d-veoveo-sumo-server-0 ctr -n k8s.io images import -

# Apply disposable credentials and the SUMO-owned gateway profile.
showcase-sumo-resources:
    {{kubectl}} --context {{sumo-kube-context}} apply -f deploy/local/k3d/development-resources.yaml
    {{kubectl}} --context {{sumo-kube-context}} -n veoveo create configmap veoveo-gateway-control-plane --from-file=gateway.json=showcase/sumo/deploy/gateway.json --from-file=jwks.json=showcase/sumo/deploy/jwks.json --dry-run=client -o yaml | {{kubectl}} --context {{sumo-kube-context}} apply -f -

# Install the local platform with only the services needed by SUMO.
showcase-sumo-platform-up:
    {{helm}} --kube-context {{sumo-kube-context}} upgrade --install veoveo deploy/helm/veoveo --namespace veoveo --create-namespace --values deploy/local/k3d/values.yaml --values showcase/sumo/deploy/platform-values.yaml --wait --timeout 12m

# Bring up the full SUMO showcase (SUMO + sumo-mcp + hub).
showcase-sumo-up:
    {{helm}} --kube-context {{sumo-kube-context}} upgrade --install sumo showcase/sumo/deploy/helm --namespace veoveo --wait --timeout 12m

# Stop only the SUMO profile while retaining the local platform.
showcase-sumo-down:
    {{helm}} --kube-context {{sumo-kube-context}} uninstall sumo --namespace veoveo

# Launch Rerun against the loopback Recording Hub projection.
showcase-sumo-view:
    test -n "${MAPBOX_ACCESS_TOKEN:-}"; RERUN_MAPBOX_ACCESS_TOKEN="$MAPBOX_ACCESS_TOKEN" rerun --connect rerun+http://127.0.0.1:9877/proxy

# End-to-end verify: full SUMO showcase up, world durable in hub, served MCP driven e2e.
showcase-sumo-verify:
    cargo run -p veoveo-smoke -- sumo-verify --context {{sumo-kube-context}}
