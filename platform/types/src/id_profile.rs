//! Owner admission, UUID/hex mechanics and immutable identity wire/schema policy.
use uuid::{Uuid, Variant};

pub trait IdProfile: Sized {
    type Error;
    const PROFILE: IdProfileSpec<Self::Error>;
}
#[derive(Clone, Copy, Debug)]
pub struct IdMetadata {
    pub type_name: &'static str,
    pub prefix: &'static str,
    pub error_context: &'static str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdFailure {
    Prefix,
    ParseUuid,
    UuidVersion,
    UuidVariant,
    UuidSpelling,
    Length,
    Character,
    HexCase,
    Zero,
}
pub struct IdProfileSpec<E> {
    pub grammar: IdGrammar<E>,
    pub generation: IdGeneration,
    pub wire: IdWire,
    pub schema: IdSchema,
    pub schema_id: Option<fn(IdMetadata) -> std::borrow::Cow<'static, str>>,
    pub canonical: bool,
}
impl<E> IdProfileSpec<E> {
    /// Complete owner validator; String wire/schema, no generation or normalization.
    pub const fn text(validate: fn(&str, IdMetadata) -> Result<(), E>) -> Self {
        Self::base(IdGrammar::Text(validate))
    }
    /// UUID grammar; String wire/schema until explicitly changed by the owner.
    pub const fn uuid(grammar: UuidGrammar, error: fn(&str, IdMetadata, IdFailure) -> E) -> Self {
        Self::base(IdGrammar::Uuid(grammar, error))
    }
    /// UUID grammar with fresh UUIDv7 generation; owners select admission separately.
    pub const fn generated_uuid(
        grammar: UuidGrammar,
        error: fn(&str, IdMetadata, IdFailure) -> E,
    ) -> Self {
        let mut profile = Self::uuid(grammar, error);
        profile.generation.fresh = FreshId::UuidV7;
        profile
    }
    /// An owner supplies the complete schema without changing admission or wire policy.
    pub const fn owner_schema(
        mut self,
        schema: fn(&mut schemars::SchemaGenerator, IdMetadata) -> schemars::Schema,
        inline: bool,
    ) -> Self {
        self.schema = IdSchema::Owner { schema, inline };
        self
    }
    /// Hex grammar; String wire/schema until explicitly changed by the owner.
    pub const fn hex(grammar: HexGrammar, error: fn(&str, IdMetadata, IdFailure) -> E) -> Self {
        Self::base(IdGrammar::Hex(grammar, error))
    }
    const fn base(grammar: IdGrammar<E>) -> Self {
        Self {
            grammar,
            generation: IdGeneration {
                fresh: FreshId::None,
                stable_v5_namespace: None,
            },
            wire: IdWire::String,
            schema: IdSchema::DerivedString,
            schema_id: None,
            canonical: false,
        }
    }
}
impl<E> Copy for IdProfileSpec<E> {}
impl<E> Clone for IdProfileSpec<E> {
    fn clone(&self) -> Self {
        *self
    }
}
pub enum IdGrammar<E> {
    Text(fn(&str, IdMetadata) -> Result<(), E>),
    Uuid(UuidGrammar, fn(&str, IdMetadata, IdFailure) -> E),
    Hex(HexGrammar, fn(&str, IdMetadata, IdFailure) -> E),
}
impl<E> Copy for IdGrammar<E> {}
impl<E> Clone for IdGrammar<E> {
    fn clone(&self) -> Self {
        *self
    }
}
#[derive(Clone, Copy, Debug)]
pub struct UuidGrammar {
    pub versions: &'static [usize],
    pub variant: UuidVariant,
    pub spelling: UuidSpelling,
}
impl UuidGrammar {
    /// RFC UUID admission with canonical lowercase hyphenated spelling.
    pub const fn canonical(versions: &'static [usize]) -> Self {
        Self {
            versions,
            variant: UuidVariant::Rfc4122,
            spelling: UuidSpelling::CanonicalLowerHyphenated,
        }
    }
    /// RFC UUID admission with every spelling accepted by the UUID parser.
    pub const fn aliases(versions: &'static [usize]) -> Self {
        Self {
            versions,
            variant: UuidVariant::Rfc4122,
            spelling: UuidSpelling::ParserAliases,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UuidVariant {
    Any,
    Rfc4122,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UuidSpelling {
    ParserAliases,
    CanonicalLowerHyphenated,
}
#[derive(Clone, Copy, Debug)]
pub struct HexGrammar {
    pub length: usize,
    pub case: HexCase,
    pub nonzero: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum HexCase {
    Lower,
    Upper,
    Either,
}
#[derive(Clone, Copy, Debug)]
pub struct IdGeneration {
    pub fresh: FreshId,
    pub stable_v5_namespace: Option<Uuid>,
}
#[derive(Clone, Copy, Debug)]
pub enum FreshId {
    None,
    UuidV7,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdWire {
    String,
    InnerUuid,
    UuidOutStringIn,
}
#[derive(Clone, Copy)]
pub enum IdSchema {
    DerivedString,
    DerivedUuid,
    UuidPattern {
        pattern: &'static str,
    },
    Hex {
        pattern: &'static str,
    },
    Owner {
        schema: fn(&mut schemars::SchemaGenerator, IdMetadata) -> schemars::Schema,
        inline: bool,
    },
}
fn fail<P: IdProfile>(value: &str, meta: IdMetadata, reason: IdFailure) -> P::Error {
    match P::PROFILE.grammar {
        IdGrammar::Uuid(_, error) | IdGrammar::Hex(_, error) => error(value, meta, reason),
        IdGrammar::Text(_) => unreachable!("text admission delegates its complete validator"),
    }
}
pub fn admit_profile_uuid<P: IdProfile>(value: &str, meta: IdMetadata) -> Result<Uuid, P::Error> {
    let tail = value
        .strip_prefix(meta.prefix)
        .ok_or_else(|| fail::<P>(value, meta, IdFailure::Prefix))?;
    let uuid = Uuid::parse_str(tail).map_err(|_| fail::<P>(value, meta, IdFailure::ParseUuid))?;
    let IdGrammar::Uuid(grammar, _) = P::PROFILE.grammar else {
        panic!("UUID storage requires UUID grammar")
    };
    if !grammar.versions.is_empty() && !grammar.versions.contains(&uuid.get_version_num()) {
        return Err(fail::<P>(value, meta, IdFailure::UuidVersion));
    }
    if grammar.variant == UuidVariant::Rfc4122 && uuid.get_variant() != Variant::RFC4122 {
        return Err(fail::<P>(value, meta, IdFailure::UuidVariant));
    }
    if grammar.spelling == UuidSpelling::CanonicalLowerHyphenated && tail != uuid.to_string() {
        return Err(fail::<P>(value, meta, IdFailure::UuidSpelling));
    }
    Ok(uuid)
}
fn check_profile_string<P: IdProfile>(
    value: &str,
    meta: IdMetadata,
) -> Result<Option<String>, P::Error> {
    match P::PROFILE.grammar {
        IdGrammar::Text(validate) => validate(value, meta)?,
        IdGrammar::Uuid(_, _) => {
            let uuid = admit_profile_uuid::<P>(value, meta)?;
            if P::PROFILE.canonical {
                return Ok(Some(format!("{}{uuid}", meta.prefix)));
            }
        }
        IdGrammar::Hex(grammar, _) => {
            let tail = value
                .strip_prefix(meta.prefix)
                .ok_or_else(|| fail::<P>(value, meta, IdFailure::Prefix))?;
            let failure = if tail.len() != grammar.length {
                Some(IdFailure::Length)
            } else if !tail.bytes().all(|b| b.is_ascii_hexdigit()) {
                Some(IdFailure::Character)
            } else if match grammar.case {
                HexCase::Lower => tail.bytes().any(|b| b.is_ascii_uppercase()),
                HexCase::Upper => tail.bytes().any(|b| b.is_ascii_lowercase()),
                HexCase::Either => false,
            } {
                Some(IdFailure::HexCase)
            } else if grammar.nonzero && tail.bytes().all(|b| b == b'0') {
                Some(IdFailure::Zero)
            } else {
                None
            };
            if let Some(failure) = failure {
                return Err(fail::<P>(value, meta, failure));
            }
        }
    }
    Ok(None)
}
pub fn admit_profile_owned_string<P: IdProfile>(
    value: String,
    meta: IdMetadata,
) -> Result<String, P::Error> {
    Ok(check_profile_string::<P>(&value, meta)?.unwrap_or(value))
}
pub fn admit_profile_string<P: IdProfile>(
    value: &str,
    meta: IdMetadata,
) -> Result<String, P::Error> {
    Ok(check_profile_string::<P>(value, meta)?.unwrap_or_else(|| value.to_owned()))
}
pub const fn uuid_grammar_accepts_version<E>(grammar: IdGrammar<E>, version: usize) -> bool {
    let IdGrammar::Uuid(grammar, _) = grammar else {
        return false;
    };
    if grammar.versions.is_empty() {
        return true;
    }
    let mut i = 0;
    while i < grammar.versions.len() {
        if grammar.versions[i] == version {
            return true;
        }
        i += 1;
    }
    false
}
pub fn fresh_profile_uuid<P: IdProfile>(_: IdMetadata) -> Uuid {
    Uuid::now_v7()
}
pub fn fresh_profile_string<P: IdProfile>(meta: IdMetadata) -> String {
    format!("{}{}", meta.prefix, Uuid::now_v7())
}
pub fn stable_profile_string<P: IdProfile>(key: &[u8], meta: IdMetadata) -> String {
    let namespace = P::PROFILE
        .generation
        .stable_v5_namespace
        .expect("stable capability requires namespace");
    let value = format!("{}{}", meta.prefix, Uuid::new_v5(&namespace, key));
    admit_profile_owned_string::<P>(value, meta)
        .unwrap_or_else(|_| panic!("owner generator must produce an admitted identity"))
}
pub trait UuidIdentity: crate::Identity {
    fn as_uuid(&self) -> Uuid;
}
pub trait StableKeyIdentity: crate::Identity {
    fn from_stable_key(key: &[u8]) -> Self;
}
