use serde::{Deserialize, Serialize};
use veoveo_artifact_contract::{ArtifactUploadReceipt, ArtifactUri};
use veoveo_browser_smoke::HardwareIdentity;

#[derive(Clone, Copy, Debug, Eq, PartialEq, veoveo_types::Vocabulary)]
pub enum BrowserUploadReportSchema {
    #[vocabulary(rename = "veoveo.ai/console-artifact-upload-acceptance/v2")]
    Upload,
    #[vocabulary(rename = "veoveo.ai/console-artifact-upload-ux-acceptance/v2")]
    Ux,
    #[vocabulary(rename = "veoveo.ai/console-artifact-upload-resume-acceptance/v2")]
    Resume,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UploadPolicyObservation {
    pub status: u16,
    pub allowed: bool,
    pub explanation: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrowserUploadReport {
    pub schema: BrowserUploadReportSchema,
    pub source_revision: String,
    pub page_url: String,
    pub hardware: HardwareIdentity,
    pub policy: UploadPolicyObservation,
    pub preflight_only: bool,
    pub steps: Vec<String>,
    pub screenshots: Vec<(String, String)>,
    pub accepted_before_reload: u64,
    pub elapsed_seconds: f64,
    pub large_receipt: Option<ArtifactUploadReceipt>,
    pub csv_receipt: Option<ArtifactUploadReceipt>,
}

impl BrowserUploadReport {
    /// Copied receipts must preserve the selected occurrence identity. This is a
    /// portable relationship check, not proof of current authorization or GPU use.
    pub fn check_receipts(&self) -> anyhow::Result<()> {
        for receipt in self.large_receipt.iter().chain(self.csv_receipt.iter()) {
            anyhow::ensure!(
                receipt.artifact_uri == ArtifactUri::plane(receipt.artifact_id),
                "upload report receipt occurrence mismatch"
            );
        }
        if let (Some(large), Some(csv)) = (&self.large_receipt, &self.csv_receipt) {
            anyhow::ensure!(
                large.upload_id != csv.upload_id && large.artifact_id != csv.artifact_id,
                "upload report receipts reuse one selected upload or occurrence"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_artifact_contract::{ArtifactId, ArtifactUploadId, UploadSha256};

    fn produced() -> BrowserUploadReport {
        let id = ArtifactId::new();
        BrowserUploadReport {
            schema: BrowserUploadReportSchema::Upload,
            source_revision: "fixture".into(),
            page_url: "https://example.invalid/console/".into(),
            // A typed decoder fixture, not a hardware qualification observation.
            hardware: serde_json::from_value(serde_json::json!({
                "userAgent":"fixture", "webgpuVendor":"fixture", "webgpuArchitecture":"fixture",
                "webgpuDevice":"fixture", "webgpuDescription":"fixture", "webglAvailable":false,
                "webglVendor":"fixture", "webglRenderer":"fixture"
            }))
            .unwrap(),
            policy: UploadPolicyObservation {
                status: 200,
                allowed: true,
                explanation: String::new(),
            },
            preflight_only: false,
            steps: vec![],
            screenshots: vec![],
            accepted_before_reload: 0,
            elapsed_seconds: 0.0,
            large_receipt: Some(ArtifactUploadReceipt {
                upload_id: ArtifactUploadId::new(),
                artifact_id: id,
                artifact_uri: ArtifactUri::plane(id),
                sha256: UploadSha256::parse("a".repeat(64)).unwrap(),
                byte_len: 1,
                mime_type: "application/octet-stream".into(),
                filename: "fixture.bin".into(),
                created_at: chrono::Utc::now(),
            }),
            csv_receipt: None,
        }
    }

    #[test]
    fn upload_report_receives_actual_owner_receipts_and_refuses_retired_or_mixed_fields() {
        let report = produced();
        report.check_receipts().unwrap();
        let current = serde_json::to_value(report).unwrap();
        for (parent, key, retired) in [
            ("", "sourceRevision", "source_revision"),
            ("", "largeReceipt", "large_receipt"),
            ("/hardware", "userAgent", "user_agent"),
            ("/hardware", "webgpuVendor", "webgpu_vendor"),
            ("/hardware", "webgpuArchitecture", "webgpu_architecture"),
            ("/hardware", "webgpuDevice", "webgpu_device"),
            ("/hardware", "webgpuDescription", "webgpu_description"),
            ("/hardware", "webglAvailable", "webgl_available"),
            ("/hardware", "webglVendor", "webgl_vendor"),
            ("/hardware", "webglRenderer", "webgl_renderer"),
            ("/largeReceipt", "artifactId", "artifact_id"),
            ("/largeReceipt", "artifactUri", "artifact_uri"),
            ("/largeReceipt", "uploadId", "upload_id"),
            ("/largeReceipt", "byteLen", "byte_len"),
            ("/largeReceipt", "mimeType", "mime_type"),
            ("/largeReceipt", "createdAt", "created_at"),
        ] {
            for keep_current in [false, true] {
                let mut bad = current.clone();
                let fields = bad.pointer_mut(parent).unwrap().as_object_mut().unwrap();
                let value = fields.get(key).unwrap().clone();
                if !keep_current {
                    fields.remove(key);
                }
                fields.insert(retired.to_owned(), value);
                assert!(
                    serde_json::from_str::<BrowserUploadReport>(
                        &serde_json::to_string(&bad).unwrap()
                    )
                    .is_err()
                );
                assert!(serde_json::from_value::<BrowserUploadReport>(bad).is_err());
            }
        }
        let mut old = current.clone();
        old["schema"] = "veoveo.ai/console-artifact-upload-acceptance/v1".into();
        assert!(serde_json::from_value::<BrowserUploadReport>(old).is_err());
        let mut foreign = current.clone();
        foreign["largeReceipt"]["artifactUri"] =
            serde_json::to_value(ArtifactUri::plane(ArtifactId::new())).unwrap();
        let admitted_shape: BrowserUploadReport = serde_json::from_value(foreign).unwrap();
        assert!(admitted_shape.check_receipts().is_err());
        let mut duplicate = current;
        duplicate["csvReceipt"] = duplicate["largeReceipt"].clone();
        let admitted_shape: BrowserUploadReport = serde_json::from_value(duplicate).unwrap();
        assert!(admitted_shape.check_receipts().is_err());
    }
}
