use std::borrow::Cow;

/// An owner-defined identity admitted from text.
///
/// Implementations keep their domain's accepted spellings and validation error.
/// Admission establishes neither existence nor authority. Text projection is an
/// explicit exposure operation; secret owners may redact Display separately.
/// Independently developed owners may implement this trait without a derive.
/// An unvalidated string cannot be used as a typed identity component:
/// ```compile_fail
/// use veoveo_types::Identity;
/// fn component<I: Identity>(value: &I) { let _ = value.identity_text(); }
/// component(&String::from("unvalidated"));
/// ```
/// The derive accepts only an owner-admitted, nongeneric tuple newtype:
/// ```compile_fail
/// #[derive(veoveo_types::Id)]
/// #[id(string, validate = |_| Ok::<_, ()>(()), error = ())]
/// enum Invalid { Value }
/// ```
/// ```compile_fail
/// #[derive(veoveo_types::Id)]
/// #[id(admit = |_| Ok::<_, ()>(()), error = ())]
/// struct Generic<T>(T);
/// ```
/// ```compile_fail
/// #[derive(veoveo_types::Id)]
/// #[id(string, validate = |_| Ok::<_, ()>(()), error = ())]
/// struct Pair(String, String);
/// ```
/// ```compile_fail
/// #[derive(veoveo_types::Id)]
/// #[id(string, error = ())]
/// struct Unchecked(String);
/// ```
/// ```compile_fail
/// #[derive(veoveo_types::Id)]
/// #[id(string, validate = |_| Ok::<_, ()>(()), error = ())]
/// struct WrongInner(u64);
/// ```
pub trait Identity: Sized {
    type Error;

    fn parse_identity(value: &str) -> Result<Self, Self::Error>;

    fn identity_text(&self) -> Cow<'_, str>;
}
