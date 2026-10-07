use super::*;
pub(super) async fn cmd_fake_openai_llm(port: u16, ready_file: Option<PathBuf>) -> Result<()> {
    let router = AxumRouter::new().route("/v1/chat/completions", axum_post(fake_llm_completion));
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    if let Some(path) = ready_file {
        std::fs::write(path, b"ready\n")?;
    }
    axum::serve(listener, router).await?;
    Ok(())
}

async fn fake_llm_completion(AxumJson(request): AxumJson<Value>) -> AxumJson<Value> {
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let text_of = |message: &Value| match message.get("content") {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let has_task_update = messages
        .iter()
        .any(|message| text_of(message).contains("Authoritative background-task continuation:"));
    let has_heartbeat = messages
        .iter()
        .any(|message| text_of(message).contains("Scheduled heartbeat"));
    let has_episode_count_ask = messages
        .iter()
        .any(|message| text_of(message).contains("Count your episodes"));
    let assistant_turns = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("assistant"))
        .count();

    let has_pilot_context = messages
        .iter()
        .any(|message| text_of(message).contains("Add target alpha"));
    // The stored operator request remains in SQL-backed context on later wakes.
    // Only the current wake can ask the script to dispatch a new mission.
    let has_pilot_ask = messages.iter().any(|message| {
        let text = text_of(message);
        text.split_once("## Wake\n\n")
            .and_then(|(_, body)| body.split("\n\n## ").next())
            .is_some_and(|wake| wake.contains("Add target alpha"))
    });
    let has_optimization_update = messages.iter().any(|message| {
        let text = text_of(message);
        text.contains("Authoritative background-task continuation:")
            && text.contains("optimization__solve_milp")
    });

    if has_heartbeat && !has_task_update && !has_episode_count_ask && !has_pilot_ask {
        return AxumJson(fake_llm_stop_response(&request, "IDLE."));
    }
    // The pilot memory script records an intent and dispatches a selection MILP.
    // The completed task adds a resource bookmark using the current pilot schema.
    if has_optimization_update {
        let choice = match assistant_turns {
            0 => fake_llm_tool_call_choice(
                "memory_write",
                json!({
                    "op": "insert",
                    "table": "resource_bookmarks",
                    "row": {
                        "name": "alpha-selection", "uri": "optimization://solutions"
                    }
                }),
            ),
            _ => return AxumJson(fake_llm_stop_response(&request, "MISSION PLANNED.")),
        };
        return AxumJson(fake_llm_response(&request, choice));
    }
    if has_pilot_context && !has_pilot_ask {
        return AxumJson(fake_llm_stop_response(&request, "AWAITING OPTIMIZATION."));
    }
    if has_pilot_ask && !has_task_update {
        let choice = match assistant_turns {
            0 => fake_llm_tool_call_choice(
                "memory_write",
                json!({
                    "op": "insert",
                    "table": "mission_intents",
                    "row": {
                        "mission_key": "alpha",
                        "operator_request": "Add target alpha at 37.7749,-122.4194 and plan the visit.",
                        "state": "received"
                    }
                }),
            ),
            1 => fake_llm_tool_call_choice(
                "frames__convert_frame",
                json!({
                    "target": {"kind": "ecef_wgs84"},
                    "points": [{
                        "kind": "wgs84", "latitude_degrees": 37.7749,
                        "longitude_degrees": -122.4194, "ellipsoid_height_m": 0.0
                    }]
                }),
            ),
            2 => fake_llm_tool_call_choice(
                "optimization__solve_milp",
                json!({
                    "problem": {
                        "source": "inline",
                        "problem": {
                            "version": "veoveo.ai/milp-problem/v2",
                            "variables": [{
                                "variableId": "visit-alpha",
                                "kind": "integer",
                                "bounds": {"lower": 0.0, "upper": 1.0}
                            }],
                            "objective": {
                                "direction": "maximize",
                                "linearTerms": [{
                                    "variableId": "visit-alpha",
                                    "coefficient": 1.0
                                }],
                                "offset": 0.0
                            },
                            "constraints": [{
                                "constraintId": "visit-required",
                                "terms": [{
                                    "variableId": "visit-alpha",
                                    "coefficient": 1.0
                                }],
                                "bounds": {"lower": 1.0, "upper": 1.0}
                            }]
                        },
                    },
                    "policy": {
                        "profileUri": "optimization://profile/balanced"
                    },
                    "output": {
                        "retainWarmStart": false,
                        "retainIncumbents": true
                    }
                }),
            ),
            _ => {
                return AxumJson(fake_llm_stop_response(&request, "AWAITING OPTIMIZATION."));
            }
        };
        return AxumJson(fake_llm_response(&request, choice));
    }
    if has_episode_count_ask && !has_task_update {
        let choice = if assistant_turns == 0 {
            fake_llm_tool_call_choice(
                "memory_query",
                json!({ "sql": "SELECT COUNT(*) AS episodes FROM agent_memory.episode_log" }),
            )
        } else {
            return AxumJson(fake_llm_stop_response(&request, "EPISODES COUNTED."));
        };
        return AxumJson(fake_llm_response(&request, choice));
    }

    let choice = match (has_task_update, assistant_turns) {
        (true, 0) => fake_llm_tool_call_choice(
            "memory_write",
            json!({
                "op": "insert",
                "table": "notes",
                "row": { "note": "media task completed", "source": "agent-kernel-smoke" }
            }),
        ),
        (true, 1) => fake_llm_tool_call_choice(
            "timeline_query",
            json!({ "entities": "/agent/**", "maxRows": 10 }),
        ),
        (true, _) => json!({
            "index": 0,
            "message": {
                "role": "assistant",
                "content": "OBJECTIVE COMPLETE: the artifact is recorded in memory."
            },
            "finish_reason": "stop"
        }),
        (false, 0) => fake_llm_tool_call_choice(
            "memory_query",
            json!({ "sql": "SELECT COUNT(*) AS episodes FROM agent_memory.episode_log" }),
        ),
        (false, 1) => fake_llm_tool_call_choice(
            "media__run",
            json!({
                "model": "fake/image",
                "input": { "prompt": "agent kernel smoke" }
            }),
        ),
        (false, _) => json!({
            "index": 0,
            "message": { "role": "assistant", "content": "WAITING FOR BACKGROUND TASKS" },
            "finish_reason": "stop"
        }),
    };

    AxumJson(fake_llm_response(&request, choice))
}

fn fake_llm_response(request: &Value, choice: Value) -> Value {
    json!({
        "id": "chatcmpl-fake",
        "object": "chat.completion",
        "created": 0,
        "model": request.get("model").cloned().unwrap_or_else(|| json!("fake")),
        "choices": [choice],
        "usage": { "prompt_tokens": 20, "completion_tokens": 10, "total_tokens": 30 }
    })
}

fn fake_llm_stop_response(request: &Value, content: &str) -> Value {
    fake_llm_response(
        request,
        json!({
            "index": 0,
            "message": { "role": "assistant", "content": content },
            "finish_reason": "stop"
        }),
    )
}

fn fake_llm_tool_call_choice(tool_name: &str, arguments: Value) -> Value {
    json!({
        "index": 0,
        "message": {
            "role": "assistant",
            "content": null,
            "tool_calls": [{
                "id": format!("call_{tool_name}"),
                "type": "function",
                "function": { "name": tool_name, "arguments": arguments.to_string() }
            }]
        },
        "finish_reason": "tool_calls"
    })
}
