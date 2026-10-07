IF $chunk_target {
    UPDATE type::table($chunk_table)
        SET observation = $observation
        WHERE uri = $uri
        RETURN NONE;
} ELSE {
    UPDATE ONLY $member SET observation = $observation RETURN NONE;
};
