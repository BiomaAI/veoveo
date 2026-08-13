//! Final MCP protocol constants shared by every first-party endpoint.

use std::borrow::Cow;

use rmcp::model::{
    CacheScope, ProtocolVersion, ReadResourceResponse, ReadResourceResult, RequestMetaObject,
};

/// Freshness window for authorization-filtered catalog responses.
pub const PRIVATE_CATALOG_TTL_MS: u64 = 5_000;

/// Freshness window for authorization-filtered resource reads.
pub const PRIVATE_RESOURCE_TTL_MS: u64 = 1_000;

/// Freshness window for immutable, authority-independent contract content.
pub const PUBLIC_IMMUTABLE_TTL_MS: u64 = 300_000;

/// Maximum W3C `tracestate` size accepted at the MCP boundary.
pub const MAX_TRACESTATE_BYTES: usize = 512;

/// Maximum W3C Baggage size accepted at the MCP boundary.
pub const MAX_BAGGAGE_BYTES: usize = 8 * 1024;

/// The sole wire revision accepted and advertised by Veoveo endpoints.
pub fn final_protocol_versions() -> Cow<'static, [ProtocolVersion]> {
    Cow::Borrowed(&[ProtocolVersion::V_2026_07_28])
}

/// Applies the final private-resource cache policy and converts to an MRTR-aware response.
///
/// Multi-round retries are immediately stale because responses carrying request state or
/// client input must never enter a reusable response cache.
pub fn private_resource_response(
    mut result: ReadResourceResult,
    cacheable: bool,
) -> ReadResourceResponse {
    result.ttl_ms = Some(if cacheable {
        PRIVATE_RESOURCE_TTL_MS
    } else {
        0
    });
    result.cache_scope = Some(CacheScope::Private);
    result.into()
}

/// Returns the 32-byte W3C trace identifier from a strict version-00
/// `traceparent`. Invalid or all-zero identifiers are deliberately ignored.
pub fn trace_id_from_traceparent(value: &str) -> Option<&str> {
    let bytes = value.as_bytes();
    if bytes.len() != 55
        || bytes[2] != b'-'
        || bytes[35] != b'-'
        || bytes[52] != b'-'
        || &value[0..2] != "00"
    {
        return None;
    }
    let trace_id = &value[3..35];
    let parent_id = &value[36..52];
    let flags = &value[53..55];
    let is_lower_hex = |text: &str| {
        text.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    };
    if !is_lower_hex(trace_id)
        || !is_lower_hex(parent_id)
        || !is_lower_hex(flags)
        || trace_id.bytes().all(|byte| byte == b'0')
        || parent_id.bytes().all(|byte| byte == b'0')
    {
        return None;
    }
    Some(trace_id)
}

/// Removes malformed or oversized observability fields while preserving the
/// remaining final-profile request metadata. Trace fields never contribute to
/// authorization.
pub fn sanitized_request_meta(meta: &RequestMetaObject) -> RequestMetaObject {
    let mut sanitized = meta.clone();
    if sanitized
        .get_traceparent()
        .is_some_and(|value| trace_id_from_traceparent(value).is_none())
    {
        sanitized.remove("traceparent");
    }
    if sanitized
        .get_tracestate()
        .is_some_and(|value| !valid_trace_list(value, MAX_TRACESTATE_BYTES, 32))
    {
        sanitized.remove("tracestate");
    }
    if sanitized
        .get_baggage()
        .is_some_and(|value| !valid_trace_list(value, MAX_BAGGAGE_BYTES, 180))
    {
        sanitized.remove("baggage");
    }
    sanitized
}

fn valid_trace_list(value: &str, maximum_bytes: usize, maximum_members: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum_bytes
        && value.split(',').count() <= maximum_members
        && value.split(',').all(|member| !member.trim().is_empty())
        && value
            .bytes()
            .all(|byte| byte == b'\t' || (b' '..=b'~').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn final_profile_advertises_one_protocol_revision() {
        assert_eq!(
            final_protocol_versions().as_ref(),
            &[ProtocolVersion::V_2026_07_28]
        );
    }

    #[test]
    fn multi_round_resource_retry_is_immediately_stale() {
        let response = private_resource_response(ReadResourceResult::new(Vec::new()), false);
        let ReadResourceResponse::Complete(result) = response else {
            panic!("resource response must be complete");
        };
        assert_eq!(result.ttl_ms, Some(0));
        assert_eq!(result.cache_scope, Some(CacheScope::Private));
    }

    #[test]
    fn strict_traceparent_yields_only_its_trace_id() {
        let parent = "00-0af7651916cd43dd8448eb211c80319c-00f067aa0ba902b7-01";
        assert_eq!(
            trace_id_from_traceparent(parent),
            Some("0af7651916cd43dd8448eb211c80319c")
        );
        assert_eq!(
            trace_id_from_traceparent("00-00000000000000000000000000000000-00f067aa0ba902b7-01"),
            None
        );
    }

    #[test]
    fn malformed_trace_fields_are_removed_without_touching_client_context() {
        let mut meta = RequestMetaObject::with_client_context(
            ProtocolVersion::V_2026_07_28,
            rmcp::model::Implementation::new("test", "1"),
            rmcp::model::ClientCapabilities::default(),
        );
        meta.set_traceparent("not-a-traceparent");
        meta.set_tracestate("\r\ninvalid");
        meta.set_baggage("x".repeat(MAX_BAGGAGE_BYTES + 1));

        let sanitized = sanitized_request_meta(&meta);

        assert!(sanitized.get_traceparent().is_none());
        assert!(sanitized.get_tracestate().is_none());
        assert!(sanitized.get_baggage().is_none());
        assert_eq!(
            sanitized.protocol_version(),
            Some(ProtocolVersion::V_2026_07_28)
        );
    }
}
