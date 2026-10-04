IF type::is_object(selected) {
    IF type::is_object(selected.input) {
        selected.input.kind = selected.task_type
    } ELSE { false }
} ELSE { false }
