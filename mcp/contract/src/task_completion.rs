//! Admission of MCP product addresses before durable Task settlement.
use rmcp::{
    ErrorData,
    model::{CallToolResult, ContentBlock},
};
use veoveo_types::ResourceUri;

/// Returns the C02 product address after checking its matching resource link.
/// Results without a declared product carry no product address.
pub fn result_uri(result: &CallToolResult) -> Result<Option<ResourceUri>, ErrorData> {
    let invalid = || {
        ErrorData::internal_error(
            "Task completion product address disagrees with its MCP result",
            None,
        )
    };
    if result
        .structured_content
        .as_ref()
        .is_some_and(|value| value.get("result_uri").is_some())
    {
        return Err(invalid());
    }
    let declared = result
        .structured_content
        .as_ref()
        .and_then(|value| value.get("resultUri"));
    let links: Vec<_> = result
        .content
        .iter()
        .filter_map(|content| match content {
            ContentBlock::ResourceLink(link) => Some(link),
            _ => None,
        })
        .collect();
    let Some(declared) = declared else {
        return if links.is_empty() {
            Ok(None)
        } else {
            Err(invalid())
        };
    };
    let uri = ResourceUri::new(declared.as_str().ok_or_else(invalid)?).map_err(|_| invalid())?;
    if links.len() != 1 || links[0].uri != uri.as_str() {
        return Err(invalid());
    }
    Ok(Some(uri))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::Resource;
    use serde_json::json;

    #[test]
    fn product_plain_and_tool_error_addresses_are_admitted_once() {
        let product = crate::hosting::product_result(
            "done",
            Resource::new("fixture://items/1", "item"),
            &json!({"resultUri":"fixture://items/1"}),
        )
        .unwrap();
        assert_eq!(
            result_uri(&product).unwrap().unwrap().as_str(),
            "fixture://items/1"
        );
        assert_eq!(
            result_uri(&product.clone()).unwrap(),
            result_uri(&product).unwrap()
        );
        let plain = CallToolResult::success(vec![ContentBlock::text("done")]);
        assert_eq!(result_uri(&plain).unwrap(), None);
        let mut error = plain.clone();
        error.is_error = Some(true);
        assert_eq!(result_uri(&error).unwrap(), None);
        let mut mismatch = product.clone();
        mismatch.structured_content = Some(json!({"resultUri":"fixture://items/2"}));
        assert!(result_uri(&mismatch).is_err());
        mismatch.structured_content = Some(json!({"resultUri":"not a resource address"}));
        assert!(result_uri(&mismatch).is_err());
        mismatch.structured_content = Some(json!({"resultUri":42}));
        assert!(result_uri(&mismatch).is_err());
        mismatch.structured_content = Some(json!({"result_uri":"fixture://items/1"}));
        assert!(result_uri(&mismatch).is_err());
        mismatch.structured_content =
            Some(json!({"resultUri":"fixture://items/1", "result_uri":"fixture://items/1"}));
        assert!(result_uri(&mismatch).is_err());
        mismatch = product.clone();
        mismatch.is_error = Some(true);
        assert!(result_uri(&mismatch).unwrap().is_some());
        mismatch = product;
        mismatch.content.push(mismatch.content[1].clone());
        assert!(result_uri(&mismatch).is_err());
    }
}
