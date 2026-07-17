group "default" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-forwarder",
    "recording-hub",
    "recording-mcp",
    "console-bff",
  ]
}

group "platform" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-forwarder",
    "recording-hub",
    "recording-mcp",
    "console-bff",
  ]
}

group "bioma" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-forwarder",
    "recording-hub",
    "recording-mcp",
    "console-bff",
    "artifact-mcp",
    "media-mcp",
    "perception-mcp",
    "timeseries-mcp",
    "duckdb-mcp",
    "optimization-mcp",
    "frames-mcp",
    "map-mcp",
    "view-mcp",
    "time-mcp",
    "datasheet-mcp",
    "chart-mcp",
    "mcp-stdio-bridge",
  ]
}

group "sumo-showcase" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-forwarder",
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

target "recording-forwarder" {
  inherits   = ["base"]
  dockerfile = "platform/recordings/forwarder/Dockerfile"
  tags       = ["veoveo/recording-forwarder:0.1.0"]
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

target "artifact-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/artifact-mcp/Dockerfile"
  tags       = ["veoveo/artifact-mcp:0.1.0"]
}

target "media-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/media-mcp/Dockerfile"
  tags       = ["veoveo/media-mcp:0.1.0"]
}

target "perception-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/perception-mcp/Dockerfile"
  tags       = ["veoveo/perception-mcp:0.1.0"]
}

target "timeseries-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/timeseries-mcp/Dockerfile"
  tags       = ["veoveo/timeseries-mcp:0.1.0"]
}

target "duckdb-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/duckdb-mcp/Dockerfile"
  tags       = ["veoveo/duckdb-mcp:0.1.0"]
}

target "optimization-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/optimization-mcp/Dockerfile"
  tags       = ["veoveo/optimization-mcp:0.1.0"]
}

target "frames-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/frames-mcp/Dockerfile"
  tags       = ["veoveo/frames-mcp:0.1.0"]
}

target "map-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/map-mcp/Dockerfile"
  tags       = ["veoveo/map-mcp:0.1.0"]
}

target "view-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/view-mcp/Dockerfile"
  tags       = ["veoveo/view-mcp:0.1.0"]
}

target "time-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/time-mcp/Dockerfile"
  tags       = ["veoveo/time-mcp:0.1.0"]
}

target "datasheet-mcp" {
  inherits   = ["base"]
  dockerfile = "templates/python-mcp/Dockerfile"
  tags       = ["veoveo/datasheet-mcp:0.1.0"]
}

target "chart-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/chart-mcp/Dockerfile"
  tags       = ["veoveo/chart-mcp:0.1.0"]
}

target "mcp-stdio-bridge" {
  inherits   = ["base"]
  dockerfile = "mcp/bridges/stdio/Dockerfile"
  tags       = ["veoveo/mcp-stdio-bridge:0.1.0"]
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

target "uav-sim-base" {
  context    = "showcase/uav-sim/runtime"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  target     = "runtime-base"
  tags       = ["veoveo/uav-sim-base:isaac-6.0.1-cesium-0.29.0-pegasus-5.1.0-px4-1.17.0"]
}

target "uav-sim-runtime" {
  context    = "showcase/uav-sim/runtime"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  target     = "runtime"
  tags       = ["veoveo/uav-sim-runtime:6.0.1"]
  args = {
    UAV_SIM_BASE_IMAGE = "veoveo/uav-sim-base:isaac-6.0.1-cesium-0.29.0-pegasus-5.1.0-px4-1.17.0"
  }
}

target "uav-sim-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/uav-sim-mcp/Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = ["veoveo/uav-sim-mcp:0.1.0"]
}
