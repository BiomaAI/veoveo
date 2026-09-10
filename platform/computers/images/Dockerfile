# Qualified amd64 candidate for the initial local Computers profile.
# Ubuntu 26.04 is the current stable LTS; the archive snapshot fixes package inputs.
FROM ubuntu:26.04@sha256:e5a4d6262ab5dbc25a85e60550dd7c87fd41a74fe43881534ed8288b2a7a3f8d

ENV DEBIAN_FRONTEND=noninteractive
# Bootstrap HTTPS from the same signed archive snapshot before APT has a CA store.
ADD --checksum=sha256:f7025ab9b24cd73215510931037b02d6960d89584d0d00afba81851abdbe6ef1 \
    https://snapshot.ubuntu.com/ubuntu/20260909T000000Z/pool/main/c/ca-certificates/ca-certificates_20260223_all.deb /tmp/ca-certificates.deb
RUN dpkg-deb --extract /tmp/ca-certificates.deb /tmp/bootstrap-ca \
    && mkdir -p /etc/ssl/certs \
    && cat /tmp/bootstrap-ca/usr/share/ca-certificates/mozilla/*.crt > /etc/ssl/certs/ca-certificates.crt \
    && apt-get -o Acquire::https::CaInfo=/etc/ssl/certs/ca-certificates.crt update --error-on=any --snapshot 20260909T000000Z \
    && apt-get -o Acquire::https::CaInfo=/etc/ssl/certs/ca-certificates.crt install --snapshot 20260909T000000Z --yes --no-install-recommends \
       ca-certificates curl git iproute2 nftables openssh-sftp-server \
       procps python3 python3-venv \
    && rm -rf /var/lib/apt/lists/* /tmp/ca-certificates.deb /tmp/bootstrap-ca \
    && dpkg-query -W > /usr/share/veoveo-computer-packages.txt

RUN groupadd --gid 10001 computer \
    && useradd --uid 10001 --gid 10001 --home-dir /sandbox/persistent --shell /bin/bash computer \
    && mkdir -p /sandbox/persistent \
    && chown 10001:10001 /sandbox/persistent

ENV HOME=/sandbox/persistent
COPY --from=veoveo-rust-artifacts /bin/veoveo-computer-exec /usr/local/bin/veoveo-computer-exec
COPY platform/computers/images/retained-home.sh /etc/profile.d/20-veoveo-home.sh
RUN /usr/local/bin/veoveo-computer-exec --version
# OpenShell reserves the OCI workdir separately from retained mounts. The login
# profile selects the user's retained home after the provider applies isolation.
WORKDIR /sandbox
# The provider starts its privileged supervisor, which applies confinement and
# drops the user process to the policy's explicit uid/gid. This command has no
# authority to control the provider, host, installation or another Computer.
CMD ["/bin/bash", "-l"]
