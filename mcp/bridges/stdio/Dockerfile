# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd

ARG RERUN_VERSION=0.36.2
ARG TARGETARCH

# The Vulkan loader resolves the hardware driver mounted by the GPU runtime.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        libegl1 \
        libvulkan1 \
        libx11-6 \
        libxext6 \
    && rm -rf /var/lib/apt/lists/*

RUN case "${TARGETARCH}" in \
        arm64) rerun_arch=aarch64 ;; \
        amd64) rerun_arch=x86_64 ;; \
        *) echo "unsupported TARGETARCH: ${TARGETARCH}" >&2; exit 1 ;; \
    esac \
    && curl -fsSL -o /usr/local/bin/rerun \
        "https://github.com/rerun-io/rerun/releases/download/${RERUN_VERSION}/rerun-cli-${RERUN_VERSION}-${rerun_arch}-unknown-linux-gnu" \
    && chmod +x /usr/local/bin/rerun \
    && if ldd /usr/local/bin/rerun | grep "not found"; then exit 1; fi \
    && /usr/local/bin/rerun --version | grep -F "rerun-cli ${RERUN_VERSION} "

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo

COPY --from=veoveo-rust-artifacts /bin/bridge /usr/local/bin/bridge
COPY mcp/bridges/stdio/docker-entrypoint.sh /usr/local/bin/rerun-mcp-entrypoint
RUN chmod +x /usr/local/bin/rerun-mcp-entrypoint

USER veoveo
EXPOSE 8790 9876
ENTRYPOINT ["/usr/local/bin/rerun-mcp-entrypoint"]
