# syntax=docker/dockerfile:1.25.0@sha256:0adf442eae370b6087e08edc7c50b552d80ddf261576f4ebd6421006b2461f12
# Reuse the exact Engine tuple qualified by the private-daemon native fixtures.
FROM docker:29.8.0-dind@sha256:77759fdec1efef224ba7110ef7b5b3c6af6164ffaef5441d3beba059bde8b857 AS engine
FROM veoveo-computer-storage

# The storage parent retains its signed Debian snapshot sources. Provider Z3 is
# copied from its qualified image; the GNU runtime satisfies both Rust binaries.
RUN apt-get update --error-on=any \
    && apt-get install -y --no-install-recommends iptables nftables iproute2 kmod libstdc++6 \
    && dpkg-query -W > /usr/share/veoveo-computer-host-packages.txt \
    && rm -rf /var/lib/apt/lists/*
COPY --from=engine /usr/local/bin/dockerd /usr/local/bin/docker /usr/local/bin/containerd /usr/local/bin/containerd-shim-runc-v2 /usr/local/bin/ctr /usr/local/bin/runc /usr/local/bin/docker-init /usr/local/bin/docker-proxy /usr/local/bin/dind /usr/local/bin/
COPY --from=veoveo-computer-provider /usr/local/bin/ /usr/local/bin/
COPY --from=veoveo-computer-provider /usr/lib/x86_64-linux-gnu/libz3.so.4 /usr/lib/x86_64-linux-gnu/libz3.so.4
COPY --from=veoveo-computer-provider /usr/share/veoveo-computer-provider /usr/share/veoveo-computer-provider
COPY --from=veoveo-computer-provider /usr/share/doc/veoveo-computer-provider /usr/share/doc/veoveo-computer-provider
COPY --from=veoveo-computer-provider /usr/share/veoveo-computer-provider-packages.txt /usr/share/veoveo-computer-provider-packages.txt
COPY --from=veoveo-rust-artifacts /bin/veoveo-computer-host /usr/local/bin/veoveo-computer-host
RUN /usr/local/bin/openshell-gateway --version \
    && /usr/local/bin/openshell-sandbox --version \
    && /usr/local/bin/dockerd --version \
    && /usr/local/bin/veoveo-computer-host --help \
    && cd /usr/local/bin \
    && sha256sum veoveo-computer-host veoveo-computer-storage openshell-gateway openshell-driver-docker openshell-sandbox dockerd docker containerd containerd-shim-runc-v2 runc docker-init docker-proxy dind > /usr/share/veoveo-computer-host-binaries.sha256
EXPOSE 8805 8806
USER root
# dind only runs inside this container's private network/mount/PID namespaces.
ENTRYPOINT ["/usr/local/bin/dind", "/usr/local/bin/docker-init", "--", "/usr/local/bin/veoveo-computer-host"]
CMD ["run", "--config", "/etc/veoveo/computers/host-config/host.json"]
