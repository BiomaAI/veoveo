use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use duckdb::{params_from_iter, types::Value as DuckValue};

use crate::{
    analytics::MapAnalytics,
    contract::{
        Cql2Expression, Cql2Filter, Cql2Literal, Cql2Operator, MapFeature, MapFeatureId,
        QueryFeaturesOutput, QueryFeaturesRequest, Wgs84BoundingBox,
    },
};

const MAX_QUERY_LIMIT: u32 = 1_000;
const MAX_FILTER_DEPTH: usize = 16;
const MAX_FILTER_NODES: usize = 64;

pub(super) fn query_features(
    analytics: &MapAnalytics,
    tenant_key: &str,
    work_context_key: &str,
    request: &QueryFeaturesRequest,
    publication_revision: Option<u64>,
    projection_sequence: u64,
) -> Result<QueryFeaturesOutput> {
    if !(1..=MAX_QUERY_LIMIT).contains(&request.limit) {
        bail!("feature query limit must be between 1 and {MAX_QUERY_LIMIT}");
    }
    if let Some(bbox) = &request.bbox {
        bbox.validate()?;
    }
    if let Some(interval) = &request.datetime {
        interval.validate()?;
    }
    let cursor = request.cursor.as_deref().map(decode_cursor).transpose()?;

    let mut parameters = vec![
        DuckValue::Text(tenant_key.to_owned()),
        DuckValue::Text(work_context_key.to_owned()),
        DuckValue::Text(request.layer_id.to_string()),
    ];
    let source = if let Some(layer_revision) = publication_revision {
        parameters.push(DuckValue::BigInt(i64::try_from(layer_revision)?));
        "SELECT * EXCLUDE (version_rank) FROM (
           SELECT *, row_number() OVER (PARTITION BY feature_key ORDER BY feature_revision DESC) AS version_rank
           FROM map_authored_feature_revision
           WHERE tenant_key = ? AND work_context_key = ? AND layer_key = ? AND layer_revision <= ?
         ) WHERE version_rank = 1"
    } else {
        "SELECT * FROM map_authored_feature_head
         WHERE tenant_key = ? AND work_context_key = ? AND layer_key = ?"
    };
    let mut predicates = vec!["authored.deleted = false".to_owned()];
    if let Some(bbox) = &request.bbox {
        predicates.push(bbox_predicate(bbox));
    }
    if let Some(interval) = &request.datetime {
        if let Some(start) = interval.interval[0].as_timestamp() {
            predicates.push(
                "(authored.valid_until IS NULL OR authored.valid_until >= ?::TIMESTAMPTZ)"
                    .to_owned(),
            );
            parameters.push(DuckValue::Text(start.to_rfc3339()));
        }
        if let Some(end) = interval.interval[1].as_timestamp() {
            predicates.push(
                "(authored.valid_from IS NULL OR authored.valid_from <= ?::TIMESTAMPTZ)".to_owned(),
            );
            parameters.push(DuckValue::Text(end.to_rfc3339()));
        }
    }
    if let Some(geometry_type) = &request.geometry_type {
        predicates.push("authored.geometry_type = ?".to_owned());
        parameters.push(DuckValue::Text(
            serde_json::to_value(geometry_type)?
                .as_str()
                .context("feature geometry type must serialize as a string")?
                .to_owned(),
        ));
    }
    if let Some(filter) = &request.filter {
        let mut compiler = FilterCompiler::default();
        predicates.push(compiler.compile(filter, 1)?);
        parameters.extend(compiler.parameters);
    }
    if let Some(cursor) = cursor {
        predicates.push("authored.feature_key > ?".to_owned());
        parameters.push(DuckValue::Text(cursor.to_string()));
    }
    parameters.push(DuckValue::BigInt(i64::from(request.limit) + 1));
    let sql = format!(
        "SELECT authored.canonical_json, authored.feature_key
         FROM ({source}) AS authored
         WHERE {}
         ORDER BY authored.feature_key ASC
         LIMIT ?",
        predicates.join(" AND ")
    );
    let connection = analytics.connection(true)?;
    let mut statement = connection.prepare(&sql)?;
    let mut rows = statement.query(params_from_iter(parameters.iter()))?;
    let mut features = Vec::new();
    while let Some(row) = rows.next()? {
        let canonical_json: String = row.get(0)?;
        let key: String = row.get(1)?;
        let feature: MapFeature = serde_json::from_str(&canonical_json)
            .context("decoding projected authored map feature")?;
        if feature.id.as_str() != key || feature.layer_id != request.layer_id {
            bail!("projected authored map feature identity is inconsistent");
        }
        features.push(feature);
    }
    let next_cursor = if features.len() > request.limit as usize {
        features.pop();
        features.last().map(|feature| encode_cursor(&feature.id))
    } else {
        None
    };
    Ok(QueryFeaturesOutput {
        layer_id: request.layer_id.clone(),
        features,
        next_cursor,
        projection_sequence,
    })
}

fn bbox_predicate(bbox: &Wgs84BoundingBox) -> String {
    let polygon = |west: f64, east: f64| {
        format!(
            "ST_Intersects(authored.geometry, ST_GeomFromText('POLYGON(({west} {south}, {east} {south}, {east} {north}, {west} {north}, {west} {south}))'))",
            south = bbox.south,
            north = bbox.north,
        )
    };
    if bbox.west <= bbox.east {
        polygon(bbox.west, bbox.east)
    } else {
        format!(
            "({} OR {})",
            polygon(bbox.west, 180.0),
            polygon(-180.0, bbox.east)
        )
    }
}

#[derive(Default)]
struct FilterCompiler {
    nodes: usize,
    parameters: Vec<DuckValue>,
}

impl FilterCompiler {
    fn compile(&mut self, filter: &Cql2Filter, depth: usize) -> Result<String> {
        if depth > MAX_FILTER_DEPTH {
            bail!("CQL2 filter exceeds maximum depth {MAX_FILTER_DEPTH}");
        }
        self.nodes += 1;
        if self.nodes > MAX_FILTER_NODES {
            bail!("CQL2 filter exceeds maximum node count {MAX_FILTER_NODES}");
        }
        match filter.op {
            Cql2Operator::And => self.logical("AND", &filter.args, depth),
            Cql2Operator::Or => self.logical("OR", &filter.args, depth),
            Cql2Operator::Not => {
                let [argument] = filter.args.as_slice() else {
                    bail!("the CQL2 not operator requires exactly one argument");
                };
                Ok(format!("(NOT {})", self.predicate(argument, depth + 1)?))
            }
            Cql2Operator::Equal => self.comparison(&filter.args, "=", false),
            Cql2Operator::NotEqual => self.comparison(&filter.args, "<>", false),
            Cql2Operator::LessThan => self.comparison(&filter.args, "<", true),
            Cql2Operator::LessThanOrEqual => self.comparison(&filter.args, "<=", true),
            Cql2Operator::GreaterThan => self.comparison(&filter.args, ">", true),
            Cql2Operator::GreaterThanOrEqual => self.comparison(&filter.args, ">=", true),
            Cql2Operator::IsNull => {
                let [Cql2Expression::Property(property)] = filter.args.as_slice() else {
                    bail!("the CQL2 isNull operator requires one property reference");
                };
                self.count_operands(1)?;
                let path = property_path(&property.property)?;
                self.parameters.push(DuckValue::Text(path.clone()));
                self.parameters.push(DuckValue::Text(path));
                Ok("(json_extract(authored.properties_json, ?) IS NULL OR json_type(authored.properties_json, ?) = 'NULL')".to_owned())
            }
        }
    }

    fn predicate(&mut self, expression: &Cql2Expression, depth: usize) -> Result<String> {
        let Cql2Expression::Operation(filter) = expression else {
            bail!("a CQL2 logical argument must be a predicate object");
        };
        self.compile(filter, depth)
    }

    fn logical(&mut self, operator: &str, args: &[Cql2Expression], depth: usize) -> Result<String> {
        if args.is_empty() || args.len() > 16 {
            bail!("a CQL2 logical operation must contain between 1 and 16 arguments");
        }
        let compiled = args
            .iter()
            .map(|argument| self.predicate(argument, depth + 1))
            .collect::<Result<Vec<_>>>()?;
        Ok(format!("({})", compiled.join(&format!(" {operator} "))))
    }

    fn comparison(
        &mut self,
        args: &[Cql2Expression],
        operator: &str,
        ordered: bool,
    ) -> Result<String> {
        let [
            Cql2Expression::Property(property),
            Cql2Expression::Literal(value),
        ] = args
        else {
            bail!(
                "a supported CQL2 comparison requires a property reference followed by a scalar literal"
            );
        };
        self.count_operands(2)?;
        let path = property_path(&property.property)?;
        self.parameters.push(DuckValue::Text(path));
        let expression = match value {
            Cql2Literal::String(value) => {
                self.parameters.push(DuckValue::Text(value.clone()));
                "json_extract_string(authored.properties_json, ?)"
            }
            Cql2Literal::Number(value) if value.is_finite() => {
                self.parameters.push(DuckValue::Double(*value));
                "TRY_CAST(json_extract_string(authored.properties_json, ?) AS DOUBLE)"
            }
            Cql2Literal::Boolean(value) if !ordered => {
                self.parameters.push(DuckValue::Boolean(*value));
                "TRY_CAST(json_extract_string(authored.properties_json, ?) AS BOOLEAN)"
            }
            Cql2Literal::Number(_) => {
                bail!("CQL2 numeric literal is outside the supported finite range")
            }
            Cql2Literal::Null(()) => {
                bail!("use the CQL2 isNull operator for null comparisons")
            }
            Cql2Literal::Boolean(_) => bail!("ordered CQL2 comparisons do not accept booleans"),
        };
        Ok(format!("({expression} {operator} ?)"))
    }

    fn count_operands(&mut self, count: usize) -> Result<()> {
        self.nodes += count;
        if self.nodes > MAX_FILTER_NODES {
            bail!("CQL2 filter exceeds maximum node count {MAX_FILTER_NODES}");
        }
        Ok(())
    }
}

fn property_path(property: &str) -> Result<String> {
    if property.trim().is_empty() || property.len() > 256 || property.chars().any(char::is_control)
    {
        bail!("CQL2 property name must be 1..=256 bytes without control characters");
    }
    Ok(format!(
        "/{}",
        property.replace('~', "~0").replace('/', "~1")
    ))
}

fn encode_cursor(feature_id: &MapFeatureId) -> String {
    URL_SAFE_NO_PAD.encode(feature_id.as_str())
}

fn decode_cursor(cursor: &str) -> Result<MapFeatureId> {
    if cursor.len() > 256 {
        bail!("feature query cursor exceeds its byte limit");
    }
    let decoded = URL_SAFE_NO_PAD
        .decode(cursor)
        .context("feature query cursor is not valid base64url")?;
    let decoded = String::from_utf8(decoded).context("feature query cursor is not UTF-8")?;
    decoded
        .parse()
        .context("feature query cursor has an invalid feature id")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cql2_properties_are_bound_as_json_pointers() {
        let mut compiler = FilterCompiler::default();
        let filter: Cql2Filter = serde_json::from_value(serde_json::json!({
            "op": "=",
            "args": [
                {"property": "name') OR true--"},
                "yard"
            ]
        }))
        .unwrap();
        let sql = compiler.compile(&filter, 1).unwrap();
        assert_eq!(
            sql,
            "(json_extract_string(authored.properties_json, ?) = ?)"
        );
        assert_eq!(compiler.parameters.len(), 2);
    }

    #[test]
    fn cql2_json_round_trips_the_standard_operation_shape() {
        let value = serde_json::json!({
            "op": "and",
            "args": [
                {"op": "=", "args": [{"property": "kind"}, "hospital"]},
                {"op": "isNull", "args": [{"property": "closed_at"}]}
            ]
        });
        let filter: Cql2Filter = serde_json::from_value(value.clone()).unwrap();

        assert_eq!(serde_json::to_value(filter).unwrap(), value);
    }

    #[test]
    fn cursors_round_trip_canonical_feature_ids() {
        let id = MapFeatureId::new();
        assert_eq!(decode_cursor(&encode_cursor(&id)).unwrap(), id);
    }

    #[test]
    fn dateline_bbox_is_split_into_two_query_polygons() {
        let predicate = bbox_predicate(&Wgs84BoundingBox {
            west: 170.0,
            south: -10.0,
            east: -170.0,
            north: 10.0,
        });
        assert!(predicate.contains("170 -10"));
        assert!(predicate.contains("-180 -10"));
        assert!(predicate.contains(" OR "));
    }
}
