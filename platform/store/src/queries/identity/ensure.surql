-- Identity discovery may create records; metadata projection cannot overwrite
-- installation-owned security fields from an earlier read.
BEGIN TRANSACTION;
LET $current_enterprise = SELECT * FROM ONLY $enterprise;
IF $current_enterprise = NONE {
    CREATE ONLY $enterprise CONTENT $enterprise_content RETURN NONE;
};
LET $current_tenant = SELECT * FROM ONLY $tenant;
IF $current_tenant = NONE {
    CREATE ONLY $tenant CONTENT $tenant_content RETURN NONE;
} ELSE IF $current_tenant.enterprise != $enterprise OR $current_tenant.slug != $tenant_content.slug {
    THROW 'identity_tenant_conflict';
};
LET $current_principal = SELECT * FROM ONLY $principal;
IF $current_principal = NONE {
    CREATE ONLY $principal CONTENT $principal_content RETURN NONE;
} ELSE {
    IF $current_principal.tenant != $tenant
        OR $current_principal.kind != $principal_content.kind
        OR $current_principal.issuer != $principal_content.issuer
        OR $current_principal.subject != $principal_content.subject {
        THROW 'identity_principal_conflict';
    };
    LET $name = IF $display_name != NONE AND $display_name != NULL {
        $display_name
    } ELSE IF $current_principal.display_name = $principal_key {
        $fallback_display_name
    } ELSE {
        $current_principal.display_name
    };
    IF $current_principal.display_name != $name {
        UPDATE ONLY $principal SET display_name = $name, updated_at = time::now() RETURN NONE;
    };
};
COMMIT TRANSACTION;
