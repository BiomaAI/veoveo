IF type::is_object(selected) {
    IF selected.status = 'succeeded' AND type::is_object(selected.result) {
        IF (selected.result.isError ?? false) = false AND type::is_object(selected.result.structuredContent) {
            selected.result.structuredContent.result_uri != NONE
        } ELSE { false }
    } ELSE { false }
} ELSE { false }
