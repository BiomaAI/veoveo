LET $active = SELECT * FROM ONLY gateway_control_active:current;
SELECT document, profile_policy_version FROM gateway_control_object WHERE revision = $active.revision AND object_kind = 'profile' AND object_id = $profile;
