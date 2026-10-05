SELECT * FROM wake WHERE agent = $agent AND tenant = $tenant AND kind = 'operator_message' ORDER BY created_at DESC LIMIT 256;
