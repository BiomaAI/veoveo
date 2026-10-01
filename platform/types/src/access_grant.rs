use crate::{AccessLevel, AccessSubject};
use chrono::{DateTime, Utc};

/// The authorization facts a domain grant supplies to the shared evaluator.
/// Resource identity and persistence remain in the owning domain.
pub trait AccessGrant {
    fn subject(&self) -> &AccessSubject;
    fn level(&self) -> AccessLevel;
    fn expires_at(&self) -> Option<DateTime<Utc>>;
}
