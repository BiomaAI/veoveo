//! Result builders for domain methods.
//!
//! Hosted servers build the same few result shapes. These helpers build them
//! once, and [`product_result`] enforces contract rule C02: a tool result that
//! creates an addressable product carries one top-level `resultUri` in its
//! structured content and one resource link with the same URI.

use rmcp::{
    ErrorData,
    model::{
        CallToolResult, CompleteResult, CompletionInfo, ContentBlock, ReadResourceResult, Resource,
        ResourceContents,
    },
};
use serde::Serialize;

fn serialize<T: Serialize + ?Sized>(value: &T) -> Result<serde_json::Value, ErrorData> {
    serde_json::to_value(value)
        .map_err(|_| ErrorData::internal_error("result serialization failed", None))
}

/// A JSON resource body for `uri`.
pub fn json_read<T: Serialize + ?Sized>(
    uri: &str,
    value: &T,
) -> Result<ReadResourceResult, ErrorData> {
    let text = serde_json::to_string(value)
        .map_err(|_| ErrorData::internal_error("resource serialization failed", None))?;
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("application/json"),
    ]))
}

/// A tool result with a short text summary and typed structured content.
pub fn structured_result<T: Serialize + ?Sized>(
    summary: impl Into<String>,
    output: &T,
) -> Result<CallToolResult, ErrorData> {
    let mut result = CallToolResult::success(vec![ContentBlock::text(summary.into())]);
    result.structured_content = Some(serialize(output)?);
    Ok(result)
}

/// A tool result for an addressable product (contract C02). `output` must
/// serialize to an object whose top-level `resultUri` equals `link.uri`;
/// anything else is an internal error, because it would publish a product the
/// caller cannot follow.
pub fn product_result<T: Serialize>(
    status: impl Into<String>,
    link: Resource,
    output: &T,
) -> Result<CallToolResult, ErrorData> {
    let mut result = CallToolResult::success(vec![
        ContentBlock::text(status.into()),
        ContentBlock::ResourceLink(link),
    ]);
    result.structured_content = Some(serialize(output)?);
    if !matches!(crate::task_completion::result_uri(&result), Ok(Some(_))) {
        return Err(ErrorData::internal_error(
            "product resultUri must match its resource link",
            None,
        ));
    }
    Ok(result)
}

/// A completion result from ranked candidates: at most
/// [`CompletionInfo::MAX_VALUES`] values. When candidates exceed that, the
/// result sets `has_more` and omits `total`, because a bounded query that
/// returned the candidates does not know the full count.
pub fn completion(mut values: Vec<String>) -> Result<CompleteResult, ErrorData> {
    let has_more = values.len() > CompletionInfo::MAX_VALUES;
    let total = if has_more {
        None
    } else {
        u32::try_from(values.len()).ok()
    };
    values.truncate(CompletionInfo::MAX_VALUES);
    let info = CompletionInfo::with_pagination(values, total, has_more)
        .map_err(|error| ErrorData::internal_error(error, None))?;
    Ok(CompleteResult::new(info))
}

/// Ranks candidates for a completion prefix: case-insensitive prefix matches
/// first, then substring matches, each in input order.
pub fn rank_completions<'a>(
    candidates: impl IntoIterator<Item = &'a str>,
    value: &str,
) -> Vec<String> {
    let needle = value.to_lowercase();
    let (mut prefixed, mut contained) = (Vec::new(), Vec::new());
    for candidate in candidates {
        let folded = candidate.to_lowercase();
        if folded.starts_with(&needle) {
            prefixed.push(candidate.to_owned());
        } else if folded.contains(&needle) {
            contained.push(candidate.to_owned());
        }
    }
    prefixed.extend(contained);
    prefixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Product {
        result_uri: veoveo_types::ResourceUri,
    }

    #[test]
    fn product_results_require_a_matching_result_uri() {
        let link = Resource::new("fixture://items/1", "item");
        let result = product_result(
            "done",
            link.clone(),
            &Product {
                result_uri: veoveo_types::ResourceUri::new("fixture://items/1").unwrap(),
            },
        )
        .unwrap();
        assert_eq!(result.content.len(), 2);
        assert_eq!(
            crate::task_completion::result_uri(&result)
                .unwrap()
                .unwrap()
                .as_str(),
            "fixture://items/1"
        );
        let plain = structured_result("done", &serde_json::json!({"count":1})).unwrap();
        assert_eq!(crate::task_completion::result_uri(&plain).unwrap(), None);
        assert_eq!(
            result.structured_content.as_ref().unwrap()["resultUri"],
            "fixture://items/1"
        );
        assert!(
            product_result(
                "done",
                link.clone(),
                &Product {
                    result_uri: veoveo_types::ResourceUri::new("fixture://items/2").unwrap()
                }
            )
            .is_err()
        );
        for output in [
            serde_json::json!({}),
            serde_json::json!({"result_uri":"fixture://items/1"}),
            serde_json::json!({"resultUri":"fixture://items/1", "result_uri":"fixture://items/1"}),
            serde_json::json!({"resultUri":null}),
            serde_json::json!({"resultUri":42}),
            serde_json::json!({"resultUri":"not a resource URI"}),
        ] {
            assert!(
                product_result("done", link.clone(), &output).is_err(),
                "{output}"
            );
        }
    }

    #[test]
    fn completions_rank_prefixes_first_and_report_only_known_totals() {
        assert_eq!(
            rank_completions(["design", "agents", "redesign"], "des"),
            ["design", "redesign"]
        );
        let many = (0..150).map(|i| format!("v{i}")).collect();
        let result = completion(many).unwrap();
        assert_eq!(result.completion.values.len(), CompletionInfo::MAX_VALUES);
        assert_eq!(result.completion.total, None);
        assert_eq!(result.completion.has_more, Some(true));
        let exact = completion(vec!["design".into()]).unwrap().completion;
        assert_eq!(exact.total, Some(1));
        assert_eq!(exact.has_more, Some(false));
    }
}
