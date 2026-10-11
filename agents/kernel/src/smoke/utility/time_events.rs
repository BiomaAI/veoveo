//! Deterministic qualification model; Time vocabulary stays outside the kernel.
use super::{fake_llm_response, fake_llm_stop_response, fake_llm_tool_call_choice};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use veoveo_time_mcp::contract::{
    EventCursor, TemporalEvent, TemporalEventState, TimeResource, TimeResourcePage,
};
use veoveo_types::ResourceAddress;

/// The provider request is open JSON. Domain resource bodies are decoded through
/// their owning Time contract before this fixture decides a semantic effect.
pub(super) fn completion(request: &Value) -> Result<Value> {
    let messages = request
        .get("messages")
        .and_then(Value::as_array)
        .context("missing model messages")?;
    let results: Vec<Value> = messages
        .iter()
        .filter(|m| m.get("role").and_then(Value::as_str) == Some("tool"))
        .map(|m| {
            let content = m.get("content").context("missing tool content")?;
            let text = match content {
                Value::String(text) => text.clone(),
                Value::Array(parts) => parts
                    .iter()
                    .filter_map(|p| p.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join(""),
                _ => anyhow::bail!("unsupported tool content"),
            };
            Ok(serde_json::from_str(&text)?)
        })
        .collect::<Result<_>>()?;
    let root = TimeResource::Events { cursor: None }.to_uri()?;
    let read = |uri: &str| {
        results
            .iter()
            .find(|result| result.get("uri").and_then(Value::as_str) == Some(uri))
    };
    let tool = |name: &str, args: Value| {
        let mut choice = fake_llm_tool_call_choice(name, args);
        choice["message"]["tool_calls"][0]["id"] = json!(format!("call_time_{}", results.len()));
        Ok(fake_llm_response(request, choice))
    };
    let Some(index) = read(root.as_str()) else {
        return tool("resource_read", json!({"uri":root}));
    };
    let page: TimeResourcePage<EventCursor> = decode_resource(index)?;
    ensure!(
        page.next_cursor.is_none(),
        "qualification event corpus exceeds one page"
    );
    let mut due = Vec::new();
    for entry in page.items {
        ensure!(
            matches!(
                TimeResource::parse(entry.uri.as_str())?,
                TimeResource::Event(_)
            ),
            "event index returned another resource family"
        );
        let Some(member) = read(entry.uri.as_str()) else {
            return tool("resource_read", json!({"uri":entry.uri}));
        };
        let event: TemporalEvent = decode_resource(member)?;
        ensure!(
            TimeResource::Event(event.event_id.clone()).to_uri()? == entry.uri,
            "event identity disagrees with index"
        );
        eprintln!(
            "time fixture: observed {} {}",
            serde_json::to_value(event.state)?
                .as_str()
                .context("state spelling")?,
            event.event_id
        );
        if event.state == TemporalEventState::Due {
            due.push(event);
        }
    }
    if due.is_empty() {
        eprintln!("time fixture: current events not due");
        return Ok(fake_llm_stop_response(request, "CURRENT EVENTS NOT DUE"));
    }
    // The current model turn contains only this episode's exchanges. Read the
    // retained domain ledger every episode rather than relying on model history.
    let Some(ledger) = results.iter().find(|r| r.get("rows").is_some()) else {
        return tool(
            "memory_query",
            json!({"sql":"SELECT event_id FROM time_effects ORDER BY event_id","maxRows":32}),
        );
    };
    let rows = ledger
        .get("rows")
        .and_then(Value::as_array)
        .context("invalid semantic ledger result")?;
    // A successful write in this episode adds its ID to the already-read rows.
    let written: Vec<_> = messages
        .iter()
        .filter_map(|m| m.get("tool_calls").and_then(Value::as_array))
        .flatten()
        .filter(|call| {
            call.pointer("/function/name").and_then(Value::as_str) == Some("memory_write")
        })
        .map(|call| {
            let id = call
                .get("id")
                .and_then(Value::as_str)
                .context("missing write call ID")?;
            let (_, result) = messages
                .iter()
                .filter(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
                .zip(&results)
                .find(|(message, _)| {
                    message.get("tool_call_id").and_then(Value::as_str) == Some(id)
                })
                .context("write has no observed result")?;
            ensure!(
                result.get("affectedRows").and_then(Value::as_u64) == Some(1),
                "semantic write did not acknowledge one row"
            );
            Ok(serde_json::from_str::<Value>(
                call.pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .context("missing write arguments")?,
            )?)
        })
        .collect::<Result<_>>()?;
    let confirmed_ids: Vec<_> = due.iter().map(|event| event.event_id.clone()).collect();
    for event in due {
        let id = serde_json::to_value(&event.event_id)?;
        if rows.iter().any(|row| row.get("event_id") == Some(&id))
            || written
                .iter()
                .any(|write| write.pointer("/row/event_id") == Some(&id))
        {
            continue;
        }
        return tool(
            "memory_write",
            json!({"op":"insert","table":"time_effects","row":{"event_id":event.event_id,"state":event.state,"record_version":event.record_version}}),
        );
    }
    for id in confirmed_ids {
        eprintln!("time fixture: due effect {id}");
    }
    eprintln!("time fixture: due effects ledgered");
    Ok(fake_llm_stop_response(request, "DUE EFFECTS LEDGERED"))
}
fn decode_resource<T: serde::de::DeserializeOwned>(result: &Value) -> Result<T> {
    ensure!(
        result.get("status").and_then(Value::as_str) == Some("complete"),
        "resource read incomplete"
    );
    let contents = result
        .get("contents")
        .and_then(Value::as_array)
        .context("missing resource contents")?;
    ensure!(contents.len() == 1, "expected one Time JSON body");
    serde_json::from_str(
        contents[0]
            .get("text")
            .and_then(Value::as_str)
            .context("missing resource text")?,
    )
    .context("invalid typed Time body")
}

#[cfg(test)]
mod tests {
    use super::*;
    use veoveo_time_mcp::contract::{
        AuthorityBinding, AuthorityReleaseId, TemporalEventId, TimeInstant, TimeVersion,
    };
    fn event(state: TemporalEventState) -> TemporalEvent {
        TemporalEvent {
            event_id: TemporalEventId::parse("event-fixture").unwrap(),
            name: "owner fixture".into(),
            due: TimeInstant::from_total_nanoseconds(
                0,
                0,
                AuthorityBinding::new(
                    AuthorityReleaseId::parse("time-release-tzdb").unwrap(),
                    AuthorityReleaseId::parse("time-release-leaps").unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
            state,
            record_version: TimeVersion::new(1).unwrap(),
        }
    }
    fn resource(uri: &str, body: Value) -> Value {
        json!({"role":"tool","tool_call_id":format!("read-{uri}"),"content":json!({"status":"complete","uri":uri,"contents":[{"uri":uri,"text":body.to_string()}],"responseBytes":100}).to_string()})
    }
    fn observed(state: TemporalEventState) -> Value {
        let event = event(state);
        let uri = TimeResource::Event(event.event_id.clone())
            .to_uri()
            .unwrap();
        json!({"messages":[resource("time://events",json!({"items":[{"uri":uri,"title":"owner fixture"}]})),resource(uri.as_str(),serde_json::to_value(event).unwrap())]})
    }
    #[test]
    fn creation_and_cancelled_invalidation_do_not_authorize_due_effect() {
        for state in [TemporalEventState::Scheduled, TemporalEventState::Cancelled] {
            let answer = completion(&observed(state)).unwrap();
            assert_eq!(
                answer
                    .pointer("/choices/0/message/content")
                    .and_then(Value::as_str),
                Some("CURRENT EVENTS NOT DUE")
            );
            assert!(answer.pointer("/choices/0/message/tool_calls").is_none());
        }
        let answer = completion(&observed(TemporalEventState::Due)).unwrap();
        assert_eq!(
            answer
                .pointer("/choices/0/message/tool_calls/0/function/name")
                .and_then(Value::as_str),
            Some("memory_query"),
            "Due requires retained semantic ledger before effect"
        );
    }
    #[test]
    fn duplicate_due_and_failed_write_never_claim_new_semantic_success() {
        let mut request = observed(TemporalEventState::Due);
        request["messages"].as_array_mut().unwrap().push(json!({"role":"tool","tool_call_id":"ledger","content":json!({"rows":[{"event_id":"event-fixture"}],"rowCount":1}).to_string()}));
        let answer = completion(&request).unwrap();
        assert!(
            answer.pointer("/choices/0/message/tool_calls").is_none(),
            "retained effect suppresses duplicate mutation"
        );
        request["messages"][2]["content"] = json!({"rows":[],"rowCount":0}).to_string().into();
        let answer = completion(&request).unwrap();
        let choice = answer.pointer("/choices/0/message").unwrap().clone();
        let id = choice.pointer("/tool_calls/0/id").unwrap().clone();
        request["messages"].as_array_mut().unwrap().push(choice);
        request["messages"].as_array_mut().unwrap().push(json!({"role":"tool","tool_call_id":id,"content":json!({"affectedRows":0}).to_string()}));
        assert!(
            completion(&request).is_err(),
            "failed write cannot settle a semantic effect from its intent"
        );
    }
    #[test]
    fn event_read_must_match_index_identity() {
        let mut request = observed(TemporalEventState::Due);
        // Preserve the actual member URI from the owner builder.
        let original = TimeResource::Event(TemporalEventId::parse("event-fixture").unwrap())
            .to_uri()
            .unwrap();
        request["messages"][1] = resource(
            original.as_str(),
            serde_json::to_value({
                let mut v = event(TemporalEventState::Due);
                v.event_id = TemporalEventId::parse("event-other").unwrap();
                v
            })
            .unwrap(),
        );
        assert!(completion(&request).is_err());
    }
}
