use super::*;
use anyhow::ensure;

const NVIDIA_GPU_UUID_PREFIX: &str = "GPU-";

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct NvidiaGpuUuid(String);

impl NvidiaGpuUuid {
    fn parse(value: &str, source: &str) -> Result<Self> {
        let value = value.trim();
        ensure!(
            value.starts_with(NVIDIA_GPU_UUID_PREFIX)
                && value.len() > NVIDIA_GPU_UUID_PREFIX.len()
                && !value.contains(',')
                && !value.chars().any(char::is_whitespace),
            "{source} did not contain one NVIDIA GPU UUID: {value:?}"
        );
        Ok(Self(value.to_owned()))
    }

    pub(crate) fn from_visible_devices(output: &str) -> Result<Self> {
        Self::parse(output, "NVIDIA_VISIBLE_DEVICES")
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NvidiaGpuIdentity {
    pub(crate) name: String,
    pub(crate) uuid: NvidiaGpuUuid,
    pub(crate) driver_version: String,
}

pub(crate) fn parse_single_nvidia_smi_gpu(output: &str) -> Result<NvidiaGpuIdentity> {
    let lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    ensure!(
        lines.len() == 1,
        "expected exactly one visible NVIDIA GPU, found {}: {output}",
        lines.len()
    );
    let fields = lines[0].split(',').map(str::trim).collect::<Vec<_>>();
    ensure!(
        fields.len() == 3,
        "nvidia-smi did not return name, UUID, and driver version: {}",
        lines[0]
    );
    let name = fields[0];
    let driver_version = fields[2];
    let fingerprint = name.to_ascii_lowercase();
    ensure!(
        fingerprint.contains("nvidia")
            && !fingerprint.contains("software")
            && !fingerprint.contains("llvmpipe"),
        "nvidia-smi did not report NVIDIA hardware: {name}"
    );
    ensure!(
        !driver_version.is_empty(),
        "nvidia-smi omitted the NVIDIA driver version"
    );
    Ok(NvidiaGpuIdentity {
        name: name.to_owned(),
        uuid: NvidiaGpuUuid::parse(fields[1], "nvidia-smi")?,
        driver_version: driver_version.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_one_allocated_gpu() {
        let identity =
            parse_single_nvidia_smi_gpu("NVIDIA L40S, GPU-1234-abcd, 575.57.08\n").unwrap();
        assert_eq!(identity.name, "NVIDIA L40S");
        assert_eq!(identity.uuid.as_str(), "GPU-1234-abcd");
        assert_eq!(identity.driver_version, "575.57.08");
        assert_eq!(
            NvidiaGpuUuid::from_visible_devices("GPU-1234-abcd\n").unwrap(),
            identity.uuid
        );
    }

    #[test]
    fn rejects_multiple_visible_gpus() {
        let error = parse_single_nvidia_smi_gpu(
            "NVIDIA L40S, GPU-first, 575.57.08\nNVIDIA L40S, GPU-second, 575.57.08\n",
        )
        .unwrap_err();
        assert!(error.to_string().contains("exactly one visible NVIDIA GPU"));
    }

    #[test]
    fn rejects_all_as_an_allocated_uuid() {
        let error = NvidiaGpuUuid::from_visible_devices("all").unwrap_err();
        assert!(error.to_string().contains("one NVIDIA GPU UUID"));
    }
}
