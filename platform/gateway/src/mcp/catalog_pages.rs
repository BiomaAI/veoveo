//! Pages over currently admitted descriptors, independent of upstream insertion order.
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rmcp::model::{
    ErrorData as McpError, PaginatedRequestParams, Prompt, Resource, ResourceTemplate, Tool,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use veoveo_gateway_contract::GatewayToolName;
use veoveo_mcp_contract::Page;
use veoveo_types::{PromptName, ResourceTemplateUri, ResourceUri};

use crate::mcp_support::{mcp_internal, mcp_invalid_params};

const MAX_CURSOR_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Surface {
    Tools,
    Resources,
    Templates,
    Prompts,
}

#[derive(Serialize, Deserialize)]
enum Version {
    #[serde(rename = "1")]
    V1,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor<K> {
    version: Version,
    surface: Surface,
    after: K,
}

pub(super) trait CatalogItem {
    type Key: Ord + Serialize + DeserializeOwned;
    const SURFACE: Surface;
    fn key(&self) -> Result<Self::Key, McpError>;
}

impl CatalogItem for Tool {
    type Key = GatewayToolName;
    const SURFACE: Surface = Surface::Tools;
    fn key(&self) -> Result<Self::Key, McpError> {
        GatewayToolName::new(self.name.as_ref())
            .map_err(|_| mcp_internal("invalid tool identity in admitted catalog"))
    }
}
impl CatalogItem for Prompt {
    type Key = PromptName;
    const SURFACE: Surface = Surface::Prompts;
    fn key(&self) -> Result<Self::Key, McpError> {
        PromptName::new(self.name.clone())
            .map_err(|_| mcp_internal("invalid prompt identity in admitted catalog"))
    }
}
impl CatalogItem for Resource {
    type Key = ResourceUri;
    const SURFACE: Surface = Surface::Resources;
    fn key(&self) -> Result<Self::Key, McpError> {
        ResourceUri::new(self.uri.clone())
            .map_err(|_| mcp_internal("invalid resource identity in admitted catalog"))
    }
}
impl CatalogItem for ResourceTemplate {
    type Key = ResourceTemplateUri;
    const SURFACE: Surface = Surface::Templates;
    fn key(&self) -> Result<Self::Key, McpError> {
        ResourceTemplateUri::new(self.uri_template.clone())
            .map_err(|_| mcp_internal("invalid template identity in admitted catalog"))
    }
}

/// Admission runs before this function on every request. A cursor carries only
/// an exclusive position, never cached descriptors or authority.
pub(super) fn page<T: CatalogItem>(
    items: Vec<T>,
    request: Option<&PaginatedRequestParams>,
) -> Result<Page<T>, McpError> {
    let after = request
        .and_then(|request| request.cursor.as_deref())
        .map(decode::<T>)
        .transpose()?;
    let mut keyed = items
        .into_iter()
        .map(|item| Ok((item.key()?, item)))
        .collect::<Result<Vec<_>, McpError>>()?;
    keyed.sort_unstable_by(|(left, _), (right, _)| left.cmp(right));
    if keyed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(mcp_internal(
            "admitted catalog contains duplicate identities",
        ));
    }
    let mut remaining = keyed
        .into_iter()
        .filter(|(key, _)| after.as_ref().is_none_or(|after| key > after));
    let selected = remaining
        .by_ref()
        .take(super::GATEWAY_PAGE_SIZE)
        .collect::<Vec<_>>();
    let next_cursor = if remaining.next().is_some() {
        let (last, _) = selected.last().expect("positive gateway page size");
        use veoveo_types::CursorCodec;
        let encoded = CatalogCursorCodec::<T::Key>::new(T::SURFACE).encode(last)?;
        Some(encoded)
    } else {
        None
    };
    Ok(Page {
        items: selected.into_iter().map(|(_, item)| item).collect(),
        next_cursor,
    })
}

struct CatalogCursorCodec<K> {
    surface: Surface,
    marker: std::marker::PhantomData<K>,
}
impl<K> CatalogCursorCodec<K> {
    fn new(surface: Surface) -> Self {
        Self {
            surface,
            marker: std::marker::PhantomData,
        }
    }
}
impl<K: Serialize + DeserializeOwned> veoveo_types::CursorCodec for CatalogCursorCodec<K> {
    type Position = K;
    type Error = McpError;
    fn check(&self, _position: &K) -> Result<(), McpError> {
        Ok(())
    }
    fn encode(&self, position: &K) -> Result<String, McpError> {
        let bytes = serde_json::to_vec(&Cursor {
            version: Version::V1,
            surface: self.surface,
            after: position,
        })
        .map_err(|_| mcp_internal("catalog cursor serialization failed"))?;
        let encoded = URL_SAFE_NO_PAD.encode(bytes);
        if encoded.len() > MAX_CURSOR_BYTES {
            return Err(mcp_internal(
                "catalog position exceeds the 16 KiB cursor limit",
            ));
        }
        Ok(encoded)
    }
    fn decode(&self, encoded: &str) -> Result<K, McpError> {
        let invalid =
            || mcp_invalid_params("invalid catalog cursor; restart enumeration without a cursor");
        if encoded.len() > MAX_CURSOR_BYTES {
            return Err(invalid());
        }
        let bytes = URL_SAFE_NO_PAD.decode(encoded).map_err(|_| invalid())?;
        let cursor: Cursor<K> = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        if cursor.surface != self.surface {
            return Err(invalid());
        }
        Ok(cursor.after)
    }
}
fn decode<T: CatalogItem>(encoded: &str) -> Result<T::Key, McpError> {
    use veoveo_types::CursorCodec;
    let codec = CatalogCursorCodec::<T::Key>::new(T::SURFACE);
    let position = codec.decode(encoded)?;
    codec.check(&position)?;
    Ok(position)
}

#[cfg(test)]
mod tests;
