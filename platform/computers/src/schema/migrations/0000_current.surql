-- Current owner schema; schema definitions only.

DEFINE TABLE computer TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE computer_automation_grant TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE computer_automation_guard TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_automation_policy TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE computer_automation_request TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_capacity TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_cli_connection TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_cli_grant TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE computer_cli_pairing TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_cli_pairing_guard TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_execution TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE computer_execution_payload TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_execution_request TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_execution_slot TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_file_transfer TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE computer_file_transfer_payload TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_file_transfer_request TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_maintenance TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE computer_maintenance_policy TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_maintenance_request TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_maintenance_resume TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_operation TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE computer_operation_request TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_request TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_session_grant TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE computer_session_grant_guard TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE computer_session_grant_policy TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE computer_usage TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FIELD OVERWRITE active_operation ON computer TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON computer TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_context ON computer TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_context.authority ON computer TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON computer TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE phase ON computer TYPE 'reserved' | 'provisioning' | 'starting' | 'ready' | 'stopping' | 'stopped' | 'error' | 'recovery_required' DEFAULT 'reserved' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE process_id ON computer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_resource_id ON computer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE replacement_instance_id ON computer TYPE none | uuid ASSERT $value = NONE OR $value != $this.computer_id AND $value != u'00000000-0000-0000-0000-000000000000' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE template_fingerprint ON computer TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE template_id ON computer TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant_key ON computer TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON computer TYPE datetime DEFAULT time::now() PERMISSIONS FULL;





DEFINE FIELD OVERWRITE authority ON computer_automation_grant TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_automation_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution_limits ON computer_automation_grant TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON computer_automation_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_id ON computer_automation_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grantee ON computer_automation_grant TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grantee_issuer ON computer_automation_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grantee_kind ON computer_automation_grant TYPE 'user' | 'service' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grantee_subject ON computer_automation_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON computer_automation_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON computer_automation_grant TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE oauth_client_id ON computer_automation_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON computer_automation_grant TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE permissions ON computer_automation_grant TYPE array<'read' | 'execute' | 'start' | 'stop'> ASSERT array::len($value) > 0 AND array::len($value) <= 4 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE permissions.* ON computer_automation_grant TYPE 'read' | 'execute' | 'start' | 'stop' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal_id ON computer_automation_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer_automation_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON computer_automation_grant TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON computer_automation_grant TYPE none | datetime PERMISSIONS FULL;




DEFINE FIELD OVERWRITE revision ON computer_automation_guard TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admissions ON computer_automation_policy TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON computer_automation_policy TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE max_grants ON computer_automation_policy TYPE int ASSERT $value >= 0 AND $value <= 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE maximum_execution_seconds ON computer_automation_policy TYPE int ASSERT $value > 0 AND $value <= 7200 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE maximum_lifetime_seconds ON computer_automation_policy TYPE int ASSERT $value > 0 AND $value <= 86400 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE maximum_output_bytes ON computer_automation_policy TYPE int ASSERT $value > 0 AND $value <= 67108864 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON computer_automation_request TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant ON computer_automation_request TYPE record<computer_automation_grant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admissions ON computer_capacity TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON computer_capacity TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE per_owner ON computer_capacity TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE per_tenant ON computer_capacity TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider ON computer_capacity TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE closed_at ON computer_cli_connection TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE connection_id ON computer_cli_connection TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON computer_cli_connection TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_id ON computer_cli_connection TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON computer_cli_connection TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE process_id ON computer_cli_connection TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_resource_id ON computer_cli_connection TYPE string PERMISSIONS FULL;


DEFINE FIELD OVERWRITE authority ON computer_cli_grant TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_cli_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE connections_revision ON computer_cli_grant TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE credential_hash ON computer_cli_grant TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON computer_cli_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE family ON computer_cli_grant TYPE record<gateway_refresh_family> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_id ON computer_cli_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idle_expires_at ON computer_cli_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON computer_cli_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_activity_at ON computer_cli_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON computer_cli_grant TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON computer_cli_grant TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer_cli_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON computer_cli_grant TYPE none | datetime PERMISSIONS FULL;



DEFINE FIELD OVERWRITE binding_hash ON computer_cli_pairing TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE callback_port ON computer_cli_pairing TYPE int ASSERT $value >= 1024 AND $value <= 65535 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE code_hash ON computer_cli_pairing TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_cli_pairing TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE consumed_at ON computer_cli_pairing TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON computer_cli_pairing TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE family ON computer_cli_pairing TYPE record<gateway_refresh_family> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON computer_cli_pairing TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON computer_cli_pairing TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON computer_cli_pairing TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE pairing_id ON computer_cli_pairing TYPE uuid PERMISSIONS FULL;



DEFINE FIELD OVERWRITE revision ON computer_cli_pairing_guard TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_key ON computer_execution TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON computer_execution TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE binding ON computer_execution TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_execution TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_deadline ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_dispatch_id ON computer_execution TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_id ON computer_execution TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_reads ON computer_execution TYPE int DEFAULT 0 ASSERT $value >= 0 AND $value <= 8 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_started_at ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON computer_execution TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_authority ON computer_execution TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_id ON computer_execution TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatched_at ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE effective_limits ON computer_execution TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution_deadline ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution_id ON computer_execution TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE interruption ON computer_execution TYPE none | 'cancelled' | 'deadline' | 'authority_lost' | 'execution_unknown' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_containment_read_id ON computer_execution TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE next_containment_read ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_access ON computer_execution TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE payload ON computer_execution TYPE record<computer_execution_payload> READONLY ASSERT record::id($value) = $this.execution_id PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer_execution TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE refusal ON computer_execution TYPE none | 'cancelled_before_dispatch' | 'authority_denied' | 'run_changed' | 'preparation_expired' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result ON computer_execution TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE settled_at ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE stage ON computer_execution TYPE 'queued' | 'dispatched' | 'containing' | 'recovery_required' | 'failed' | 'cancelled' | 'completed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON computer_execution TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_tenant ON computer_execution TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_projected_at ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE terminated_at ON computer_execution TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE termination_evidence ON computer_execution TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON computer_execution TYPE datetime PERMISSIONS FULL;




DEFINE FIELD OVERWRITE journal ON computer_execution_payload TYPE record<computer_execution> READONLY ASSERT record::id($value) = record::id($this.id) REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sealed ON computer_execution_payload TYPE { ciphertext: string, fingerprint: string, key_id: string, nonce: string, version: int } READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution ON computer_execution_request TYPE record<computer_execution> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_execution_slot TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution ON computer_execution_slot TYPE record<computer_execution | computer_file_transfer> PERMISSIONS FULL;


DEFINE FIELD OVERWRITE actor_key ON computer_file_transfer TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE artifact_access ON computer_file_transfer TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON computer_file_transfer TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE binding ON computer_file_transfer TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_file_transfer TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_deadline ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_dispatch_id ON computer_file_transfer TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_id ON computer_file_transfer TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_reads ON computer_file_transfer TYPE int DEFAULT 0 ASSERT $value >= 0 AND $value <= 8 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE containment_started_at ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON computer_file_transfer TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_authority ON computer_file_transfer TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_id ON computer_file_transfer TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatched_at ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE effective_limits ON computer_file_transfer TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution_deadline ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE interruption ON computer_file_transfer TYPE none | 'cancelled' | 'deadline' | 'authority_lost' | 'execution_unknown' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_containment_read_id ON computer_file_transfer TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE next_containment_read ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON computer_file_transfer TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE payload ON computer_file_transfer TYPE record<computer_file_transfer_payload> READONLY ASSERT record::id($value) = $this.transfer_id PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer_file_transfer TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE refusal ON computer_file_transfer TYPE none | 'cancelled_before_dispatch' | 'authority_denied' | 'run_changed' | 'preparation_expired' | 'artifact_unavailable' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE rejection ON computer_file_transfer TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result ON computer_file_transfer TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE settled_at ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE stage ON computer_file_transfer TYPE 'queued' | 'dispatched' | 'containing' | 'recovery_required' | 'completed' | 'failed' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON computer_file_transfer TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_tenant ON computer_file_transfer TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_projected_at ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE terminated_at ON computer_file_transfer TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE termination_evidence ON computer_file_transfer TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE transfer_id ON computer_file_transfer TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON computer_file_transfer TYPE datetime PERMISSIONS FULL;





DEFINE FIELD OVERWRITE journal ON computer_file_transfer_payload TYPE record<computer_file_transfer> READONLY ASSERT record::id($value) = record::id($this.id) REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sealed ON computer_file_transfer_payload TYPE { ciphertext: string, fingerprint: string, key_id: string, nonce: string, version: int } READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE transfer ON computer_file_transfer_request TYPE record<computer_file_transfer> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_context ON computer_maintenance TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_maintenance TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON computer_maintenance TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution_authority ON computer_maintenance TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation_id ON computer_maintenance TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON computer_maintenance TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE progress ON computer_maintenance TYPE object FLEXIBLE DEFAULT { steps: [], recovery: NULL } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer_maintenance TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_id ON computer_maintenance TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source ON computer_maintenance TYPE object READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_kind ON computer_maintenance TYPE 'ready' | 'stopped' | 'initial_failure' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_instance_id ON computer_maintenance TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_template_fingerprint ON computer_maintenance TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_template_id ON computer_maintenance TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE stage ON computer_maintenance TYPE 'queued' | 'stopping' | 'capturing' | 'retiring' | 'transferring' | 'creating' | 'restoring' | 'adopting' | 'succeeded' | 'cancelled' | 'recovery_required' DEFAULT 'queued' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE target_instance_id ON computer_maintenance TYPE uuid ASSERT $value != u'00000000-0000-0000-0000-000000000000' AND $value != $this.computer_id AND $value != $this.source_instance_id PERMISSIONS FULL;

DEFINE FIELD OVERWRITE target_template_fingerprint ON computer_maintenance TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE target_template_id ON computer_maintenance TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON computer_maintenance TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_tenant ON computer_maintenance TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_projected_at ON computer_maintenance TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON computer_maintenance TYPE datetime DEFAULT time::now() PERMISSIONS FULL;




DEFINE FIELD OVERWRITE created_at ON computer_maintenance_policy TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE envelope ON computer_maintenance_policy TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation_id ON computer_maintenance_policy TYPE uuid PERMISSIONS FULL;


DEFINE FIELD OVERWRITE fingerprint ON computer_maintenance_request TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation_id ON computer_maintenance_request TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON computer_maintenance_resume TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON computer_maintenance_resume TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE decision ON computer_maintenance_resume TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON computer_maintenance_resume TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE input ON computer_maintenance_resume TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation_id ON computer_maintenance_resume TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE previous_progress ON computer_maintenance_resume TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_id ON computer_maintenance_resume TYPE uuid PERMISSIONS FULL;


DEFINE FIELD OVERWRITE action ON computer_operation TYPE 'create' | 'start' | 'stop' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor_context ON computer_operation TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE automation_grant_id ON computer_operation TYPE none | uuid ASSERT $value = NONE OR $value != u'00000000-0000-0000-0000-000000000000' AND $this.action != 'create' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE completion_code ON computer_operation TYPE none | 'authority_denied' | 'cancelled_before_dispatch' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON computer_operation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_authority ON computer_operation TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_id ON computer_operation TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatched_at ON computer_operation TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE execution_authority ON computer_operation TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_observation_id ON computer_operation TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE next_observation_at ON computer_operation TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE observation_deadline ON computer_operation TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE observation_reads ON computer_operation TYPE int DEFAULT 0 ASSERT $value >= 0 AND $value <= 8 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation_id ON computer_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_context ON computer_operation TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE previous_phase ON computer_operation TYPE 'reserved' | 'stopped' | 'ready' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE previous_process_id ON computer_operation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE previous_resource_id ON computer_operation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE replacement_instance_id ON computer_operation TYPE none | uuid ASSERT $value = NONE OR $value != $this.computer_id AND $value != u'00000000-0000-0000-0000-000000000000' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result_process_id ON computer_operation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result_resource_id ON computer_operation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE settled_at ON computer_operation TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE stage ON computer_operation TYPE 'queued' | 'dispatched' | 'recovery_required' | 'succeeded' | 'failed' | 'cancelled' DEFAULT 'queued' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON computer_operation TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_tenant ON computer_operation TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_projected_at ON computer_operation TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE template_fingerprint ON computer_operation TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON computer_operation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;





DEFINE FIELD OVERWRITE fingerprint ON computer_operation_request TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation_id ON computer_operation_request TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer ON computer_request TYPE record<computer> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON computer_request TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON computer_session_grant TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE computer_id ON computer_session_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE connection_id ON computer_session_grant TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON computer_session_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE family ON computer_session_grant TYPE record<gateway_refresh_family> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_id ON computer_session_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idle_expires_at ON computer_session_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE issued_at ON computer_session_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_activity_at ON computer_session_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_key ON computer_session_grant TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE process_id ON computer_session_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_instance_id ON computer_session_grant TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_resource_id ON computer_session_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON computer_session_grant TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ticket_expires_at ON computer_session_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ticket_hash ON computer_session_grant TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;



DEFINE FIELD OVERWRITE revision ON computer_session_grant_guard TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE absolute_seconds ON computer_session_grant_policy TYPE int ASSERT $value > 0 AND $value <= 86400 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admissions ON computer_session_grant_policy TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON computer_session_grant_policy TYPE string ASSERT string::len($value) = 64 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idle_seconds ON computer_session_grant_policy TYPE int ASSERT $value > 0 AND $value <= 86400 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE max_grants ON computer_session_grant_policy TYPE int ASSERT $value >= 0 AND $value <= 128 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retained ON computer_usage TYPE int ASSERT $value >= 0 PERMISSIONS FULL;
