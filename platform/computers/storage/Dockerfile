# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
# Current official trixie-slim index verified on 2026-09-10.
FROM debian:trixie-slim@sha256:d7e12182ce18b85b93007c1dedf31f2d29e01ccf3182cc4017c709b6259bc132

COPY <<'SOURCES' /etc/apt/sources.list.d/debian.sources
Types: deb
URIs: http://snapshot.debian.org/archive/debian/20260910T000000Z/
Suites: trixie trixie-updates
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
Check-Valid-Until: no

Types: deb
URIs: http://snapshot.debian.org/archive/debian-security/20260910T000000Z/
Suites: trixie-security
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
Check-Valid-Until: no
SOURCES
RUN apt-get update --error-on=any \
    && apt-get install -y --no-install-recommends ca-certificates e2fsprogs util-linux \
    && dpkg-query -W > /usr/share/veoveo-computer-storage-packages.txt \
    && rm -rf /var/lib/apt/lists/*

COPY --from=veoveo-rust-artifacts /bin/veoveo-computer-storage /usr/local/bin/veoveo-computer-storage
# Mount/loop privileges are confined to this separately deployed storage process.
USER root
ENTRYPOINT ["/usr/local/bin/veoveo-computer-storage"]
