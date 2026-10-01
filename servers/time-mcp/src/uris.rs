/// Well-known surface roots (contract C18, C19). These literals must match
/// `ServerResourceUris` with the validated `time` scheme; a unit test below
/// pins that equivalence.
pub const DOCS_URI: &str = "time://docs";
pub const CONTRACT_URI: &str = "time://contract";

pub const CLOCK_CURRENT_URI: &str = "time://clock/current";
pub const TIMELINE_APP_URI: &str = "ui://time/timeline.html";
pub const CLOCK_QUALITY_URI: &str = "time://clock/quality";
pub const AUTHORITIES_CURRENT_URI: &str = "time://authorities/current";
pub const CALENDARS_URI: &str = "time://calendars";
pub const EPOCHS_URI: &str = "time://epochs";
pub const EVENTS_URI: &str = "time://events";

pub const DOC_TEMPLATE: &str = "time://docs/{doc_id}";
pub const ZONE_TEMPLATE: &str = "time://zones/{+zone_id}";
pub const AUTHORITY_RELEASE_TEMPLATE: &str = "time://authorities/releases/{release_id}";
pub const CALENDARS_TEMPLATE: &str = "time://calendars{?cursor}";
pub const EPOCHS_TEMPLATE: &str = "time://epochs{?cursor}";
pub const EVENTS_TEMPLATE: &str = "time://events{?cursor}";
pub const CALENDAR_TEMPLATE: &str = "time://calendars/{calendar_id}/versions/{version}";
pub const EPOCH_TEMPLATE: &str = "time://epochs/{epoch_id}";
pub const EVENT_TEMPLATE: &str = "time://events/{event_id}";

pub const AUTHORITY_RELEASES_URI: &str = "time://authorities/releases";
pub const BOOTSTRAP_AUTHORITIES_URI: &str = "time://authorities/bootstrap";
pub const AUTHORITY_RELEASES_TEMPLATE: &str = "time://authorities/releases{?cursor}";
pub const BOOTSTRAP_AUTHORITIES_TEMPLATE: &str = "time://authorities/bootstrap{?cursor}";
pub const BOOTSTRAP_AUTHORITY_TEMPLATE: &str = "time://authorities/bootstrap/{release_id}";
pub const EPOCH_VERSION_TEMPLATE: &str = "time://epochs/{epoch_id}/versions/{version}";
