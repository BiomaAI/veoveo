//! Immutable admitted cursor text with an owner-selected codec and context.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Owners define position admission, envelopes, encoding and alias policy.
/// Contextual codecs store typed context and receive it through `&self`.
/// Implementations prevent interior mutation of admitted context and positions.
pub trait CursorCodec {
    type Position;
    type Error;
    fn check(&self, position: &Self::Position) -> Result<(), Self::Error>;
    fn encode(&self, position: &Self::Position) -> Result<String, Self::Error>;
    fn decode(&self, wire: &str) -> Result<Self::Position, Self::Error>;
}

/// Explicitly admits parsing without caller-supplied context.
///
/// Implement only when the default codec completely determines admission.
/// `Default` alone does not grant deserialization to contextual codecs.
/// ```compile_fail
/// use veoveo_types::{CursorCodec, OpaqueCursor};
/// #[derive(Default)]
/// struct ContextualCodec { session: u32 }
/// impl CursorCodec for ContextualCodec {
///     type Position = u32;
///     type Error = &'static str;
///     fn check(&self, _: &u32) -> Result<(), Self::Error> { Ok(()) }
///     fn encode(&self, position: &u32) -> Result<String, Self::Error> {
///         Ok(format!("{}:{position}", self.session))
///     }
///     fn decode(&self, _: &str) -> Result<u32, Self::Error> { Err("context required") }
/// }
/// let _: OpaqueCursor<ContextualCodec> = serde_json::from_str("\"7:1\"").unwrap();
/// ```
pub trait StatelessCursorCodec: CursorCodec + Default {}

/// Parsing retains the admitted original spelling. Owners decide whether decode
/// requires canonical re-encoding equality. Deserialization requires explicit
/// stateless admission; contextual parsing needs the owner's codec instance.
/// Owners prevent interior mutation of stored context and positions that would
/// invalidate admission.
/// ```compile_fail
/// use veoveo_types::{CursorCodec, OpaqueCursor};
/// fn mutate<C: CursorCodec>(cursor: &mut OpaqueCursor<C>, value: C::Position) {
///     *cursor.position() = value;
/// }
/// ```
pub struct OpaqueCursor<C: CursorCodec> {
    codec: C,
    wire: String,
    position: C::Position,
}
impl<C: CursorCodec + Clone> Clone for OpaqueCursor<C>
where
    C::Position: Clone,
{
    fn clone(&self) -> Self {
        Self {
            codec: self.codec.clone(),
            wire: self.wire.clone(),
            position: self.position.clone(),
        }
    }
}
impl<C: CursorCodec + std::fmt::Debug> std::fmt::Debug for OpaqueCursor<C>
where
    C::Position: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpaqueCursor")
            .field("codec", &self.codec)
            .field("wire", &self.wire)
            .field("position", &self.position)
            .finish()
    }
}
impl<C: CursorCodec + PartialEq> PartialEq for OpaqueCursor<C>
where
    C::Position: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.codec == other.codec && self.wire == other.wire && self.position == other.position
    }
}
impl<C: CursorCodec + Eq> Eq for OpaqueCursor<C> where C::Position: Eq {}

impl<C: CursorCodec> Serialize for OpaqueCursor<C> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.wire.serialize(serializer)
    }
}

impl<'de, C: StatelessCursorCodec> Deserialize<'de> for OpaqueCursor<C>
where
    C::Error: std::fmt::Display,
{
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(C::default(), String::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

impl<C: CursorCodec> OpaqueCursor<C> {
    pub fn try_new(codec: C, position: C::Position) -> Result<Self, C::Error> {
        codec.check(&position)?;
        let wire = codec.encode(&position)?;
        Ok(Self {
            codec,
            wire,
            position,
        })
    }
    pub fn parse(codec: C, wire: impl Into<String>) -> Result<Self, C::Error> {
        let wire = wire.into();
        let position = codec.decode(&wire)?;
        codec.check(&position)?;
        Ok(Self {
            codec,
            wire,
            position,
        })
    }
    pub fn codec(&self) -> &C {
        &self.codec
    }
    pub fn position(&self) -> &C::Position {
        &self.position
    }
    pub fn as_str(&self) -> &str {
        &self.wire
    }
    pub fn into_wire(self) -> String {
        self.wire
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;

    #[derive(Clone, Debug, Default, PartialEq, Eq)]
    struct StatelessCodec;
    impl CursorCodec for StatelessCodec {
        type Position = u32;
        type Error = &'static str;
        fn check(&self, value: &u32) -> Result<(), Self::Error> {
            if *value == 0 { Err("zero") } else { Ok(()) }
        }
        fn encode(&self, value: &u32) -> Result<String, Self::Error> {
            Ok(value.to_string())
        }
        fn decode(&self, wire: &str) -> Result<u32, Self::Error> {
            wire.parse().map_err(|_| "position")
        }
    }
    impl StatelessCursorCodec for StatelessCodec {}

    /// An independent owner's nominal cursor.
    #[derive(Debug, Serialize, Deserialize, JsonSchema)]
    #[serde(transparent)]
    #[schemars(rename = "OwnerCursor")]
    struct OwnerCursor {
        #[schemars(with = "String")]
        cursor: OpaqueCursor<StatelessCodec>,
    }

    /// An independent owner's nominal cursor.
    #[derive(JsonSchema)]
    #[serde(try_from = "String", into = "String")]
    #[schemars(rename = "OwnerCursor")]
    struct PreviousOwnerCursor;

    #[test]
    fn stateless_serde_preserves_wire_and_runs_owner_admission() {
        let owner: OwnerCursor = serde_json::from_str("\"0007\"").unwrap();
        assert_eq!(*owner.cursor.position(), 7);
        assert_eq!(serde_json::to_string(&owner).unwrap(), "\"0007\"");
        assert_eq!(
            serde_json::from_str::<OwnerCursor>("\"0\"")
                .unwrap_err()
                .to_string(),
            "zero"
        );
        assert!(serde_json::from_str::<OwnerCursor>("7").is_err());
        assert!(serde_json::from_str::<OwnerCursor>("\"invalid\"").is_err());
    }

    #[test]
    fn nominal_transparent_schema_matches_prior_string_conversion_profile() {
        assert_eq!(
            OwnerCursor::schema_name(),
            PreviousOwnerCursor::schema_name()
        );
        assert_eq!(OwnerCursor::schema_id(), PreviousOwnerCursor::schema_id());
        assert_eq!(
            OwnerCursor::inline_schema(),
            PreviousOwnerCursor::inline_schema()
        );
        for contract in [
            schemars::generate::Contract::Deserialize,
            schemars::generate::Contract::Serialize,
        ] {
            let mut settings = schemars::generate::SchemaSettings::default();
            settings.contract = contract;
            let mut generator = settings.into_generator();
            assert_eq!(
                OwnerCursor::json_schema(&mut generator),
                PreviousOwnerCursor::json_schema(&mut generator)
            );
            assert_eq!(
                generator.root_schema_for::<OwnerCursor>(),
                generator.root_schema_for::<PreviousOwnerCursor>()
            );
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Codec {
        session: u32,
        canonical: bool,
    }
    impl CursorCodec for Codec {
        type Position = u32;
        type Error = &'static str;
        fn check(&self, value: &u32) -> Result<(), Self::Error> {
            if *value == 0 { Err("zero") } else { Ok(()) }
        }
        fn encode(&self, value: &u32) -> Result<String, Self::Error> {
            Ok(format!("{}:{value}", self.session))
        }
        fn decode(&self, wire: &str) -> Result<u32, Self::Error> {
            let (session, position) = wire.split_once(':').ok_or("shape")?;
            if session.parse::<u32>().map_err(|_| "session")? != self.session {
                return Err("session");
            }
            let value = position.parse().map_err(|_| "position")?;
            if self.canonical && self.encode(&value)? != wire {
                return Err("alias");
            }
            Ok(value)
        }
    }
    #[test]
    fn owner_context_admission_and_alias_policy_are_shared_by_construction_and_parse() {
        let codec = Codec {
            session: 7,
            canonical: false,
        };
        assert_eq!(OpaqueCursor::try_new(codec.clone(), 0), Err("zero"));
        assert_eq!(OpaqueCursor::parse(codec.clone(), "7:0"), Err("zero"));
        assert_eq!(OpaqueCursor::parse(codec.clone(), "8:1"), Err("session"));
        let parsed = OpaqueCursor::parse(codec.clone(), "07:01").unwrap();
        assert_eq!(parsed.as_str(), "07:01");
        assert_eq!(*parsed.position(), 1);
        assert_eq!(parsed.codec(), &codec);
        assert_eq!(OpaqueCursor::try_new(codec, 1).unwrap().into_wire(), "7:1");
        assert_eq!(
            OpaqueCursor::parse(
                Codec {
                    session: 7,
                    canonical: true
                },
                "07:01"
            ),
            Err("alias")
        );
    }
}
