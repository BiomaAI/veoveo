# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12

FROM docker.io/library/rust:1.99.0-slim-bookworm@sha256:452176c0cefca88c0b3184ce85a4eb03e3d4fa05d2afb5366abcba853221019e AS compiler
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
    && apt-get install -y --no-install-recommends ca-certificates git jq build-essential m4 pkg-config cmake python3 musl-tools file binutils \
    && rm -rf /var/lib/apt/lists/*
ENV RUSTUP_TOOLCHAIN=1.99.0 CARGO_TARGET_DIR=/provider-target
RUN rustup target add x86_64-unknown-linux-musl --toolchain 1.99.0 \
    && test "$(rustc --version | cut -d' ' -f2)" = 1.99.0 \
    && test "$(cargo --version | cut -d' ' -f2)" = 1.99.0

COPY platform/runtimes/computers/provider-patches/control.sh /usr/local/bin/provider-control
RUN chmod 0555 /usr/local/bin/provider-control

FROM compiler AS source
ARG PROVIDER_MANIFEST_SHA256
WORKDIR /source
# Fetch once by immutable object identity; no branch or tag resolves during builds.
RUN git init --quiet \
    && git fetch --depth=1 https://github.com/NVIDIA/OpenShell.git 6648bd0c290efbc41ba131ee9831ee45cd431f94 \
    && test "$(git rev-parse FETCH_HEAD)" = 6648bd0c290efbc41ba131ee9831ee45cd431f94
COPY platform/runtimes/computers/provider-patches/manifest.json platform/runtimes/computers/provider-patches/*.patch /patches/
COPY platform/runtimes/computers/provider-patches/generated /generated-config
COPY platform/computers/host/src/config.rs /configuration-producers/host.rs
COPY platform/runtimes/computers/tests/native_support/mod.rs /configuration-producers/native.rs
RUN <<'EOF'
set -eu
test -n "${PROVIDER_MANIFEST_SHA256:-}"
test "$(sha256sum /patches/manifest.json | cut -d' ' -f1)" = "$PROVIDER_MANIFEST_SHA256"
test "$(jq '.generatedConfigurations | length' /patches/manifest.json)" = 2
jq -e '[.generatedConfigurations[].file] | sort == ["host.toml", "native.toml"]' /patches/manifest.json > /dev/null
jq -r '.generatedConfigurations[] | [.file,.sha256,.producer,.producerSha256] | @tsv' /patches/manifest.json > /tmp/config-list
while read -r file digest producer producer_digest; do
    printf '%s  /generated-config/%s\n' "$digest" "$file" | sha256sum --check --status
    printf '%s  /configuration-producers/%s\n' "$producer_digest" "$producer" | sha256sum --check --status
done < /tmp/config-list
test "$(jq -r .upstreamRevision /patches/manifest.json)" = 6648bd0c290efbc41ba131ee9831ee45cd431f94
test "$(git rev-parse FETCH_HEAD^{tree})" = "$(jq -r .upstreamSourceTree /patches/manifest.json)"
mkdir /cli
git archive FETCH_HEAD | tar -x -C /cli
sed -i 's/^version = "0.0.0"$/version = "0.1.2"/' /cli/Cargo.toml /cli/Cargo.lock
for branch in gateway supervisor; do
    git read-tree FETCH_HEAD
    jq -r --arg branch "$branch" '.patches[] | select(.branch == "common" or .branch == $branch) | [.file,.packagedPatchSha256,.expectedBaseTree,.expectedSourceTree] | @tsv' /patches/manifest.json > /tmp/patch-list
    while read -r patch digest base tree; do
        test "$(git write-tree)" = "$base"
        printf '%s  /patches/%s\n' "$digest" "$patch" | sha256sum --check --status
        git apply --cached "/patches/$patch"
        test "$(git write-tree)" = "$tree"
    done < /tmp/patch-list
    mkdir "/$branch"
    git archive "$(git write-tree)" | tar -x -C "/$branch"
    version=$(jq -r ".${branch}Version" /patches/manifest.json)
    sed -i "s/^version = \"0.0.0\"$/version = \"$version\"/" "/$branch/Cargo.toml" "/$branch/Cargo.lock"
done
cp -r /generated-config /gateway/veoveo-config
EOF

# Build the current C++ toolchain against the existing glibc baseline.
# This stage depends only on its own source profile, so provider patches reuse it.
FROM compiler AS cxx-build
COPY platform/runtimes/computers/provider-patches/cxx-profile.json /cpp-profile/cxx-profile.json
RUN <<'CXX_BUILD'
set -eu
mkdir /cpp-source /cpp-build /cpp-evidence
python3 - <<'PYTHON'
import hashlib, json, pathlib, signal, subprocess, urllib.request
profile = json.loads(pathlib.Path('/cpp-profile/cxx-profile.json').read_text())
assert profile['schema'] == 'veoveo.ai/openshell-cxx-build/v1'
assert (profile['compiler'], profile['version'], profile['baseAbi'], profile['languages'], profile['cxxStandard'], profile['z3CxxRuntime']) == ('gcc', '16.2.0', 'glibc 2.36', ['c', 'c++'], 20, 'static=stdc++')
assert profile['bootstrap'] is False and profile['multilib'] is False
assert profile['solver']['version'] == '5.1.0' and profile['solver']['linkage'] == 'static'
assert [(i['component'], i['version']) for i in profile['inputs']] == [('gcc', '16.2.0'), ('gmp', '6.3.0'), ('mpfr', '4.2.2'), ('mpc', '1.4.1')]
for entry in profile['inputs']:
    assert entry['file'] == f"{entry['component']}-{entry['version']}.tar.xz"
    path = pathlib.Path('/cpp-source') / entry['file']
    digest = hashlib.sha512()
    size = 0
    print(f"downloading {entry['file']} (180-second deadline)", flush=True)
    signal.alarm(180)
    with urllib.request.urlopen(entry['url'], timeout=30) as source, path.open('wb') as output:
        while block := source.read(65536):
            size += len(block)
            assert size <= 256 * 1024 * 1024, 'C++ source archive exceeds its bound'
            digest.update(block)
            output.write(block)
    signal.alarm(0)
    assert digest.hexdigest() == entry['sha512'], f"source hash mismatch: {entry['file']}"
    subprocess.run(['tar', '-xf', str(path), '-C', '/cpp-source'], check=True)
PYTHON
export CONFIG_SHELL=/bin/bash
cd /cpp-build
mkdir gmp mpfr mpc gcc
cd gmp
/cpp-source/gmp-6.3.0/configure --prefix=/opt/veoveo-cxx-prerequisites --disable-shared --enable-static --enable-fat
make -j4
make install
cd ../mpfr
/cpp-source/mpfr-4.2.2/configure --prefix=/opt/veoveo-cxx-prerequisites --disable-shared --enable-static --with-gmp=/opt/veoveo-cxx-prerequisites
make -j4
make install
cd ../mpc
/cpp-source/mpc-1.4.1/configure --prefix=/opt/veoveo-cxx-prerequisites --disable-shared --enable-static --with-gmp=/opt/veoveo-cxx-prerequisites --with-mpfr=/opt/veoveo-cxx-prerequisites
make -j4
make install
cd ../gcc
/cpp-source/gcc-16.2.0/configure --prefix=/opt/veoveo-gcc --enable-languages=c,c++ --disable-bootstrap --disable-multilib --disable-nls --without-isl --disable-libsanitizer --with-gmp=/opt/veoveo-cxx-prerequisites --with-mpfr=/opt/veoveo-cxx-prerequisites --with-mpc=/opt/veoveo-cxx-prerequisites
make -j4
make install
mkdir /opt/veoveo-gcc-static
archive=$(/opt/veoveo-gcc/bin/g++ -print-file-name=libstdc++.a)
test -f "$archive"
cp "$archive" /opt/veoveo-gcc-static/libstdc++.a
/opt/veoveo-gcc/bin/g++ --version > /cpp-evidence/compiler.txt
test "$(/opt/veoveo-gcc/bin/g++ -dumpfullversion)" = 16.2.0
cp /cpp-profile/cxx-profile.json /cpp-evidence/
getconf GNU_LIBC_VERSION > /cpp-evidence/base-abi.txt
test "$(getconf GNU_LIBC_VERSION)" = "glibc 2.36"
sha256sum /opt/veoveo-gcc-static/libstdc++.a > /cpp-evidence/cxx-runtime.sha256
cp /cpp-source/gcc-16.2.0/COPYING3 /cpp-source/gcc-16.2.0/COPYING.RUNTIME /cpp-evidence/
CXX_BUILD

FROM cxx-build AS cxx-probe
RUN <<'CXX_PROBE'
set -eu
cat > /tmp/format.cpp <<'CPP'
#include <format>
#include <string>
int main() { return std::format("veoveo-{}-{:02}", 16, 2) == "veoveo-16-02" ? 0 : 1; }
CPP
/opt/veoveo-gcc/bin/g++ -std=c++20 -static-libstdc++ /tmp/format.cpp -o /tmp/format
/tmp/format
printf 'C++20 std::format compile and execution passed\n' > /cpp-evidence/format-result.txt
readelf -d /tmp/format > /cpp-evidence/format-dependencies.txt
! grep -q 'libstdc++' /cpp-evidence/format-dependencies.txt
readelf --version-info /tmp/format > /cpp-evidence/format-symbol-versions.txt
python3 - <<'PYTHON'
import pathlib, re
versions = [tuple(map(int, match)) for match in re.findall(r'\bGLIBC_(\d+)\.(\d+)', pathlib.Path('/cpp-evidence/format-symbol-versions.txt').read_text())]
assert versions and max(versions) <= (2, 36), 'C++ format probe exceeds glibc 2.36 ABI'
PYTHON
CXX_PROBE

# Export probe receipts without exporting or importing a compiler image.
FROM scratch AS cxx-probe-evidence
COPY --from=cxx-probe /cpp-evidence/ /

FROM compiler AS gateway-build
COPY --from=cxx-probe /opt/veoveo-gcc /opt/veoveo-gcc
COPY --from=cxx-probe /opt/veoveo-gcc-static /opt/veoveo-gcc-static
COPY --from=cxx-probe /cpp-profile /cpp-profile
COPY --from=cxx-probe /cpp-evidence /cpp-evidence
COPY --from=source /gateway /gateway
COPY platform/runtimes/computers/provider-patches/manifest.json /cpp-profile/manifest.json
ENV CXX=/opt/veoveo-gcc/bin/g++ CXXSTDLIB=static=stdc++ RUSTFLAGS=-Lnative=/opt/veoveo-gcc-static
RUN test "$(sha256sum /cpp-profile/cxx-profile.json | cut -d' ' -f1)" = "$(jq -r .cxx.profileSha256 /cpp-profile/manifest.json)"
WORKDIR /gateway
# Keep the compiler-independent registry source cache. Scope compiled artifacts to Rust 1.99.
RUN --mount=type=cache,id=veoveo-openshell-rust195-bookworm-cargo,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=veoveo-openshell-rust199-bookworm-target,target=/provider-target,sharing=locked <<'PROVIDER_ELF'
set -eu
cargo metadata --locked --no-deps --format-version=1 > /tmp/provider-packages.json
cargo test --locked --release -p openshell-core --test mtls_user_admission \
    && provider-control openshell-driver-docker --lib companion_state docker_restart_state_restores_selected_template_and_retained_home_without_credentials docker_restart_state_refuses_foreign_parent_image_labels_mounts_and_owner_profile docker_restart_state_missing_corrupt_and_unsupported_records_refuse_before_spec_or_auth_change log_limits supervisor_logs retained_volume_no_copy docker_identity_resolution lifecycle_fence start_generation provider_runtime_images veoveo_generated_configs_pass_actual_docker_schema \
    && provider-control openshell-server --lib veoveo_supervisor_observation_unknown_survives_current_session_composition stop_and_start_follow_durable_state_machine authenticated_start_commits_identity_with_starting_phase request_cancellation_does_not_cancel_start_worker failed_start_reconciles_backend_that_already_started stale_container_exit_queued_before_start_cannot_regress_restart live_container_exit_during_start_still_transitions_to_error start_persisted_sandboxes_supplies_fresh_authentication veoveo_generated_configs_pass_actual_gateway_loader \
    && (cargo build --locked --release --message-format=json-render-diagnostics --features openshell-gateway/bundled-z3 -p openshell-gateway -p openshell-driver-docker > /tmp/provider-artifacts.jsonl || { cat /tmp/provider-artifacts.jsonl; exit 1; })

# Admit and copy emitted artifacts while their compiler cache remains mounted.
python3 - <<'PYTHON'
import hashlib, json, pathlib, re, shutil, subprocess
# Cargo's emitted executable owns the copy; a cache pathname alone cannot admit it.
selected = {'openshell-gateway': None, 'openshell-driver-docker': None}
profile = json.loads(pathlib.Path('/cpp-profile/manifest.json').read_text())
packages = json.loads(pathlib.Path('/tmp/provider-packages.json').read_text())['packages']
expected = {}
for name in selected:
    version = profile['gatewayVersion' if name == 'openshell-gateway' else 'driverVersion']
    owners = [package for package in packages if package['name'] == name
              and package['version'] == version
              and pathlib.Path(package['manifest_path']) == pathlib.Path('/gateway/crates') / name / 'Cargo.toml'
              and any(target['name'] == name and 'bin' in target['kind'] for target in package['targets'])]
    if len(owners) != 1:
        raise SystemExit(f'{name}: selected source must declare one executable-owning package at version {version}')
    expected[name] = owners[0]['id']
for line in pathlib.Path('/tmp/provider-artifacts.jsonl').read_text().splitlines():
    event = json.loads(line)
    if event.get('reason') != 'compiler-artifact':
        continue
    target = event['target']
    if target['name'] in selected and 'bin' in target['kind'] and event.get('executable'):
        if event['package_id'] != expected[target['name']]:
            raise SystemExit(f"{target['name']}: Cargo artifact package {event['package_id']} differs from admitted owner {expected[target['name']]}")
        if selected[target['name']] is not None:
            raise SystemExit(f"duplicate Cargo executable artifact for {target['name']}")
        if target['name'] == 'openshell-gateway' and 'bundled-z3' not in event['features']:
            raise SystemExit('Cargo gateway executable artifact lacks its bundled-z3 feature')
        selected[target['name']] = event
missing = [name for name, event in selected.items() if event is None]
if missing:
    raise SystemExit(f'Cargo did not emit selected executable artifacts: {missing}')
pathlib.Path('/binaries').mkdir()
artifacts = []
for name, event in selected.items():
    source = pathlib.Path(event['executable'])
    destination = pathlib.Path('/binaries') / name
    source_hash = hashlib.sha256(source.read_bytes()).hexdigest()
    shutil.copy2(source, destination)
    copied_hash = hashlib.sha256(destination.read_bytes()).hexdigest()
    if source_hash != copied_hash:
        raise SystemExit(f'{name}: copied artifact SHA-256 differs from Cargo executable')
    artifact = dict(name=name, packageId=event['package_id'], executable=str(source), sha256=copied_hash)
    artifacts.append(artifact)
    print(f'{name}: Cargo artifact {artifact}', flush=True)
pathlib.Path('/binaries/build-artifacts.txt').write_text(json.dumps(artifacts, indent=2) + '\n')
failures = []
for name, receipt in [('openshell-gateway', 'gateway'), ('openshell-driver-docker', 'driver')]:
    binary = pathlib.Path('/binaries') / name
    dependencies = subprocess.check_output(['readelf', '-d', str(binary)], text=True)
    symbols = subprocess.check_output(['readelf', '--version-info', str(binary)], text=True)
    (binary.parent / f'{receipt}-dependencies.txt').write_text(dependencies)
    (binary.parent / f'{receipt}-symbol-versions.txt').write_text(symbols)
    needed = re.findall(r'\(NEEDED\).*?\[([^\]]+)\]', dependencies)
    versions = [tuple(map(int, match.split('.'))) for match in re.findall(r'\bGLIBC_(\d+\.\d+(?:\.\d+)?)', symbols)]
    maximum = max(versions, default=None)
    print(f'{name}: DT_NEEDED={needed}; maximum required GLIBC={maximum}', flush=True)
    forbidden = [library for library in needed if 'libz3' in library or 'libstdc++' in library]
    if forbidden:
        failures.append(f'{name}: dynamic solver/C++ dependencies {forbidden}; build the executable with bundled-z3 and the selected static C++ archive')
    if maximum is None or maximum + (0,) * (3 - len(maximum)) > (2, 36, 0):
        failures.append(f'{name}: required GLIBC {maximum} exceeds or cannot establish the Bookworm 2.36 ceiling')
if failures:
    raise SystemExit('\n'.join(failures))
PYTHON
PROVIDER_ELF

FROM compiler AS supervisor-build
COPY --from=source /supervisor /supervisor
WORKDIR /supervisor
RUN --mount=type=cache,id=veoveo-openshell-rust195-bookworm-cargo,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=veoveo-openshell-rust199-bookworm-target,target=/provider-target,sharing=locked \
    provider-control openshell-supervisor-process --lib main_session::tests ssh::tests \
    && provider-control openshell-sandbox --lib process::tests landlock::tests \
    && provider-control openshell-sandbox --lib process::tests::child_can_reopen_owned_terminal_after_numeric_privilege_drop process::tests::explicit_numeric_identity_drops_inherited_supplementary_groups --ignored --exact \
    && cargo build --locked --release -p openshell-supervisor \
    && cargo build --locked --release -p openshell-sandbox --target x86_64-unknown-linux-musl \
    && readelf -l /provider-target/x86_64-unknown-linux-musl/release/openshell-sandbox > /tmp/sandbox-program-headers \
    && ! grep -q INTERP /tmp/sandbox-program-headers \
    && readelf -d /provider-target/x86_64-unknown-linux-musl/release/openshell-sandbox > /tmp/sandbox-dynamic \
    && ! grep -q NEEDED /tmp/sandbox-dynamic \
    && mkdir /binaries \
    && cp /provider-target/release/openshell-supervisor /binaries/ \
    && cp /provider-target/x86_64-unknown-linux-musl/release/openshell-sandbox /binaries/ \
    && rustc --version > /binaries/compiler.txt \
    && cargo --version >> /binaries/compiler.txt \
    && file /binaries/* > /binaries/abi.txt \
    && readelf -d /binaries/openshell-supervisor > /binaries/supervisor-dependencies.txt

FROM compiler AS cli-build
COPY --from=source /cli /cli
WORKDIR /cli
RUN --mount=type=cache,id=veoveo-openshell-rust195-bookworm-cargo,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,id=veoveo-openshell-rust199-bookworm-target,target=/provider-target,sharing=locked \
    cargo build --locked --release -p openshell-cli \
    && mkdir /binaries \
    && cp /provider-target/release/openshell /binaries/

FROM debian:bookworm-slim@sha256:88200866dfff7ea7f5cbcb6ec7c8a701889efe6fe859fe64d6990e4b07ea4171
ARG PROVIDER_MANIFEST_SHA256
COPY --from=compiler /etc/apt/sources.list.d/debian.sources /etc/apt/sources.list.d/debian.sources
RUN apt-get update --error-on=any \
    && apt-get install -y --no-install-recommends ca-certificates libstdc++6 \
    && dpkg-query -W > /usr/share/veoveo-computer-provider-packages.txt \
    && rm -rf /var/lib/apt/lists/*
COPY --from=gateway-build /binaries/openshell-gateway /binaries/openshell-driver-docker /usr/local/bin/
COPY --from=gateway-build /binaries/*.txt /usr/share/veoveo-computer-provider/
COPY --from=gateway-build /cpp-evidence/ /usr/share/veoveo-computer-provider/cxx-build/
COPY --from=supervisor-build /binaries/openshell-supervisor /usr/local/bin/openshell-supervisor
COPY --from=supervisor-build /binaries/openshell-sandbox /usr/local/bin/openshell-sandbox
COPY --from=supervisor-build /binaries/*.txt /usr/share/veoveo-computer-provider/
COPY --from=cli-build /binaries/ /usr/local/bin/
# Docker driver executes this fixed sidecar handler path. The sandbox is the static injected workload boundary.
RUN ln -s /usr/local/bin/openshell-supervisor /openshell-supervisor \
    && ln -s /usr/local/bin/openshell-sandbox /openshell-sandbox
COPY --from=source /gateway/LICENSE /usr/share/doc/veoveo-computer-provider/LICENSE
COPY platform/runtimes/computers/provider-patches /usr/share/veoveo-computer-provider/source
RUN cd /usr/local/bin && sha256sum openshell openshell-gateway openshell-driver-docker openshell-supervisor openshell-sandbox > /usr/share/veoveo-computer-provider/binaries.sha256
LABEL ai.veoveo.provider.profile="0.1.2-veoveo.1" \
      ai.veoveo.provider.manifest-sha256="$PROVIDER_MANIFEST_SHA256" \
      ai.veoveo.provider.supervisor-source-tree="0020919d2c5d2c61230a0a188edaf4469ba9d0f1"
ENTRYPOINT ["/usr/local/bin/openshell-gateway"]
