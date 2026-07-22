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

variable "VEOVEO_REGISTRY" {
  default = ""
}

variable "VEOVEO_IMAGE_TAG" {
  default = "0.1.0"
}

function "image_ref" {
  params = [name]
  result = format(
    "%sveoveo/%s:%s",
    VEOVEO_REGISTRY != "" ? format("%s/", VEOVEO_REGISTRY) : "",
    name,
    VEOVEO_IMAGE_TAG,
  )
}

group "platform-core" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-forwarder",
    "recording-hub",
    "recording-mcp",
    "console-bff",
  ]
}

group "platform-full" {
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
    "reason-mcp",
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

group "showcase-sumo" {
  targets = [
    "sumo-sim",
    "sumo-mcp",
  ]
}

group "showcase-uav-sim" {
  targets = [
    "uav-sim-runtime",
    "uav-sim-mcp",
  ]
}

target "base" {
  context = "."
}

target "mcp-gateway" {
  inherits   = ["base"]
  dockerfile = "platform/gateway/Dockerfile"
  tags       = [image_ref("mcp-gateway")]
}

target "artifact-service" {
  inherits   = ["base"]
  dockerfile = "platform/artifacts/service/Dockerfile"
  tags       = [image_ref("artifact-service")]
}

target "recording-forwarder" {
  inherits   = ["base"]
  dockerfile = "platform/recordings/forwarder/Dockerfile"
  tags       = [image_ref("recording-forwarder")]
}

target "recording-hub" {
  inherits   = ["base"]
  dockerfile = "platform/recordings/hub/Dockerfile"
  tags       = [image_ref("recording-hub")]
}

target "recording-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/recording-mcp/Dockerfile"
  tags       = [image_ref("recording-mcp")]
}

target "console-bff" {
  inherits   = ["base"]
  dockerfile = "apps/console/bff/Dockerfile"
  tags       = [image_ref("console-bff")]
}

target "artifact-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/artifact-mcp/Dockerfile"
  tags       = [image_ref("artifact-mcp")]
}

target "media-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/media-mcp/Dockerfile"
  tags       = [image_ref("media-mcp")]
}

target "perception-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/perception-mcp/Dockerfile"
  tags       = [image_ref("perception-mcp")]
}

target "reason-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/reason-mcp/Dockerfile"
  tags       = [image_ref("reason-mcp")]
}

target "timeseries-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/timeseries-mcp/Dockerfile"
  tags       = [image_ref("timeseries-mcp")]
}

target "duckdb-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/duckdb-mcp/Dockerfile"
  tags       = [image_ref("duckdb-mcp")]
}

target "optimization-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/optimization-mcp/Dockerfile"
  tags       = [image_ref("optimization-mcp")]
}

target "frames-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/frames-mcp/Dockerfile"
  tags       = [image_ref("frames-mcp")]
}

target "map-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/map-mcp/Dockerfile"
  tags       = [image_ref("map-mcp")]
}

target "view-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/view-mcp/Dockerfile"
  tags       = [image_ref("view-mcp")]
}

target "time-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/time-mcp/Dockerfile"
  tags       = [image_ref("time-mcp")]
}

target "datasheet-mcp" {
  inherits   = ["base"]
  dockerfile = "templates/python-mcp/Dockerfile"
  tags       = [image_ref("datasheet-mcp")]
}

target "chart-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/chart-mcp/Dockerfile"
  tags       = [image_ref("chart-mcp")]
}

target "mcp-stdio-bridge" {
  inherits   = ["base"]
  dockerfile = "mcp/bridges/stdio/Dockerfile"
  tags       = [image_ref("mcp-stdio-bridge")]
}

target "sumo-sim" {
  context    = "showcase/sumo/sim"
  dockerfile = "Dockerfile"
  tags       = [image_ref("sumo-sim")]
}

target "sumo-mcp" {
  inherits   = ["base"]
  dockerfile = "showcase/sumo/sumo-mcp/Dockerfile"
  tags       = [image_ref("sumo-mcp")]
}

target "uav-sim-base" {
  context    = "showcase/uav-sim/runtime"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  target     = "runtime-base"
  tags       = [image_ref("uav-sim-base")]
}

target "uav-sim-runtime" {
  context    = "showcase/uav-sim/runtime"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  target     = "runtime"
  tags       = [image_ref("uav-sim-runtime")]
  contexts = {
    uav-sim-base = "target:uav-sim-base"
  }
  args = {
    UAV_SIM_BASE_IMAGE = "uav-sim-base"
  }
}

target "uav-sim-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/uav-sim-mcp/Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = [image_ref("uav-sim-mcp")]
}
