//! Checked observation declarations, independent of database execution.
use crate::{DeclarationError, TableName};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ChangefeedRetention(u32);
impl ChangefeedRetention {
    pub fn from_days(days: u32) -> Result<Self, DeclarationError> {
        days.checked_mul(86_400)
            .filter(|seconds| *seconds > 0)
            .map(Self)
            .ok_or_else(|| {
                DeclarationError::new("changefeed retention must be positive and fit whole seconds")
            })
    }
    pub const fn seconds(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ObservationReplay {
    LiveOnly,
    Changefeed(ChangefeedRetention),
}

/// An owner declaration. Schema readiness and configured retention require
/// installation qualification; identifier admission alone does not prove them.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ObservationTable {
    name: TableName,
    replay: ObservationReplay,
}
impl ObservationTable {
    pub fn new(name: TableName, replay: ObservationReplay) -> Self {
        Self { name, replay }
    }
    pub fn name(&self) -> &TableName {
        &self.name
    }
    pub fn as_str(&self) -> &str {
        self.name.as_str()
    }
    pub const fn replay(&self) -> ObservationReplay {
        self.replay
    }
}
