DEFINE FUNCTION fn::kernel::identity::create_service_principal_v1($tenant: record<tenant>, $principal: record<principal>, $issuer: string, $subject: string, $name: string) -> object {
    RETURN CREATE ONLY $principal SET tenant = $tenant, kind = 'service', issuer = $issuer,
        subject = $subject, display_name = $name, claims_hash = '', enabled = true;
} PERMISSIONS FULL;
