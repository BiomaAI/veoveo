pub const CLOCK_CURRENT_URI: &str = "time://clock/current";
pub const CLOCK_QUALITY_URI: &str = "time://clock/quality";
pub const AUTHORITIES_CURRENT_URI: &str = "time://authorities/current";
pub const CALENDARS_URI: &str = "time://calendars";
pub const EPOCHS_URI: &str = "time://epochs";
pub const EVENTS_URI: &str = "time://events";

pub const ZONE_TEMPLATE: &str = "time://zones/{zone_id}";
pub const CALENDAR_TEMPLATE: &str = "time://calendars/{calendar_id}/versions/{version}";
pub const EPOCH_TEMPLATE: &str = "time://epochs/{epoch_id}";
pub const EVENT_TEMPLATE: &str = "time://events/{event_id}";

pub fn zone_uri(zone_id: &str) -> String {
    format!("time://zones/{zone_id}")
}

pub fn calendar_uri(calendar_id: &str, version: u64) -> String {
    format!("time://calendars/{calendar_id}/versions/{version}")
}

pub fn epoch_uri(epoch_id: &str) -> String {
    format!("time://epochs/{epoch_id}")
}

pub fn event_uri(event_id: &str) -> String {
    format!("time://events/{event_id}")
}

pub fn parse_zone(uri: &str) -> Option<&str> {
    parse_single(uri, "time://zones/")
}

pub fn parse_calendar(uri: &str) -> Option<(&str, u64)> {
    let value = uri.strip_prefix("time://calendars/")?;
    let (id, version) = value.split_once("/versions/")?;
    Some((id, version.parse().ok()?))
}

pub fn parse_epoch(uri: &str) -> Option<&str> {
    parse_single(uri, "time://epochs/")
}

pub fn parse_event(uri: &str) -> Option<&str> {
    parse_single(uri, "time://events/")
}

fn parse_single<'a>(uri: &'a str, prefix: &str) -> Option<&'a str> {
    let value = uri.strip_prefix(prefix)?;
    (!value.is_empty() && !value.contains('?') && !value.contains('#')).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_resource_uris_round_trip() {
        assert_eq!(
            parse_zone(&zone_uri("America/New_York")),
            Some("America/New_York")
        );
        assert_eq!(
            parse_calendar(&calendar_uri("calendar-abc", 4)),
            Some(("calendar-abc", 4))
        );
    }
}
