//! Compact catalog metadata; full geometry belongs to the exact resource.
use super::{
    MapRestrictionError, MapRestrictionUri, MobilityFamily, Restriction, RestrictionEffectKind,
    RestrictionId, RestrictionKind,
};
use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "SummaryWire", into = "SummaryWire")]
pub struct RestrictionSummary {
    uri: MapRestrictionUri,
    kind: RestrictionKind,
    effect_kind: RestrictionEffectKind,
    affected_mobility_families: BTreeSet<MobilityFamily>,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    cancelled_by: Option<RestrictionId>,
    record_version: u64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SummaryWire {
    restriction_id: RestrictionId,
    resource_uri: MapRestrictionUri,
    kind: RestrictionKind,
    effect_kind: RestrictionEffectKind,
    #[schemars(length(min = 1))]
    affected_mobility_families: BTreeSet<MobilityFamily>,
    valid_from: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    cancelled_by: Option<RestrictionId>,
    #[schemars(range(min = 1))]
    record_version: u64,
}

impl RestrictionSummary {
    pub fn new(record: &Restriction) -> Result<Self, MapRestrictionError> {
        SummaryWire {
            restriction_id: record.restriction_id.clone(),
            resource_uri: MapRestrictionUri::new(record.restriction_id.clone()),
            kind: record.kind,
            effect_kind: record.effect.kind,
            affected_mobility_families: record.affected_mobility_families.clone(),
            valid_from: record.valid_from,
            valid_until: record.valid_until,
            cancelled_by: record.cancelled_by.clone(),
            record_version: record.record_version,
        }
        .try_into()
    }
    pub fn restriction_id(&self) -> &RestrictionId {
        self.uri.id()
    }
    pub fn resource_uri(&self) -> &MapRestrictionUri {
        &self.uri
    }
    pub fn kind(&self) -> RestrictionKind {
        self.kind
    }
    pub fn effect_kind(&self) -> RestrictionEffectKind {
        self.effect_kind
    }
    pub fn affected_mobility_families(&self) -> &BTreeSet<MobilityFamily> {
        &self.affected_mobility_families
    }
    pub fn valid_from(&self) -> DateTime<Utc> {
        self.valid_from
    }
    pub fn valid_until(&self) -> Option<DateTime<Utc>> {
        self.valid_until
    }
    pub fn cancelled_by(&self) -> Option<&RestrictionId> {
        self.cancelled_by.as_ref()
    }
    pub fn record_version(&self) -> u64 {
        self.record_version
    }
}
impl TryFrom<SummaryWire> for RestrictionSummary {
    type Error = MapRestrictionError;
    fn try_from(wire: SummaryWire) -> Result<Self, Self::Error> {
        if &wire.restriction_id != wire.resource_uri.id()
            || wire.record_version == 0
            || wire.affected_mobility_families.is_empty()
            || wire.valid_until.is_some_and(|end| end <= wire.valid_from)
        {
            return Err(MapRestrictionError::Metadata);
        }
        Ok(Self {
            uri: wire.resource_uri,
            kind: wire.kind,
            effect_kind: wire.effect_kind,
            affected_mobility_families: wire.affected_mobility_families,
            valid_from: wire.valid_from,
            valid_until: wire.valid_until,
            cancelled_by: wire.cancelled_by,
            record_version: wire.record_version,
        })
    }
}
impl From<RestrictionSummary> for SummaryWire {
    fn from(value: RestrictionSummary) -> Self {
        Self {
            restriction_id: value.uri.id().clone(),
            resource_uri: value.uri,
            kind: value.kind,
            effect_kind: value.effect_kind,
            affected_mobility_families: value.affected_mobility_families,
            valid_from: value.valid_from,
            valid_until: value.valid_until,
            cancelled_by: value.cancelled_by,
            record_version: value.record_version,
        }
    }
}
