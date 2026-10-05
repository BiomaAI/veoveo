RETURN { exists: fn::kernel::tasks::exists_v1($task), matches: fn::kernel::tasks::input_matches_v1($task, $expected_input) };
