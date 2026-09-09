//! Official protobuf JSON mapping over the exact generated descriptor.
use crate::{Result, RuntimeFailure, protocol};
use prost_reflect::{DescriptorPool, DynamicMessage};
use std::sync::LazyLock;
static DESCRIPTORS: LazyLock<DescriptorPool> = LazyLock::new(|| {
    DescriptorPool::decode(protocol::FILE_DESCRIPTOR_SET)
        .expect("verified generated OpenShell descriptor")
});

/// Parse installation `template.policy` using protobuf JSON field names, enum
/// names and scalar rules. This parses shape; DevelopmentTemplate performs
/// containment admission. No handwritten JSON mirror or provider model exists.
pub fn parse_policy(value: &serde_json::Value) -> Result<protocol::sandbox::v1::SandboxPolicy> {
    let bytes = serde_json::to_vec(value).map_err(|_| RuntimeFailure::InvalidTemplate)?;
    if bytes.len() > 1024 * 1024 {
        return Err(RuntimeFailure::InvalidTemplate);
    }
    let descriptor = DESCRIPTORS
        .get_message_by_name("openshell.sandbox.v1.SandboxPolicy")
        .expect("pinned policy descriptor");
    let mut json = serde_json::Deserializer::from_slice(&bytes);
    let message = DynamicMessage::deserialize(descriptor, &mut json)
        .map_err(|_| RuntimeFailure::InvalidTemplate)?;
    json.end().map_err(|_| RuntimeFailure::InvalidTemplate)?;
    message
        .transcode_to()
        .map_err(|_| RuntimeFailure::InvalidTemplate)
}
