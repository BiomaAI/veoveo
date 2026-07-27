group "default" {
  targets = ["platform-core"]
}

variable "VEOVEO_REGISTRY" {
  default = ""
}

variable "VEOVEO_IMAGE_TAG" {
  default = "0.1.0"
}

variable "RUST_TRIXIE_IMAGE" {
  default = "docker.io/library/rust:1.97.1-slim-trixie@sha256:5c6f46a6e4472ab1ca7ba7d494e6677f2f219ebc02f32025d3986f057635ec9c"
}

variable "RUST_BOOKWORM_IMAGE" {
  default = "docker.io/library/rust:1.97.1-slim-bookworm@sha256:99e09cb2284e2ddbb73a995deee3e91783fd04d177602ccf6eab326d778ee777"
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
    "simulation-runtime",
    "simulation-view-mcp",
    "simulation-view-pose",
    "simulation-view-isaac",
  ]
}

group "external-extension-platform" {
  targets = [
    "mcp-gateway",
    "artifact-service",
    "recording-forwarder",
    "recording-hub",
    "recording-mcp",
    "artifact-mcp",
    "frames-mcp",
    "map-mcp",
    "media-mcp",
  ]
}

group "showcase-sumo" {
  targets = ["sumo-sim", "sumo-mcp"]
}

group "showcase-sumo-base" {
  targets = ["sumo-base"]
}

group "showcase-uav-sim" {
  targets = ["uav-sim-runtime", "uav-sim-mcp"]
}

group "simulation-runtime" {
  targets = ["simulation-runtime"]
}

group "simulation-view" {
  targets = [
    "simulation-runtime",
    "simulation-view-mcp",
    "simulation-view-pose",
    "simulation-view-isaac",
  ]
}

group "showcase-uav-sim-overlay-acceptance" {
  targets = ["uav-sim-runtime", "simulation-overlay-acceptance"]
}

group "extension-support" {
  targets = ["mcp-conformance", "gateway-composer"]
}

target "base" {
  context   = "."
  platforms = ["linux/amd64"]
}

target "rust-trixie-artifacts" {
  inherits   = ["base"]
  dockerfile = "tools/image-build/rust-workspace.Dockerfile"
  target     = "artifacts"
  args = {
    RUST_IMAGE             = RUST_TRIXIE_IMAGE
    VEOVEO_CARGO_PACKAGES  = ""
    VEOVEO_CARGO_BINARIES  = ""
    VEOVEO_AUXILIARY       = ""
    VEOVEO_TARGET_CACHE_ID = ""
  }
}

target "rust-bookworm-artifacts" {
  inherits   = ["base"]
  dockerfile = "tools/image-build/rust-workspace.Dockerfile"
  target     = "artifacts"
  args = {
    RUST_IMAGE             = RUST_BOOKWORM_IMAGE
    VEOVEO_CARGO_PACKAGES  = ""
    VEOVEO_CARGO_BINARIES  = ""
    VEOVEO_AUXILIARY       = ""
    VEOVEO_TARGET_CACHE_ID = ""
  }
}

target "_rust-trixie-runtime" {
  inherits = ["base"]
  contexts = {
    veoveo-rust-artifacts = "target:rust-trixie-artifacts"
  }
}

target "_rust-bookworm-runtime" {
  inherits = ["base"]
  contexts = {
    veoveo-rust-artifacts = "target:rust-bookworm-artifacts"
  }
}

target "mcp-gateway" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "platform/gateway/Dockerfile"
  tags       = [image_ref("mcp-gateway")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-mcp-gateway"
    "io.veoveo.build.binaries"  = "gateway"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = "libduckdb"
  }
}

target "artifact-service" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "platform/artifacts/service/Dockerfile"
  tags       = [image_ref("artifact-service")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-artifact-service"
    "io.veoveo.build.binaries"  = "artifact-service"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "recording-forwarder" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "platform/recordings/forwarder/Dockerfile"
  tags       = [image_ref("recording-forwarder")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-recording-forwarder"
    "io.veoveo.build.binaries"  = "recording-forwarder"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "recording-hub" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "platform/recordings/hub/Dockerfile"
  tags       = [image_ref("recording-hub")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-recording-hub"
    "io.veoveo.build.binaries"  = "spooler,sensor-sim,hub-query"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "recording-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/recording-mcp/Dockerfile"
  tags       = [image_ref("recording-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-recording-mcp"
    "io.veoveo.build.binaries"  = "recording-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "console-bff" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "apps/console/bff/Dockerfile"
  tags       = [image_ref("console-bff")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-console-bff"
    "io.veoveo.build.binaries"  = "console-bff"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "artifact-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/artifact-mcp/Dockerfile"
  tags       = [image_ref("artifact-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-artifact-mcp"
    "io.veoveo.build.binaries"  = "artifact-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "media-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/media-mcp/Dockerfile"
  tags       = [image_ref("media-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-media-mcp"
    "io.veoveo.build.binaries"  = "media-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "timeseries-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/timeseries-mcp/Dockerfile"
  tags       = [image_ref("timeseries-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-timeseries-mcp"
    "io.veoveo.build.binaries"  = "timeseries-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = "libduckdb"
  }
}

target "duckdb-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/duckdb-mcp/Dockerfile"
  tags       = [image_ref("duckdb-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-duckdb-mcp"
    "io.veoveo.build.binaries"  = "duckdb-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = "libduckdb,duckdb-spatial"
  }
}

target "optimization-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/optimization-mcp/Dockerfile"
  tags       = [image_ref("optimization-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-optimization-mcp"
    "io.veoveo.build.binaries"  = "optimization-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = "libduckdb"
  }
}

target "frames-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/frames-mcp/Dockerfile"
  tags       = [image_ref("frames-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-frames-mcp"
    "io.veoveo.build.binaries"  = "frames-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "mcp-stdio-bridge" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "mcp/bridges/stdio/Dockerfile"
  tags       = [image_ref("mcp-stdio-bridge")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-mcp-stdio-bridge"
    "io.veoveo.build.binaries"  = "bridge"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "mcp-conformance" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "mcp/conformance/Dockerfile"
  tags       = [image_ref("mcp-conformance")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-mcp-conformance"
    "io.veoveo.build.binaries"  = "certify"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "gateway-composer" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "mcp/composer/Dockerfile"
  tags       = [image_ref("gateway-composer")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-gateway-composer"
    "io.veoveo.build.binaries"  = "gateway-compose"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "agent-kernel" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "agents/kernel/Dockerfile"
  tags       = [image_ref("agent-kernel")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-agent-kernel"
    "io.veoveo.build.binaries"  = "agent"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = "libduckdb"
  }
}

target "map-mcp" {
  inherits   = ["_rust-bookworm-runtime"]
  dockerfile = "servers/map-mcp/Dockerfile"
  tags       = [image_ref("map-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-map-mcp"
    "io.veoveo.build.binaries"  = "map-mcp"
    "io.veoveo.build.family"    = "rust-bookworm-v1"
    "io.veoveo.build.auxiliary" = "libduckdb,duckdb-spatial"
  }
}

target "time-mcp" {
  inherits   = ["_rust-bookworm-runtime"]
  dockerfile = "servers/time-mcp/Dockerfile"
  tags       = [image_ref("time-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-time-mcp"
    "io.veoveo.build.binaries"  = "time-mcp"
    "io.veoveo.build.family"    = "rust-bookworm-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "view-mcp" {
  inherits   = ["_rust-bookworm-runtime"]
  dockerfile = "servers/view-mcp/Dockerfile"
  tags       = [image_ref("view-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-view-mcp"
    "io.veoveo.build.binaries"  = "view-mcp"
    "io.veoveo.build.family"    = "rust-bookworm-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "perception-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/perception-mcp/Dockerfile"
  tags       = [image_ref("perception-mcp")]
  args = {
    VEOVEO_TARGET_CACHE_ID = ""
  }
  labels = {
    "io.veoveo.build.mode"      = "rust-standalone"
    "io.veoveo.build.package"   = "veoveo-perception-mcp"
    "io.veoveo.build.binaries"  = "perception-mcp"
    "io.veoveo.build.family"    = "rust-deepstream-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "reason-mcp" {
  inherits   = ["base"]
  dockerfile = "servers/reason-mcp/Dockerfile"
  tags       = [image_ref("reason-mcp")]
  args = {
    VEOVEO_TARGET_CACHE_ID = ""
  }
  labels = {
    "io.veoveo.build.mode"      = "rust-standalone"
    "io.veoveo.build.package"   = "veoveo-reason-mcp"
    "io.veoveo.build.binaries"  = "reason-mcp"
    "io.veoveo.build.family"    = "rust-vllm-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "datasheet-mcp" {
  inherits   = ["base"]
  dockerfile = "templates/python-mcp/Dockerfile"
  tags       = [image_ref("datasheet-mcp")]
}

target "chart-mcp" {
  inherits   = ["base"]
  context    = "servers/chart-mcp"
  dockerfile = "Dockerfile"
  tags       = [image_ref("chart-mcp")]
}

target "sumo-sim" {
  context    = "showcase/sumo/sim"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = [image_ref("sumo-sim")]
  contexts = {
    sumo-base = "target:sumo-base"
  }
  args = {
    SUMO_BASE_IMAGE = "sumo-base"
  }
}

target "sumo-mcp" {
  inherits   = ["base"]
  dockerfile = "showcase/sumo/sumo-mcp/Dockerfile"
  tags       = [image_ref("sumo-mcp")]
  contexts = {
    sumo-base = "target:sumo-base"
  }
  args = {
    SUMO_BASE_IMAGE         = "sumo-base"
    VEOVEO_TARGET_CACHE_ID = ""
  }
  labels = {
    "io.veoveo.build.mode"      = "rust-standalone"
    "io.veoveo.build.package"   = "veoveo-sumo-mcp"
    "io.veoveo.build.binaries"  = "sumo-mcp"
    "io.veoveo.build.family"    = "rust-sumo-bullseye-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "sumo-base" {
  context    = "showcase/sumo/base"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = [image_ref("sumo-base")]
}

target "simulation-runtime" {
  context    = "platform/runtimes/simulation"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = [image_ref("simulation-runtime")]
}

target "simulation-view-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/simulation-view-mcp/Dockerfile"
  tags       = [image_ref("simulation-view-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-simulation-view-mcp"
    "io.veoveo.build.binaries"  = "simulation-view-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "simulation-view-pose" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "platform/simulation/pose-ingress/Dockerfile"
  tags       = [image_ref("simulation-view-pose")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-simulation-view-pose-ingress"
    "io.veoveo.build.binaries"  = "simulation-view-pose"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}

target "simulation-view-isaac" {
  context    = "platform/simulation/view-isaac"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = [image_ref("simulation-view-isaac")]
  contexts = {
    simulation-runtime = "target:simulation-runtime"
  }
  args = {
    SIMULATION_RUNTIME_IMAGE = "simulation-runtime"
  }
}

target "uav-sim-runtime" {
  context    = "showcase/uav-sim/runtime"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  target     = "runtime"
  tags       = [image_ref("uav-sim-runtime")]
  contexts = {
    simulation-runtime = "target:simulation-runtime"
    simulation-pose-sdk = "./sdk/python/src/veoveo_mcp"
  }
  args = {
    SIMULATION_RUNTIME_IMAGE = "simulation-runtime"
  }
}

target "simulation-overlay-acceptance" {
  context    = "testing/fixtures/simulation-overlay"
  dockerfile = "Dockerfile"
  platforms  = ["linux/amd64"]
  tags       = [image_ref("simulation-overlay-acceptance")]
  contexts = {
    simulation-runtime = "target:simulation-runtime"
  }
  args = {
    VEOVEO_SIMULATION_BASE = "simulation-runtime"
  }
}

target "uav-sim-mcp" {
  inherits   = ["_rust-trixie-runtime"]
  dockerfile = "servers/uav-sim-mcp/Dockerfile"
  tags       = [image_ref("uav-sim-mcp")]
  labels = {
    "io.veoveo.build.mode"      = "rust-shared"
    "io.veoveo.build.package"   = "veoveo-uav-sim-mcp"
    "io.veoveo.build.binaries"  = "uav-sim-mcp"
    "io.veoveo.build.family"    = "rust-trixie-v1"
    "io.veoveo.build.auxiliary" = ""
  }
}
