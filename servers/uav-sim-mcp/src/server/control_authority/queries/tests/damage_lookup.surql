IF $remove { DELETE ONLY $catalog; } ELSE { UPDATE ONLY $catalog SET identity.expected_revision += 1; };
