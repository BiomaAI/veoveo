SELECT * FROM ONLY $grant WHERE expires_at > time::now() AND grant_class IN $classes;
