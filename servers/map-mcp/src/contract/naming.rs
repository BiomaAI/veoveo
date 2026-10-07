//! Narrow schema roles for standard GIS values and the owner format tag.
use schemars::Schema;
use veoveo_types::naming::{
    NamingAuthority, NamingDeclaration, NamingLabel, ScalarGrammar, ScalarNaming, scalar_schema,
};

fn standard_scalar(schema: &mut Schema, document: &str, profile: &str, revision: &str) {
    let declaration = NamingDeclaration {
        authority: NamingAuthority::Standard {
            document: veoveo_types::HttpsUrl::parse(document).expect("declared standard URL"),
        },
        profile: NamingLabel::new(profile).expect("declared standard profile"),
        version: NamingLabel::new(revision).expect("declared revision"),
        applicability: NamingLabel::new("only this standard scalar vocabulary")
            .expect("declared scalar"),
    };
    *schema = scalar_schema(schema.clone(), ScalarNaming::Standard { declaration })
        .expect("standard scalar schema role");
}

pub(super) fn geojson_scalar(schema: &mut Schema) {
    standard_scalar(
        schema,
        "https://www.rfc-editor.org/rfc/rfc7946",
        "GeoJSON type names",
        "RFC 7946",
    );
}

pub(super) fn cql2_operator(schema: &mut Schema) {
    standard_scalar(
        schema,
        "https://docs.ogc.org/is/21-065r2/21-065r2.html",
        "CQL2 operator names",
        "1.0",
    );
}

pub(super) fn format_tag(schema: &mut Schema) {
    *schema = scalar_schema(
        schema.clone(),
        ScalarNaming::builtin(ScalarGrammar::FormatTag),
    )
    .expect("owner format tag schema role");
}

pub(super) fn geojson_geometry(schema: &mut Schema) {
    // The object fields remain inspected; only each generated type discriminator
    // uses the RFC 7946 vocabulary.
    for keyword in ["oneOf", "anyOf"] {
        if let Some(branches) = schema
            .get_mut(keyword)
            .and_then(serde_json::Value::as_array_mut)
        {
            for branch in branches {
                if let Some(tag) = branch.pointer_mut("/properties/type") {
                    let mut child = Schema::try_from(tag.clone()).expect("generated type schema");
                    geojson_scalar(&mut child);
                    *tag = child.to_value();
                }
            }
        }
    }
}
