ARG K3S_VERSION=v1.36.2-k3s1
ARG CUDA_VERSION=13.3.0-base-ubuntu24.04

FROM rancher/k3s:${K3S_VERSION} AS k3s
FROM nvidia/cuda:${CUDA_VERSION}

ARG NVIDIA_CONTAINER_TOOLKIT_VERSION=1.19.1-1

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
    && nvidia-ctk runtime configure --runtime=containerd \
    && rm -rf /var/lib/apt/lists/*

COPY --from=k3s / / --exclude=/bin
COPY --from=k3s /bin /bin
COPY nvidia-device-plugin.yaml /var/lib/rancher/k3s/server/manifests/nvidia-device-plugin.yaml

VOLUME /var/lib/kubelet
VOLUME /var/lib/rancher/k3s
VOLUME /var/lib/cni
VOLUME /var/log

ENV PATH="$PATH:/bin/aux"

ENTRYPOINT ["/bin/k3s"]
CMD ["agent"]
