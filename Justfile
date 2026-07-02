set shell := ["bash", "-euo", "pipefail", "-c"]
set dotenv-load := true

compose := "docker compose -f compose.yaml -f compose.tunnel.yaml --profile dev --profile tunnel"
mcp-url := "http://localhost:8787/media/mcp"
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

# Build the media MCP image.
compose-build:
    {{compose}} build media-mcp

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

# Run the default image edit e2e against the public base URL and save returned artifacts.
run-edit public_base_url output_dir='output/e2e':
    input="{\"prompt\":\"add a red wizard hat\",\"images\":[\"{{public_base_url}}/media/files/{{default-input-image}}\"]}"; cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} run '{{default-model}}' --input "$input" --output-dir '{{output_dir}}'

# Read one task usage report.
usage task_id:
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} usage '{{task_id}}'

# Read and save one artifact by sha256.
artifact sha256 output_dir='output':
    cargo run -p veoveo-mcp-contract --bin conformance -- --url {{mcp-url}} artifact '{{sha256}}' --output-dir '{{output_dir}}'

# Start the stack, check health, print MCP info, and run the default edit task.
e2e public_base_url output_dir='output/e2e':
    just compose-up
    just health '{{public_base_url}}'
    just info
    just run-edit '{{public_base_url}}' '{{output_dir}}'
