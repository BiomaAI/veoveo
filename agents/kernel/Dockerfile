# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
FROM debian:trixie-slim@sha256:020c0d20b9880058cbe785a9db107156c3c75c2ac944a6aa7ab59f2add76a7bd

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libstdc++6 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=veoveo-rust-artifacts /lib/libduckdb.so /usr/local/lib/libduckdb.so
RUN ldconfig

RUN useradd --system --uid 10001 --create-home --home-dir /var/lib/veoveo veoveo \
    && mkdir -p /var/lib/veoveo/pilot \
    && chown -R veoveo:veoveo /var/lib/veoveo

COPY --from=veoveo-rust-artifacts /bin/agent /usr/local/bin/agent

USER veoveo
EXPOSE 8794
ENTRYPOINT ["/usr/local/bin/agent"]
