from __future__ import annotations

import ctypes
import subprocess
from dataclasses import dataclass


@dataclass(frozen=True, slots=True)
class NvidiaIdentity:
    uuid: str
    name: str
    driver_version: str
    cuda_device_count: int
    nvenc_api_version: int


def verify_nvidia_gpu_and_nvenc() -> NvidiaIdentity:
    try:
        cuda = ctypes.CDLL("libcuda.so.1")
        nvenc = ctypes.CDLL("libnvidia-encode.so.1")
    except OSError as error:
        raise RuntimeError(
            "Simulation View requires NVIDIA CUDA and NVENC driver libraries"
        ) from error

    cuda.cuInit.argtypes = [ctypes.c_uint]
    cuda.cuInit.restype = ctypes.c_int
    cuda.cuDeviceGetCount.argtypes = [ctypes.POINTER(ctypes.c_int)]
    cuda.cuDeviceGetCount.restype = ctypes.c_int
    count = ctypes.c_int()
    if cuda.cuInit(0) != 0 or cuda.cuDeviceGetCount(ctypes.byref(count)) != 0:
        raise RuntimeError("NVIDIA CUDA initialization failed")
    if count.value != 1:
        raise RuntimeError(
            "Simulation View requires exactly one visible NVIDIA GPU"
        )

    nvenc.NvEncodeAPIGetMaxSupportedVersion.argtypes = [
        ctypes.POINTER(ctypes.c_uint32)
    ]
    nvenc.NvEncodeAPIGetMaxSupportedVersion.restype = ctypes.c_int
    nvenc_version = ctypes.c_uint32()
    if (
        nvenc.NvEncodeAPIGetMaxSupportedVersion(
            ctypes.byref(nvenc_version)
        )
        != 0
        or not hasattr(nvenc, "NvEncodeAPICreateInstance")
    ):
        raise RuntimeError("NVIDIA NVENC initialization failed")

    completed = subprocess.run(
        [
            "nvidia-smi",
            "--query-gpu=uuid,name,driver_version",
            "--format=csv,noheader,nounits",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    rows = [
        [field.strip() for field in row.split(",")]
        for row in completed.stdout.splitlines()
        if row.strip()
    ]
    if len(rows) != 1 or len(rows[0]) != 3:
        raise RuntimeError("nvidia-smi did not return one complete GPU record")
    uuid, name, driver = rows[0]
    return NvidiaIdentity(
        uuid=uuid,
        name=name,
        driver_version=driver,
        cuda_device_count=count.value,
        nvenc_api_version=nvenc_version.value,
    )
