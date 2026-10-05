DEFINE FUNCTION fn::kernel::gateway::control_revision_current_v1($revision: record<gateway_control_revision>, $revision_id: string, $sha256: string) -> bool {
    LET $head = SELECT * FROM ONLY gateway_control_active:current;
    LET $current = SELECT * FROM ONLY $revision;
    RETURN IF type::is_object($head) THEN
      IF type::is_object($current) THEN
        $head.revision = $revision AND $head.revision_id = $revision_id AND $current.sha256 = $sha256
      ELSE false END
    ELSE false END;
} PERMISSIONS FULL;
