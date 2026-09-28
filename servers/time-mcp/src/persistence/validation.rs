use super::*;
use url::Url;
use uuid::Uuid;

const MAX_CANONICAL_JSON_BYTES: usize = 2 * 1024 * 1024;

pub(super) fn validate_source(draft: &TimeSourceDraft) -> Result<(), PersistenceError> {
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("name", &draft.name, 256)?;
    validate_https_url("source_url", &draft.source_url)?;
    validate_text("expected_content_type", &draft.expected_content_type, 128)?;
    validate_json(&draft.canonical_json)
}

pub(super) fn validate_release(draft: &TimeAuthorityReleaseDraft) -> Result<(), PersistenceError> {
    validate_key("release_key", &draft.release_key, "time-release-")?;
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("version_label", &draft.version_label, 256)?;
    validate_https_url("source_url", &draft.source_url)?;
    validate_absolute_path("artifact_path", &draft.artifact_path)?;
    if draft.validated_at < draft.retrieved_at {
        return Err(invalid("validated_at", "must not precede retrieval"));
    }
    validate_json(&draft.canonical_json)
}

pub(super) fn validate_acquisition(draft: &TimeAcquisitionDraft) -> Result<(), PersistenceError> {
    validate_key(
        "acquisition_key",
        &draft.acquisition_key,
        "time-acquisition-",
    )?;
    validate_key("source_key", &draft.source_key, "time-source-")?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    validate_text("phase", &draft.phase, 128)?;
    if let Some(release) = &draft.staged_release_key {
        validate_key("staged_release_key", release, "time-release-")?;
    }
    validate_json(&draft.canonical_json)
}

pub(super) fn validate_event(draft: &TimeTemporalEventDraft) -> Result<(), PersistenceError> {
    validate_key("event_key", &draft.event_key, "event-")?;
    validate_text("name", &draft.name, 256)?;
    validate_nanosecond(draft.due_nanosecond)?;
    validate_text("idempotency_key", &draft.idempotency_key, 256)?;
    validate_json(&draft.canonical_json)
}

pub(crate) fn validate_key(
    field: &'static str,
    value: impl AsRef<str>,
    prefix: &'static str,
) -> Result<(), PersistenceError> {
    let raw = value
        .as_ref()
        .strip_prefix(prefix)
        .ok_or_else(|| invalid(field, "must use the canonical prefix followed by a UUIDv7"))?;
    let uuid = Uuid::parse_str(raw)
        .map_err(|_| invalid(field, "must use the canonical prefix followed by a UUIDv7"))?;
    if uuid.get_version_num() != 7 {
        return Err(invalid(
            field,
            "must use the canonical prefix followed by a UUIDv7",
        ));
    }
    Ok(())
}

pub(super) fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
) -> Result<(), PersistenceError> {
    if value.is_empty() || value.len() > max || value.chars().any(char::is_control) {
        return Err(invalid(
            field,
            "must be non-empty, bounded, and contain no control characters",
        ));
    }
    Ok(())
}

pub(super) fn validate_positive(field: &'static str, value: i64) -> Result<(), PersistenceError> {
    if value < 1 {
        return Err(invalid(field, "must be positive"));
    }
    Ok(())
}

pub(super) fn validate_nanosecond(value: i64) -> Result<(), PersistenceError> {
    if !(0..1_000_000_000).contains(&value) {
        return Err(invalid("nanosecond", "must be in 0..1000000000"));
    }
    Ok(())
}

pub(super) fn validate_zone_id(value: &str) -> Result<(), PersistenceError> {
    if value.is_empty()
        || value.len() > 128
        || value.starts_with('/')
        || value.contains("..")
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'_' | b'-' | b'+'))
    {
        return Err(invalid("zone_id", "must be a bounded IANA zone identifier"));
    }
    Ok(())
}

pub(super) fn validate_https_url(field: &'static str, value: &str) -> Result<(), PersistenceError> {
    let url = Url::parse(value).map_err(|_| invalid(field, "must be an absolute HTTPS URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid(
            field,
            "must be an absolute HTTPS URL without credentials or a fragment",
        ));
    }
    Ok(())
}

pub(super) fn validate_absolute_path(
    field: &'static str,
    value: &str,
) -> Result<(), PersistenceError> {
    if !value.starts_with('/') || value.contains("/../") || value.chars().any(char::is_control) {
        return Err(invalid(field, "must be a confined absolute path"));
    }
    Ok(())
}

pub(super) fn validate_json(value: &str) -> Result<(), PersistenceError> {
    if value.len() > MAX_CANONICAL_JSON_BYTES
        || serde_json::from_str::<serde_json::Value>(value).is_err()
    {
        return Err(invalid("canonical_json", "must be valid bounded JSON"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_time_keys_require_uuid_v7() {
        assert!(validate_key("event_key", format!("event-{}", Uuid::now_v7()), "event-").is_ok());
        assert!(validate_key("event_key", format!("event-{}", Uuid::new_v4()), "event-").is_err());
    }

    #[test]
    fn named_bootstrap_references_keep_the_public_profile_without_becoming_stored_keys() {
        let bootstrap = AuthorityReleaseId::new("time-release-tzdb-bootstrap").unwrap();
        assert!(validate_key("release_key", &bootstrap, "time-release-").is_err());
        assert_eq!(
            serde_json::from_str::<AuthorityReleaseId>(&serde_json::to_string(&bootstrap).unwrap())
                .unwrap(),
            bootstrap
        );
        let stored = AuthorityReleaseId::new(format!("time-release-{}", Uuid::now_v7())).unwrap();
        assert!(validate_key("release_key", &stored, "time-release-").is_ok());
    }

    #[test]
    fn authority_sources_require_safe_https_urls() {
        assert!(
            validate_https_url(
                "source_url",
                "https://data.iana.org/time-zones/tzdata-latest.tar.gz"
            )
            .is_ok()
        );
        assert!(validate_https_url("source_url", "http://example.test/tzdb").is_err());
        assert!(validate_https_url("source_url", "https://user@example.test/tzdb").is_err());
    }
}
