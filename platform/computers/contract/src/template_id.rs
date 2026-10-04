use schemars::{Schema, SchemaGenerator};
use veoveo_types::{IdMetadata, IdProfile, IdProfileSpec, IdSchema};

#[doc(hidden)]
pub struct TemplateNames;
impl IdProfile for TemplateNames {
    type Error = TemplateIdError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec {
        schema: IdSchema::Owner {
            schema: template_schema,
            inline: false,
        },
        ..IdProfileSpec::text(|value, _| validate_template_name(value))
    };
}
fn template_schema(_: &mut SchemaGenerator, _: IdMetadata) -> Schema {
    schemars::json_schema!({
        "type": "string", "pattern": "^[a-z0-9][a-z0-9-]{0,63}$",
        "minLength": 1, "maxLength": 64
    })
}

#[veoveo_types::id(text(TemplateNames), inner_display)]
pub struct TemplateId(String);

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error(
    "expected a Computer template name of 1–64 lowercase ASCII letters, digits or hyphens, beginning with a letter or digit"
)]
pub struct TemplateIdError;

fn validate_template_name(value: &str) -> Result<(), TemplateIdError> {
    if value.is_empty()
        || value.len() > 64
        || !value.as_bytes()[0].is_ascii_alphanumeric()
        || !value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err(TemplateIdError);
    }
    Ok(())
}
