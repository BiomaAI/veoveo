DEFINE FUNCTION fn::kernel::identity::principal_summaries_v1($tenant: record<tenant>, $principals: array<record<principal>>) -> array<object> {
    IF array::len($principals) > 256 { THROW 'identity_principal_limit'; };
    RETURN SELECT id, display_name FROM principal
        WHERE tenant = $tenant AND id IN $principals LIMIT 256;
} PERMISSIONS FULL;
