DEFINE FUNCTION fn::kernel::identity::search_enabled_users_v1($tenant: record<tenant>, $search: string) -> array<object> {
    RETURN SELECT id, display_name FROM principal
        WHERE tenant = $tenant AND enabled = true AND kind = 'user'
        AND string::contains(string::lowercase(display_name), $search)
        ORDER BY display_name ASC LIMIT 20;
} PERMISSIONS FULL;
