DEFINE FUNCTION fn::kernel::identity::service_principal_available_v1($principal: record<principal>, $client: string) -> bool {
    LET $current = SELECT * FROM ONLY $principal;
    RETURN $current = NONE AND array::len(SELECT id FROM oauth_client WHERE client_id = $client LIMIT 1) = 0;
} PERMISSIONS FULL;
