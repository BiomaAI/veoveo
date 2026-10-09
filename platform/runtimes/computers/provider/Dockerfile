# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
# Package unmodified official release assets. This stage has no provider compiler.
# Official current stable Trixie image identity verified on 2026-10-09.
FROM debian:trixie-slim@sha256:a29215f6a35e51e22adffa17f89e9d2ef06214e64a2bad10d765c46aea49f11f AS release-assets
COPY <<EOF /etc/apt/sources.list.d/debian.sources
Types: deb
URIs: http://snapshot.debian.org/archive/debian/20261005T000000Z/
Suites: trixie trixie-updates
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
Check-Valid-Until: no

Types: deb
URIs: http://snapshot.debian.org/archive/debian-security/20261005T000000Z/
Suites: trixie-security
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.gpg
Check-Valid-Until: no
EOF
RUN apt-get update --error-on=any \
    && apt-get install -y --no-install-recommends ca-certificates python3 \
    && rm -rf /var/lib/apt/lists/*
ARG PROVIDER_MANIFEST_SHA256
COPY platform/runtimes/computers/provider/manifest.json /release/manifest.json
COPY platform/runtimes/computers/provider/LICENSE /release/LICENSE
RUN <<'PYTHON_FETCH'
set -eu
test -n "${PROVIDER_MANIFEST_SHA256:-}"
test "$(sha256sum /release/manifest.json | cut -d' ' -f1)" = "$PROVIDER_MANIFEST_SHA256"
python3 - <<'PYTHON'
import hashlib, io, json, pathlib, signal, tarfile, urllib.request
profile = json.loads(pathlib.Path('/release/manifest.json').read_text())
assert profile['schema'] == 'veoveo.ai/openshell-release-profile/v1'
assert profile['driverIntegration'] == 'gateway_in_process'
pathlib.Path('/binaries').mkdir()
for binary in profile['binaries']:
    asset = binary['releaseAsset']
    signal.alarm(180)
    with urllib.request.urlopen(asset['url'], timeout=30) as source:
        archive = source.read(128 * 1024 * 1024 + 1)
    signal.alarm(0)
    assert len(archive) <= 128 * 1024 * 1024, 'release archive exceeds 128 MiB'
    assert hashlib.sha256(archive).hexdigest() == asset['archiveSha256'], 'release archive digest mismatch'
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as package:
        members = package.getmembers()
        assert len(members) == 1 and members[0].isfile(), 'one regular executable required'
        assert pathlib.PurePosixPath(members[0].name).name == binary['name']
        assert members[0].size <= 256 * 1024 * 1024, 'release executable exceeds 256 MiB'
        executable = package.extractfile(members[0]).read()
    assert hashlib.sha256(executable).hexdigest() == asset['executableSha256'], 'release executable digest mismatch'
    destination = pathlib.Path('/binaries') / binary['name']
    destination.write_bytes(executable)
    destination.chmod(0o555)
pathlib.Path('/release/binaries.sha256').write_text(''.join(
    f"{binary['releaseAsset']['executableSha256']}  {binary['name']}\n" for binary in profile['binaries']))
PYTHON
PYTHON_FETCH

FROM ghcr.io/nvidia/openshell/gateway@sha256:e0e18aa7a497290eed1f6ffb36f51540f037cec967c6d4354a264fecfd988620
# The gateway bytes equal the official image; tools retain their release bytes.
COPY --from=release-assets /binaries/ /usr/local/bin/
COPY --from=release-assets /release/manifest.json /release/binaries.sha256 /usr/share/veoveo-computer-provider/
COPY --from=release-assets /release/LICENSE /usr/share/doc/veoveo-computer-provider/LICENSE
