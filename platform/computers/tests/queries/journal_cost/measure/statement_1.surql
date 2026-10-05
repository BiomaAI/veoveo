RETURN string::len((SELECT VALUE payload.sealed.ciphertext FROM ONLY $record));
