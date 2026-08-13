variable "REGISTRY" {
  default = ""
}

variable "REPOSITORY_PREFIX" {
  default = "extensions"
}

variable "SOURCE_TAG" {
  default = "0.1.0"
}

function "image_ref" {
  params = [name]
  result = join("", [
    REGISTRY != "" ? "${REGISTRY}/" : "",
    REPOSITORY_PREFIX != "" ? "${REPOSITORY_PREFIX}/" : "",
    "${name}:${SOURCE_TAG}",
  ])
}

group "anonymous-simulation-extension" {
  targets = ["anonymous-simulation-mcp"]
}

target "anonymous-simulation-mcp" {
  context    = "."
  dockerfile = "Dockerfile"
  tags       = [image_ref("anonymous-simulation-mcp")]
  labels = {
    "org.opencontainers.image.title"       = "Anonymous external simulation MCP server"
    "org.opencontainers.image.description" = "Simulator-hosted live-view contract and packaging fixture"
    "io.veoveo.extension.role"              = "simulation-mcp-server"
  }
}
