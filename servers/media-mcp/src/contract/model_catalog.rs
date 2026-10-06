//! Registry-snapshot model paging shared by tools and resources.
use super::{MediaModelId, ModelCatalogItem, ModelCatalogOutput, ModelEntry, ModelsArgs};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use veoveo_types::{CursorCodec, ResourceFieldCodec, ResourceUri};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid Media catalog value or stale/foreign model cursor; restart the model catalog")]
pub struct ModelCatalogError;

pub fn normalized_model_filter(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelPosition {
    version: u8,
    collection: String,
    query: Option<String>,
    model_type: Option<String>,
    limit: u32,
    snapshot: veoveo_artifact_contract::UploadSha256,
    after: MediaModelId,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ModelCursorCodec;
impl veoveo_types::StatelessCursorCodec for ModelCursorCodec {}
impl CursorCodec for ModelCursorCodec {
    type Position = ModelPosition;
    type Error = ModelCatalogError;
    fn check(&self, p: &ModelPosition) -> Result<(), Self::Error> {
        if p.version != 1
            || p.collection != MediaModelIndexUri::ROOT
            || !(1..=100).contains(&p.limit)
            || p.query != normalized_model_filter(p.query.clone())
            || p.model_type != normalized_model_filter(p.model_type.clone())
        {
            return Err(ModelCatalogError);
        }
        Ok(())
    }
    fn encode(&self, p: &ModelPosition) -> Result<String, Self::Error> {
        self.check(p)?;
        serde_json::to_vec(p)
            .map(|b| URL_SAFE_NO_PAD.encode(b))
            .map_err(|_| ModelCatalogError)
    }
    fn decode(&self, wire: &str) -> Result<ModelPosition, Self::Error> {
        if wire.is_empty() || wire.len() > 4096 {
            return Err(ModelCatalogError);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(wire)
            .map_err(|_| ModelCatalogError)?;
        let value = serde_json::from_slice(&bytes).map_err(|_| ModelCatalogError)?;
        self.check(&value)?;
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct MediaModelCursor {
    #[schemars(with = "String")]
    cursor: veoveo_types::OpaqueCursor<ModelCursorCodec>,
}
impl MediaModelCursor {
    pub fn parse(wire: impl Into<String>) -> Result<Self, ModelCatalogError> {
        veoveo_types::OpaqueCursor::parse(ModelCursorCodec, wire).map(|cursor| Self { cursor })
    }
    pub fn as_str(&self) -> &str {
        self.cursor.as_str()
    }
    pub(super) fn check_page(
        &self,
        page: &super::ModelCatalogOutputValue,
    ) -> Result<(), ModelCatalogError> {
        let p = self.cursor.position();
        if page.returned != page.limit as usize
            || page.returned >= page.total_available
            || p.query != page.query
            || p.model_type != page.model_type
            || p.limit != page.limit
            || page.models.last().map(|m| &m.model_id) != Some(&p.after)
        {
            return Err(ModelCatalogError);
        }
        Ok(())
    }
}
struct ModelCursorField;
impl ResourceFieldCodec<MediaModelCursor> for ModelCursorField {
    type Error = ModelCatalogError;
    fn parse(value: &str) -> Result<MediaModelCursor, Self::Error> {
        MediaModelCursor::parse(value)
    }
    fn text(value: &MediaModelCursor) -> std::borrow::Cow<'_, str> {
        value.as_str().into()
    }
}
struct FilterField;
impl ResourceFieldCodec<String> for FilterField {
    type Error = ModelCatalogError;
    fn parse(value: &str) -> Result<String, Self::Error> {
        Ok(value.to_owned())
    }
    fn text(value: &String) -> std::borrow::Cow<'_, str> {
        value.into()
    }
}
struct LimitField;
impl ResourceFieldCodec<u32> for LimitField {
    type Error = ModelCatalogError;
    fn parse(value: &str) -> Result<u32, Self::Error> {
        let n = value.parse().map_err(|_| ModelCatalogError)?;
        if !(1..=100).contains(&n) {
            return Err(ModelCatalogError);
        }
        Ok(n)
    }
    fn text(value: &u32) -> std::borrow::Cow<'_, str> {
        value.to_string().into()
    }
}
#[veoveo_types::resource_address(
    cached(ModelCatalogErrorAddresses),
    template = "media://models{?query,type,limit,cursor}"
)]
/// Model catalog and prediction continuations cannot be interchanged.
/// ```compile_fail
/// use veoveo_media_mcp::contract::{MediaModelIndexUri, MediaPredictionId};
/// let prediction = MediaPredictionId::parse("prediction").unwrap();
/// MediaModelIndexUri::new(None, None, None, Some(&prediction));
/// ```
pub struct MediaModelIndexUri {
    #[resource(cache)]
    wire: ResourceUri,
    #[resource(variable = "query", codec = FilterField, accessor = query_filter, argument = optional_borrowed)]
    query_filter: Option<String>,
    #[resource(variable = "type", codec = FilterField, accessor = model_type, argument = optional_borrowed)]
    model_type: Option<String>,
    #[resource(codec = LimitField, accessor = limit, argument = optional_borrowed)]
    limit: Option<u32>,
    #[resource(codec = ModelCursorField, accessor = cursor, argument = optional_borrowed)]
    cursor: Option<MediaModelCursor>,
}
impl MediaModelIndexUri {
    pub const ROOT: &str = Self::RESOURCE_ROOT;
    pub const TEMPLATE: &str = Self::RESOURCE_TEMPLATE;
    pub fn arguments(&self) -> ModelsArgs {
        ModelsArgs {
            query: self.query_filter.clone(),
            model_type: self.model_type.clone(),
            limit: self.limit,
            cursor: self.cursor.clone(),
        }
    }
}

/// Admission precedes installing an index in the provider cache.
pub fn validate_model_registry(models: &[ModelEntry]) -> Result<(), ModelCatalogError> {
    let mut seen = std::collections::BTreeSet::new();
    for model in models {
        if !seen.insert(&model.model_id) || model.base_price.is_some_and(|price| !price.is_finite())
        {
            return Err(ModelCatalogError);
        }
    }
    Ok(())
}

pub fn model_catalog_page(
    models: &[ModelEntry],
    args: ModelsArgs,
) -> Result<ModelCatalogOutput, ModelCatalogError> {
    validate_model_registry(models)?;
    let query = normalized_model_filter(args.query);
    let model_type = normalized_model_filter(args.model_type);
    let limit = args.limit.unwrap_or(20);
    if !(1..=100).contains(&limit) {
        return Err(ModelCatalogError);
    }
    let mut registry: Vec<_> = models.iter().collect();
    registry.sort_by(|a, b| a.model_id.cmp(&b.model_id));
    let digest = Sha256::digest(serde_json::to_vec(&registry).map_err(|_| ModelCatalogError)?);
    let snapshot = veoveo_artifact_contract::UploadSha256::parse(hex::encode(digest))
        .map_err(|_| ModelCatalogError)?;
    let after = if let Some(cursor) = args.cursor {
        let p = cursor.cursor.position();
        if p.query != query
            || p.model_type != model_type
            || p.limit != limit
            || p.snapshot != snapshot
        {
            return Err(ModelCatalogError);
        }
        Some(p.after.clone())
    } else {
        None
    };
    let filtered: Vec<_> = registry
        .into_iter()
        .filter(|m| {
            model_type
                .as_ref()
                .is_none_or(|t| m.model_type.eq_ignore_ascii_case(t))
                && query.as_ref().is_none_or(|q| {
                    [m.model_id.as_str(), &m.name, &m.model_type, &m.description]
                        .iter()
                        .any(|s| s.to_ascii_lowercase().contains(q))
                })
        })
        .collect();
    let total_available = filtered.len();
    if let Some(after) = &after
        && !filtered.iter().any(|m| &m.model_id == after)
    {
        return Err(ModelCatalogError);
    }
    let mut remaining = filtered
        .into_iter()
        .filter(|m| after.as_ref().is_none_or(|a| &m.model_id > a));
    let page: Vec<_> = remaining
        .by_ref()
        .take(limit as usize)
        .map(catalog_item)
        .collect::<Result<_, _>>()?;
    let next_cursor = if remaining.next().is_some() {
        let position = ModelPosition {
            version: 1,
            collection: MediaModelIndexUri::ROOT.into(),
            query: query.clone(),
            model_type: model_type.clone(),
            limit,
            snapshot,
            after: page.last().ok_or(ModelCatalogError)?.model_id.clone(),
        };
        Some(MediaModelCursor {
            cursor: veoveo_types::OpaqueCursor::try_new(ModelCursorCodec, position)?,
        })
    } else {
        None
    };
    super::ModelCatalogOutputValue {
        query,
        model_type,
        total_available,
        returned: page.len(),
        models: page,
        limit,
        next_cursor,
    }
    .build()
}
fn catalog_item(m: &ModelEntry) -> Result<ModelCatalogItem, ModelCatalogError> {
    super::ModelCatalogItemValue {
        model_id: m.model_id.clone(),
        name: m.name.clone(),
        model_type: m.model_type.clone(),
        description: m.description.clone(),
        base_price: m.base_price,
        schema_uri: super::MediaModelUri::new(m.model_id.clone()),
    }
    .build()
}

pub struct ModelCatalogErrorAddresses;
impl veoveo_types::ResourceProfile for ModelCatalogErrorAddresses {
    type Error = ModelCatalogError;
    const PROFILE: veoveo_types::ResourceProfileSpec<Self::Error> =
        veoveo_types::ResourceProfileSpec {
            route_error: |_, _| ModelCatalogError,
        };
}
