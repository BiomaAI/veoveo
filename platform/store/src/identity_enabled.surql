SELECT VALUE true FROM ONLY $principal
WHERE enabled = true
    AND tenant = $tenant
    AND issuer = $issuer
    AND subject = $subject
    AND kind = $kind
    AND tenant.enabled = true
    AND tenant.slug = $tenant_key
    AND tenant.enterprise = $enterprise
    AND tenant.enterprise.enabled = true
TIMEOUT 10s;
