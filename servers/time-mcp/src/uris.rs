/// Well-known surface roots (contract C18, C19). These literals must match
/// `ServerResourceUris` with the validated `time` scheme; a unit test below
/// pins that equivalence.
pub const DOCS_URI: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_DOCS;
pub const CONTRACT_URI: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_CONTRACT;

pub const CLOCK_CURRENT_URI: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_CLOCK_CURRENT;
pub const TIMELINE_APP_URI: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_TIMELINE_APP;
pub const CLOCK_QUALITY_URI: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_CLOCK_QUALITY;
pub const AUTHORITIES_CURRENT_URI: &str =
    crate::contract::TimeResource::RESOURCE_TEMPLATE_AUTHORITIES_CURRENT;
pub const CALENDARS_URI: &str = "time://calendars";
pub const EPOCHS_URI: &str = "time://epochs";
pub const EVENTS_URI: &str = "time://events";

pub const DOC_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_DOCUMENT;
pub const ZONE_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_ZONE;
pub const AUTHORITY_RELEASE_TEMPLATE: &str =
    crate::contract::TimeResource::RESOURCE_TEMPLATE_AUTHORITY_RELEASE;
pub const CALENDARS_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_CALENDARS;
pub const EPOCHS_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_EPOCHS;
pub const EVENTS_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_EVENTS;
pub const CALENDAR_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_CALENDAR;
pub const EPOCH_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_EPOCH;
pub const EVENT_TEMPLATE: &str = crate::contract::TimeResource::RESOURCE_TEMPLATE_EVENT;

pub const AUTHORITY_RELEASES_URI: &str = "time://authorities/releases";
pub const BOOTSTRAP_AUTHORITIES_URI: &str = "time://authorities/bootstrap";
pub const AUTHORITY_RELEASES_TEMPLATE: &str =
    crate::contract::TimeResource::RESOURCE_TEMPLATE_AUTHORITY_RELEASES;
pub const BOOTSTRAP_AUTHORITIES_TEMPLATE: &str =
    crate::contract::TimeResource::RESOURCE_TEMPLATE_BOOTSTRAP_AUTHORITIES;
pub const BOOTSTRAP_AUTHORITY_TEMPLATE: &str =
    crate::contract::TimeResource::RESOURCE_TEMPLATE_BOOTSTRAP_AUTHORITY;
pub const EPOCH_VERSION_TEMPLATE: &str =
    crate::contract::TimeResource::RESOURCE_TEMPLATE_EPOCH_VERSION;
