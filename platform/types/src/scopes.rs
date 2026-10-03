/// Declare a server-owned scope enum with one spelling per variant.
///
/// Serialization, schema values, parsing, display, and `ScopeDefinition` all use
/// that declaration. The consumer needs Serde and Schemars; no runtime is required.
/// Installations may carry other validated names in their grants without parsing
/// those names into the domain enum.
/// Declared spellings must be unique OAuth scope tokens (RFC 6749 section 3.3).
/// Invalid declarations fail compilation; dynamic names keep `ScopeName`'s lexical profile.
/// An empty declaration represents a server with no domain scopes. It accepts no
/// wire value and emits the JSON Schema `false` schema.
///
/// ```
/// use veoveo_types::{ScopeDefinition, scope_enum};
/// scope_enum! {
///     pub enum OrchardScope {
///         Read => "orchard:read",
///         Harvest => "orchard:harvest",
///     }
/// }
/// assert_eq!(OrchardScope::Read.name().as_str(), "orchard:read");
/// assert_eq!("orchard:harvest".parse(), Ok(OrchardScope::Harvest));
/// assert!("another-server:read".parse::<OrchardScope>().is_err());
/// ```
/// ```compile_fail
/// veoveo_types::scope_enum! { enum BadScope { Read => "two scopes" } }
/// ```
/// ```compile_fail
/// veoveo_types::scope_enum! { enum BadScope { Read => r#"read"write"# } }
/// ```
/// ```compile_fail
/// veoveo_types::scope_enum! { enum BadScope { Read => r"read\write" } }
/// ```
/// ```compile_fail
/// veoveo_types::scope_enum! {
///     enum DuplicateScope { Read => "domain:read", Write => "domain:read" }
/// }
/// ```
#[macro_export]
macro_rules! scope_enum {
    ($(#[$attr:meta])* $vis:vis enum $name:ident {
        $($variant:ident => $wire:literal),* $(,)?
    }) => {
        const _: () = {
            let names: &[&str] = &[$($wire),*];
            let mut index = 0;
            while index < names.len() {
                let bytes = names[index].as_bytes();
                assert!(!bytes.is_empty(), "declared scope must not be empty");
                let mut position = 0;
                while position < bytes.len() {
                    assert!(matches!(bytes[position], 0x21 | 0x23..=0x5b | 0x5d..=0x7e),
                        "declared scope must follow OAuth scope-token syntax");
                    position += 1;
                }
                let mut previous = 0;
                while previous < index {
                    let other = names[previous].as_bytes();
                    let mut equal = bytes.len() == other.len();
                    let mut position = 0;
                    while equal && position < bytes.len() {
                        equal = bytes[position] == other[position];
                        position += 1;
                    }
                    assert!(!equal, "declared scopes must have distinct spellings");
                    previous += 1;
                }
                index += 1;
            }
        };
        $(#[$attr])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        $vis enum $name { $($variant),* }

        impl $name {
            $vis const ALL: &'static [Self] = &[$(Self::$variant),*];
        }

        impl $crate::ScopeDefinition for $name {
            fn name(self) -> &'static $crate::ScopeName {
                match self {
                    $(Self::$variant => {
                        static NAME: ::std::sync::LazyLock<$crate::ScopeName> =
                            ::std::sync::LazyLock::new(||
                                $crate::ScopeName::new($wire).expect("invalid declared scope"));
                        &NAME
                    }),*
                }
            }
        }

        impl ::std::convert::TryFrom<&$crate::ScopeName> for $name {
            type Error = $crate::IdentifierError;
            fn try_from(name: &$crate::ScopeName) -> Result<Self, Self::Error> {
                Self::ALL.iter().copied()
                    .find(|value| $crate::ScopeDefinition::name(*value) == name)
                    .ok_or_else(|| $crate::IdentifierError::new(
                        name.as_str(), concat!("unknown ", stringify!($name), " scope")))
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::IdentifierError;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Self::try_from(&$crate::ScopeName::new(value)?)
            }
        }

        impl ::std::convert::From<$name> for $crate::ScopeName {
            fn from(value: $name) -> Self {
                $crate::ScopeDefinition::name(value).clone()
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                ::std::fmt::Display::fmt($crate::ScopeDefinition::name(*self), f)
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                ::serde::Serialize::serialize($crate::ScopeDefinition::name(*self), serializer)
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let name = <$crate::ScopeName as ::serde::Deserialize>::deserialize(deserializer)?;
                Self::try_from(&name).map_err(::serde::de::Error::custom)
            }
        }

        impl ::schemars::JsonSchema for $name {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed(stringify!($name))
            }
            fn schema_id() -> ::std::borrow::Cow<'static, str> {
                ::std::borrow::Cow::Borrowed(concat!(module_path!(), "::", stringify!($name)))
            }
            fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                if Self::ALL.is_empty() {
                    return false.into();
                }
                ::schemars::json_schema!({
                    "type": "string",
                    "enum": Self::ALL.iter().map(|value|
                        $crate::ScopeDefinition::name(*value).as_str()).collect::<Vec<_>>()
                })
            }
        }
    };
}
