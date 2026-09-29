//! Immutable models whose Rust and JSON admission share the same builder.

macro_rules! checked_model {
    ($name:ident, $builder:ident) => {
        #[derive(Clone, Debug, serde::Serialize, schemars::JsonSchema)]
        #[serde(transparent)]
        #[schemars(transparent)]
        pub struct $name($builder);

        impl std::ops::Deref for $name {
            type Target = $builder;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                <$builder as serde::Deserialize>::deserialize(deserializer)?
                    .build()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}
pub(super) use checked_model;

pub(super) fn text(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.trim() == value
        && value.len() <= max
        && !value.chars().any(char::is_control)
}
