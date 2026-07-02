set shell := ["bash", "-euo", "pipefail", "-c"]

compose-env := 'set -a; [ ! -f .env ] || . ./.env; set +a; : "${MEDIA_PROVIDER_API_KEY:?set MEDIA_PROVIDER_API_KEY}"; export MEDIA_PROVIDER_API_KEY; export PUBLIC_URL="${PUBLIC_URL:-}"; export MEDIA_PROVIDER_WEBHOOK_SECRET="${MEDIA_PROVIDER_WEBHOOK_SECRET:-}"'
tunnel-compose-env := 'set -a; [ ! -f .env ] || . ./.env; set +a; : "${MEDIA_PROVIDER_API_KEY:?set MEDIA_PROVIDER_API_KEY}"; : "${PUBLIC_URL:?set PUBLIC_URL}"; : "${CLOUDFLARED_TUNNEL_TOKEN:?set CLOUDFLARED_TUNNEL_TOKEN}"; export MEDIA_PROVIDER_API_KEY PUBLIC_URL CLOUDFLARED_TUNNEL_TOKEN; export MEDIA_PROVIDER_WEBHOOK_SECRET="${MEDIA_PROVIDER_WEBHOOK_SECRET:-}"'
mcp-url := "http://localhost:8787/mcp"
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

# Render the dev Compose config with secrets redacted.
compose-config public_url='':
    {{compose-env}}; if [ -n '{{public_url}}' ]; then export PUBLIC_URL='{{public_url}}'; fi; : "${PUBLIC_URL:?set PUBLIC_URL}"; docker compose --profile dev config | sed -E 's/(MEDIA_PROVIDER_API_KEY: ).*/\1[redacted]/; s/(MEDIA_PROVIDER_WEBHOOK_SECRET: ).*/\1[redacted]/; s/(AWS_SECRET_ACCESS_KEY: ).*/\1[redacted]/'

# Build the media MCP image.
compose-build:
    docker compose --profile dev build media-mcp

# Build and start RustFS plus media-mcp. Pass a public tunnel URL for real provider webhooks.
compose-up public_url='':
    {{compose-env}}; if [ -n '{{public_url}}' ]; then export PUBLIC_URL='{{public_url}}'; fi; : "${PUBLIC_URL:?set PUBLIC_URL}"; docker compose --profile dev up --build -d

# Build and start RustFS plus media-mcp plus a named Cloudflare tunnel.
compose-up-named-tunnel:
    {{tunnel-compose-env}}; docker compose -f compose.yaml -f compose.tunnel.yaml --profile dev --profile tunnel up --build -d

# Stop the dev Compose stack.
compose-down:
    {{compose-env}}; docker compose --profile dev down --remove-orphans

# Stop the dev stack plus named tunnel.
compose-down-named-tunnel:
    {{tunnel-compose-env}}; docker compose -f compose.yaml -f compose.tunnel.yaml --profile dev --profile tunnel down --remove-orphans

# Stop the dev Compose stack and remove its volumes.
compose-down-volumes:
    {{compose-env}}; docker compose --profile dev down --remove-orphans --volumes

# Show Compose service status.
compose-ps:
    {{compose-env}}; docker compose --profile dev ps

# Follow logs for one service.
logs service='media-mcp':
    {{compose-env}}; docker compose --profile dev logs -f --tail=200 {{service}}

# Start a Cloudflare quick tunnel for the media server.
tunnel protocol='http2':
    cloudflared tunnel --config /dev/null --protocol {{protocol}} --url http://127.0.0.1:8787

# Check local health and, optionally, public tunnel health.
health public_url='':
    curl -fsS http://localhost:8787/healthz
    @echo
    if [ -n '{{public_url}}' ]; then curl -fsS '{{public_url}}/healthz'; echo; fi

# Show MCP server info and resource templates.
info:
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} info

# List models, optionally with a local query string.
models query='':
    if [ -n '{{query}}' ]; then cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} models '{{query}}'; else cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} models; fi

# Complete model ids by prefix.
complete prefix:
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} complete '{{prefix}}'

# Read one model schema.
schema model:
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} schema '{{model}}'

# Run an arbitrary model with a raw JSON input object.
run model input output_dir='output':
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} run '{{model}}' --input '{{input}}' --output-dir '{{output_dir}}'

# Run the default image edit e2e against the public URL and save returned artifacts.
run-edit public_url output_dir='output/e2e':
    input="{\"prompt\":\"add a red wizard hat\",\"images\":[\"{{public_url}}/files/{{default-input-image}}\"]}"; cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} run '{{default-model}}' --input "$input" --output-dir '{{output_dir}}'

# Read one task usage report.
usage task_id:
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} usage '{{task_id}}'

# Read and save one artifact by sha256.
artifact sha256 output_dir='output':
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} artifact '{{sha256}}' --output-dir '{{output_dir}}'

# Start the stack, check health, print MCP info, and run the default edit task.
e2e public_url output_dir='output/e2e':
    just compose-up '{{public_url}}'
    just health '{{public_url}}'
    just info
    just run-edit '{{public_url}}' '{{output_dir}}'
