use std::borrow::Cow;

/// An owner-defined identity admitted from text.
///
/// Implementations keep their domain's accepted spellings and validation error.
/// Admission establishes neither existence nor authority. Secret owners choose
/// explicit text exposure and supply their redacted formatters.
/// Independent owners may implement this trait without a declaration attribute.
/// An unvalidated string cannot be used as a typed identity component:
/// ```compile_fail
/// use veoveo_types::Identity;
/// fn component<I: Identity>(value: &I) { let _ = value.identity_text(); }
/// component(&String::from("unvalidated"));
/// ```
/// A declaration selects public methods and traits; its ordinary profile owns
/// grammar, generation, wire representation, schema and error mapping:
/// ```
/// use veoveo_types::*;
/// struct Names;
/// impl IdProfile for Names {
///     type Error = &'static str;
///     const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
///         if value.is_empty() { Err("invalid name") } else { Ok(()) }
///     });
/// }
/// #[veoveo_types::id(text(Names))]
/// struct Name(String);
/// assert_eq!(Name::parse("example").unwrap().as_str(), "example");
/// assert!(Name::parse("").is_err());
/// ```
/// Attributes reject unsupported declaration shapes before profile expansion:
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Names;
/// # impl IdProfile for Names {
/// #     type Error = &'static str;
/// #     const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
/// #         if value.is_empty() { Err("invalid name") } else { Ok(()) }
/// #     });
/// # }
/// #[veoveo_types::id(text(Names))]
/// enum Invalid { Value }
/// ```
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Names;
/// # impl IdProfile for Names {
/// #     type Error = &'static str;
/// #     const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
/// #         if value.is_empty() { Err("invalid name") } else { Ok(()) }
/// #     });
/// # }
/// #[veoveo_types::id(text(Names))]
/// struct Generic<T>(T);
/// ```
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Names;
/// # impl IdProfile for Names {
/// #     type Error = &'static str;
/// #     const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
/// #         if value.is_empty() { Err("invalid name") } else { Ok(()) }
/// #     });
/// # }
/// #[veoveo_types::id(text(Names))]
/// struct Pair(String, String);
/// ```
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Names;
/// # impl IdProfile for Names {
/// #     type Error = &'static str;
/// #     const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
/// #         if value.is_empty() { Err("invalid name") } else { Ok(()) }
/// #     });
/// # }
/// #[veoveo_types::id(text(Names))]
/// struct WrongInner(u64);
/// ```
/// A selector cannot reinterpret a different admitted grammar:
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Names;
/// # impl IdProfile for Names {
/// #     type Error = &'static str;
/// #     const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::text(|value, _| {
/// #         if value.is_empty() { Err("invalid name") } else { Ok(()) }
/// #     });
/// # }
/// #[veoveo_types::id(hex(Names))]
/// struct WrongGrammar(String);
/// ```
/// Admission profiles are paths to ordinary types, not closure expressions:
/// ```compile_fail
/// #[veoveo_types::id(text(|_| Ok::<_, ()>(()))) ]
/// struct Unchecked(String);
/// ```
/// Generation capabilities must fit the admitted UUID versions:
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Ids;
/// # impl IdProfile for Ids {
/// # type Error = &'static str;
/// # const PROFILE:IdProfileSpec<Self::Error>=IdProfileSpec {
/// # generation:IdGeneration { fresh:FreshId::UuidV7, stable_v5_namespace:Some(uuid::Uuid::nil()) },
/// # ..IdProfileSpec::uuid(UuidGrammar {versions:&[5],variant:UuidVariant::Any,spelling:UuidSpelling::ParserAliases},|_,_,_|"invalid") };
/// # }
/// #[veoveo_types::id(uuid(Ids), fresh)]
/// struct Contradiction(uuid::Uuid);
/// ```
/// Generation capabilities must fit the admitted UUID versions:
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Ids;
/// # impl IdProfile for Ids {
/// # type Error = &'static str;
/// # const PROFILE:IdProfileSpec<Self::Error>=IdProfileSpec {
/// # generation:IdGeneration { fresh:FreshId::UuidV7, stable_v5_namespace:Some(uuid::Uuid::nil()) },
/// # ..IdProfileSpec::uuid(UuidGrammar {versions:&[7],variant:UuidVariant::Any,spelling:UuidSpelling::ParserAliases},|_,_,_|"invalid") };
/// # }
/// #[veoveo_types::id(prefixed(Ids,"item-"), fresh, stable)]
/// struct Contradiction(String);
/// ```
/// Secret declarations require the owner's explicit projection accessor:
/// ```compile_fail
/// # use veoveo_types::*;
/// # struct Names;
/// # impl IdProfile for Names {
/// # type Error=&'static str;
/// # const PROFILE:IdProfileSpec<Self::Error>=IdProfileSpec::text(|_,_|Ok(()));
/// # }
/// #[veoveo_types::id(text(Names),secret)]
/// struct Secret(String);
/// let _ = Secret::parse("bearer").unwrap().as_str();
/// ```
pub trait Identity: Sized {
    type Error;

    fn parse_identity(value: &str) -> Result<Self, Self::Error>;

    fn identity_text(&self) -> Cow<'_, str>;
}
