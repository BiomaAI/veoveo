ARG K3S_VERSION=v1.37.0-k3s1
ARG CUDA_VERSION=13.3.0-base-ubuntu24.04

FROM rancher/k3s:${K3S_VERSION}@sha256:d33b1973401a60410681d66c007f5c3a51d565a7c03608904764aef3321fee4d AS k3s
FROM nvidia/cuda:${CUDA_VERSION}@sha256:bcf7d05f0b13b9bbb86d9a4cd039d331894b8f1145ad009d1af75023bcd1dc5c AS cuda-base
FROM cuda-base

ARG NVIDIA_CONTAINER_TOOLKIT_VERSION=1.20.1-1

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && curl -fsSL https://nvidia.github.io/libnvidia-container/gpgkey \
      | gpg --dearmor -o /usr/share/keyrings/nvidia-container-toolkit-keyring.gpg \
    && curl -fsSL https://nvidia.github.io/libnvidia-container/stable/deb/nvidia-container-toolkit.list \
      | sed 's#deb https://#deb [signed-by=/usr/share/keyrings/nvidia-container-toolkit-keyring.gpg] https://#g' \
      > /etc/apt/sources.list.d/nvidia-container-toolkit.list \
    && apt-get update \
    && apt-get install -y --no-install-recommends \
      nvidia-container-toolkit=${NVIDIA_CONTAINER_TOOLKIT_VERSION} \
      nvidia-container-toolkit-base=${NVIDIA_CONTAINER_TOOLKIT_VERSION} \
      libnvidia-container-tools=${NVIDIA_CONTAINER_TOOLKIT_VERSION} \
      libnvidia-container1=${NVIDIA_CONTAINER_TOOLKIT_VERSION} \
    && nvidia-ctk runtime configure --runtime=containerd \
    && rm -rf /var/lib/apt/lists/*

COPY --from=k3s / / --exclude=/bin
COPY --from=k3s /bin /bin
# K3s replaces Ubuntu's tar with BusyBox; dpkg-deb needs GNU tar for later
# package maintenance on a retained node.
COPY --from=cuda-base /usr/bin/tar /tmp/veoveo-gnu-tar
RUN rm -f /usr/bin/tar \
    && mv /tmp/veoveo-gnu-tar /usr/bin/tar \
    && /usr/bin/tar --version | grep -Fq 'GNU tar'

VOLUME /var/lib/kubelet
VOLUME /var/lib/rancher/k3s
VOLUME /var/lib/cni
VOLUME /var/log

ENV PATH="$PATH:/bin/aux"

ENTRYPOINT ["/bin/k3s"]
CMD ["agent"]
