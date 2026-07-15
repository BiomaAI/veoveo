group "default" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-hub",
    "recording-mcp",
    "console-bff",
  ]
}

group "platform" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-hub",
    "recording-mcp",
    "console-bff",
  ]
}

group "sumo-showcase" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-hub",
    "recording-mcp",
    "console-bff",
    "sumo-sim",
    "sumo-mcp",
  ]
}

target "base" {
  context = "."
}

target "mcp-gateway" {
  inherits   = ["base"]
  dockerfile = "platform/gateway/Dockerfile"
  tags       = ["veoveo/mcp-gateway:0.1.0"]
}

target "artifact-service" {
  inherits   = ["base"]
  dockerfile = "platform/artifacts/service/Dockerfile"
  tags       = ["veoveo/artifact-service:0.1.0"]
}

target "recording-hub" {
  inherits   = ["base"]
  dockerfile = "platform/recordings/hub/Dockerfile"
  tags       = ["veoveo/recording-hub:0.1.0"]
}

target "recording-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/recording-mcp/Dockerfile"
  tags       = ["veoveo/recording-mcp:0.1.0"]
}

target "console-bff" {
  inherits   = ["base"]
  dockerfile = "apps/console/bff/Dockerfile"
  tags       = ["veoveo/console-bff:0.1.0"]
}

target "sumo-sim" {
  context    = "showcase/sumo/sim"
  dockerfile = "Dockerfile"
  tags       = ["veoveo/sumo-sim:1.27.1"]
}

target "sumo-mcp" {
  inherits   = ["base"]
  dockerfile = "showcase/sumo/sumo-mcp/Dockerfile"
  tags       = ["veoveo/sumo-mcp:0.1.0"]
}
