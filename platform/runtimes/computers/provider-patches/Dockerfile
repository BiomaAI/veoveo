# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12

# The qualified provider toolchain is independent of Veoveo's Rust toolchain.
FROM rust:1.95.0-slim-bookworm@sha256:d7482085ff5b415f84dba5647ae71606650bdef00db7aeb69f4b3d170c3e4082 AS compiler
COPY <<EOF /etc/apt/sources.list.d/debian.sources
Types: deb
URIs: http://snapshot.debian.org/archive/debian/20260910T000000Z/
Suites: bookworm bookworm-updates
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
Check-Valid-Until: no

Types: deb
URIs: http://snapshot.debian.org/archive/debian-security/20260910T000000Z/
Suites: bookworm-security
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
Check-Valid-Until: no
EOF
RUN apt-get update --error-on=any \
    && apt-get install -y --no-install-recommends ca-certificates git jq build-essential pkg-config libz3-dev \
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_TOOLCHAIN=1.95.0 CARGO_TARGET_DIR=/provider-target
WORKDIR /source
# Fetch once by immutable object identity; no branch or tag resolves during builds.
RUN git init --quiet \
    && git fetch --depth=1 https://github.com/NVIDIA/OpenShell.git d1155aa70042d3e2ee49dbfa15346b108b7c1d92 \
    && test "$(git rev-parse FETCH_HEAD)" = d1155aa70042d3e2ee49dbfa15346b108b7c1d92
COPY platform/runtimes/computers/provider-patches/manifest.json platform/runtimes/computers/provider-patches/*.patch /patches/
RUN <<'EOF'
set -eu
test "$(jq -r .upstreamRevision /patches/manifest.json)" = d1155aa70042d3e2ee49dbfa15346b108b7c1d92
for branch in gateway supervisor; do
    git read-tree FETCH_HEAD
    jq -r --arg branch "$branch" '.patches[] | select(.branch == "common" or .branch == $branch) | [.file,.packagedPatchSha256,.expectedSourceTree] | @tsv' /patches/manifest.json > /tmp/patch-list
    while read -r patch digest tree; do
        printf '%s  /patches/%s\n' "$digest" "$patch" | sha256sum --check --status
        git apply --cached "/patches/$patch"
        test "$(git write-tree)" = "$tree"
    done < /tmp/patch-list
    mkdir "/$branch"
    git archive "$(git write-tree)" | tar -x -C "/$branch"
    version=$(jq -r ".${branch}Version" /patches/manifest.json)
    sed -i "s/^version = \"0.0.0\"$/version = \"$version\"/" "/$branch/Cargo.toml" "/$branch/Cargo.lock"
done
EOF
WORKDIR /gateway
RUN --mount=type=cache,id=veoveo-openshell-rust195-bookworm-cargo,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=veoveo-openshell-rust195-bookworm-target,target=/provider-target,sharing=locked \
    cargo build --locked --release -p openshell-server -p openshell-driver-docker \
    && mkdir /binaries \
    && cp /provider-target/release/openshell-gateway /provider-target/release/openshell-driver-docker /binaries/
WORKDIR /supervisor
RUN --mount=type=cache,id=veoveo-openshell-rust195-bookworm-cargo,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=veoveo-openshell-rust195-bookworm-target,target=/provider-target,sharing=locked \
    cargo build --locked --release -p openshell-sandbox \
    && cp /provider-target/release/openshell-sandbox /binaries/

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
COPY --from=compiler /etc/apt/sources.list.d/debian.sources /etc/apt/sources.list.d/debian.sources
RUN apt-get update --error-on=any \
    && apt-get install -y --no-install-recommends ca-certificates libz3-4 libstdc++6 \
    && dpkg-query -W > /usr/share/veoveo-computer-provider-packages.txt \
    && rm -rf /var/lib/apt/lists/*
COPY --from=compiler /binaries/ /usr/local/bin/
COPY --from=compiler /gateway/LICENSE /usr/share/doc/veoveo-computer-provider/LICENSE
COPY platform/runtimes/computers/provider-patches /usr/share/veoveo-computer-provider/source
RUN cd /usr/local/bin && sha256sum openshell-gateway openshell-driver-docker openshell-sandbox > /usr/share/veoveo-computer-provider/binaries.sha256
ENTRYPOINT ["/usr/local/bin/openshell-gateway"]
