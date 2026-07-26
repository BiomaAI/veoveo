use schemars::{Schema, schema_for};

use crate::{CompatibilityManifest, ExtensionReleaseManifest};

/// Generates the canonical compatibility-manifest JSON Schema.
#[must_use]
pub fn compatibility_manifest_schema() -> Schema {
    schema_for!(CompatibilityManifest)
}

/// Generates the canonical extension-release JSON Schema.
#[must_use]
pub fn extension_release_schema() -> Schema {
    schema_for!(ExtensionReleaseManifest)
}
