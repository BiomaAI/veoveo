DEFINE FUNCTION fn::kernel::gateway::refresh_family_current_v1($family: record<gateway_refresh_family>, $at: datetime) -> option<object> {
    LET $current = SELECT * FROM ONLY $family;
    RETURN IF type::is_object($current) THEN
      IF $current.revoked_at = NONE AND $current.expires_at > $at THEN { expires_at: $current.expires_at }
      ELSE NONE END
    ELSE NONE END;
} PERMISSIONS FULL;
