import os
import subprocess
from dataclasses import asdict, dataclass

from . import SUPPORTED_CUOPT_VERSION


class GpuUnavailable(RuntimeError):
    pass


@dataclass(frozen=True)
class GpuHealth:
    ready: bool
    cuopt_version: str
    cuda_runtime_version: str
    gpu_name: str
    gpu_uuid: str
    compute_capability: str

    def to_dict(self) -> dict[str, object]:
        return asdict(self)


def initialize_gpu() -> GpuHealth:
    try:
        query = subprocess.run(
            [
                "nvidia-smi",
                "--query-gpu=name,uuid,compute_cap",
                "--format=csv,noheader,nounits",
            ],
            check=True,
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise GpuUnavailable(f"nvidia-smi failed: {error}") from error
    rows = [row.strip() for row in query.stdout.splitlines() if row.strip()]
    if len(rows) != 1:
        raise GpuUnavailable(
            f"executor requires exactly one visible GPU; found {len(rows)}"
        )
    fields = [field.strip() for field in rows[0].split(",")]
    if len(fields) != 3:
        raise GpuUnavailable("nvidia-smi returned an unexpected GPU record")
    gpu_name, gpu_uuid, compute_capability = fields

    try:
        import cudf
        import cuopt
        import rmm
    except ImportError as error:
        raise GpuUnavailable(f"cuOpt runtime import failed: {error}") from error

    cuopt_version = str(getattr(cuopt, "__version__", "unknown"))
    if not cuopt_version.startswith(SUPPORTED_CUOPT_VERSION):
        raise GpuUnavailable(
            f"cuOpt {cuopt_version} is installed; "
            f"{SUPPORTED_CUOPT_VERSION}.x is required"
        )

    pool_gib = int(os.environ.get("VEOVEO_CUOPT_POOL_GIB", "1"))
    if pool_gib <= 0:
        raise GpuUnavailable("VEOVEO_CUOPT_POOL_GIB must be positive")
    try:
        pool = rmm.mr.PoolMemoryResource(
            rmm.mr.CudaMemoryResource(),
            initial_pool_size=(2**30) * pool_gib,
        )
        rmm.mr.set_current_device_resource(pool)
        probe = cudf.Series([1, 2, 3, 4], dtype="int32")
        if int(probe.sum()) != 10:
            raise RuntimeError("GPU allocation probe returned an invalid value")
    except Exception as error:
        raise GpuUnavailable(f"CUDA allocation probe failed: {error}") from error

    cuda_runtime_version = _cuda_runtime_version()
    return GpuHealth(
        ready=True,
        cuopt_version=cuopt_version,
        cuda_runtime_version=cuda_runtime_version,
        gpu_name=gpu_name,
        gpu_uuid=gpu_uuid,
        compute_capability=compute_capability,
    )


def _cuda_runtime_version() -> str:
    try:
        from cuda.bindings import runtime

        status, version = runtime.cudaRuntimeGetVersion()
        if int(status) != 0:
            return "unknown"
        version = int(version)
        return f"{version // 1000}.{(version % 1000) // 10}"
    except Exception:
        return os.environ.get("CUDA_VERSION", "unknown")
