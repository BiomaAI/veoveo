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
    "org.opencontainers.image.title"       = "Anonymous external Simulation View producer"
    "org.opencontainers.image.description" = "Fixture-owned scene assets and typed latest-pose publication"
    "io.veoveo.extension.role"              = "simulation-producer"
  }
}
