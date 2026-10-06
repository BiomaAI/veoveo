//! Portable relationships; SQL visibility and current authority stay with readers.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("Map product identity, parent, revision or value relationship is invalid")]
pub struct MapRelationshipError;

pub(super) fn check_route(value: &RoutePlanValue) -> Result<(), MapRelationshipError> {
    if value.route_uri.id() != &value.route_id {
        return Err(MapRelationshipError);
    }
    Ok(())
}

pub(super) fn check_product(value: &LayerProductValue) -> Result<(), MapRelationshipError> {
    if !matches!(
        value.artifact_uri.address(),
        veoveo_artifact_contract::ArtifactAddress::Plane(_)
    ) || value.digest_sha256.len() != 64
        || !value
            .digest_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(MapRelationshipError);
    }
    Ok(())
}

pub(super) fn check_composition_revision(
    value: &MapCompositionRevisionValue,
) -> Result<(), MapRelationshipError> {
    if value.revision == 0 || value.layers.len() > MAX_COMPOSITION_LAYERS {
        return Err(MapRelationshipError);
    }
    value.view.validate().map_err(|_| MapRelationshipError)?;
    for layer in &value.layers {
        if !layer.opacity.is_finite() || !(0.0..=1.0).contains(&layer.opacity) {
            return Err(MapRelationshipError);
        }
    }
    Ok(())
}

pub(super) fn check_composition(value: &MapCompositionValue) -> Result<(), MapRelationshipError> {
    if value.current.composition_id != value.composition_id {
        return Err(MapRelationshipError);
    }
    Ok(())
}

pub(super) fn check_restriction(value: &RestrictionValue) -> Result<(), MapRelationshipError> {
    value
        .geometry
        .validate()
        .map_err(|_| MapRelationshipError)?;
    if value.affected_mobility_families.is_empty()
        || value.record_version == 0
        || value
            .valid_until
            .is_some_and(|until| until <= value.valid_from)
        || (value.effect.kind == RestrictionEffectKind::Limit && value.effect.limit.is_none())
    {
        return Err(MapRelationshipError);
    }
    if let Some(band) = &value.vertical_band
        && (band.lower_m.is_some_and(|v| !v.is_finite())
            || band.upper_m.is_some_and(|v| !v.is_finite())
            || band
                .lower_m
                .zip(band.upper_m)
                .is_some_and(|(lower, upper)| lower > upper))
    {
        return Err(MapRelationshipError);
    }
    Ok(())
}
pub(super) fn check_acquisition(value: &AcquisitionJobValue) -> Result<(), MapRelationshipError> {
    value
        .requested_coverage
        .validate()
        .map_err(|_| MapRelationshipError)?;
    if value.record_version == 0
        || value
            .expected_source_digest_sha256
            .as_ref()
            .is_some_and(|v| v.len() != 64 || !v.bytes().all(|b| b.is_ascii_hexdigit()))
        || value.raw_artifact_uri.as_ref().is_some_and(|v| {
            !matches!(
                v.address(),
                veoveo_artifact_contract::ArtifactAddress::Plane(_)
            )
        })
    {
        return Err(MapRelationshipError);
    }
    Ok(())
}

pub(super) fn check_layer(value: &FeatureLayerValue) -> Result<(), MapRelationshipError> {
    if value.schema.version == 0
        || value.schema.layer_id != value.layer_id
        || value
            .style
            .as_ref()
            .is_some_and(|style| style.layer_id != value.layer_id || style.version == 0)
    {
        return Err(MapRelationshipError);
    }
    Ok(())
}
pub(super) fn check_publication(value: &LayerPublicationValue) -> Result<(), MapRelationshipError> {
    if value.schema_version == 0
        || value.artifact_uris.iter().any(|uri| {
            !matches!(
                uri.address(),
                veoveo_artifact_contract::ArtifactAddress::Plane(_)
            )
        })
    {
        return Err(MapRelationshipError);
    }
    Ok(())
}
pub(super) fn check_feature(value: &MapFeatureValue) -> Result<(), MapRelationshipError> {
    value
        .geometry
        .validate()
        .map_err(|_| MapRelationshipError)?;
    if value.feature_revision == 0
        || value.layer_revision == 0
        || value.schema_version == 0
        || value.related_resources.len() > 64
        || value.evidence_resources.len() > 64
    {
        return Err(MapRelationshipError);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    fn restriction() -> RestrictionValue {
        let now = chrono::Utc::now();
        RestrictionValue {
            restriction_id: RestrictionId::new(),
            kind: RestrictionKind::NavigationalWarning,
            geometry: Wgs84Polygon {
                exterior: [(0., 0.), (1., 0.), (1., 1.), (0., 1.), (0., 0.)]
                    .into_iter()
                    .map(|(x, y)| Wgs84Position::new(x, y, None).unwrap())
                    .collect(),
                interiors: vec![],
            },
            vertical_band: None,
            affected_mobility_families: BTreeSet::from([MobilityFamily::Human]),
            effect: RestrictionEffect {
                kind: RestrictionEffectKind::Advise,
                limit: None,
                explanation: None,
            },
            valid_from: now,
            valid_until: None,
            authority: AuthorityClass::SyntheticTest,
            source_release_id: None,
            issued_at: now,
            cancelled_by: None,
            record_version: 1,
        }
    }
    #[test]
    fn restriction_admission_preserves_optional_limits_and_signed_equal_unbounded_heights() {
        let mut value = restriction();
        for band in [
            None,
            Some(VerticalBand {
                lower_m: None,
                upper_m: None,
                reference: VerticalReference::ChartDatum,
            }),
            Some(VerticalBand {
                lower_m: Some(-10.),
                upper_m: Some(-10.),
                reference: VerticalReference::MeanSeaLevel,
            }),
        ] {
            value.vertical_band = band;
            let admitted = Restriction::new(value.clone()).unwrap();
            assert!(
                serde_json::from_value::<Restriction>(serde_json::to_value(&admitted).unwrap())
                    .is_ok()
            );
        }
        let limit = RestrictionLimit::MaximumHeight {
            value: Meters::new(5.).unwrap(),
        };
        for effect in [
            RestrictionEffectKind::Advise,
            RestrictionEffectKind::Require,
            RestrictionEffectKind::Penalize,
            RestrictionEffectKind::Prohibit,
            RestrictionEffectKind::Limit,
        ] {
            value.effect.kind = effect;
            value.effect.limit = Some(limit.clone());
            assert!(Restriction::new(value.clone()).is_ok());
        }
        value.effect.limit = None;
        assert!(Restriction::new(value.clone()).is_err());
        assert!(
            serde_json::from_value::<Restriction>(serde_json::to_value(&value).unwrap()).is_err()
        );
        value.effect.kind = RestrictionEffectKind::Advise;
        value.vertical_band = Some(VerticalBand {
            lower_m: Some(2.),
            upper_m: Some(1.),
            reference: VerticalReference::Ellipsoid,
        });
        assert!(Restriction::new(value.clone()).is_err());
        assert!(
            serde_json::from_value::<Restriction>(serde_json::to_value(&value).unwrap()).is_err()
        );
        value.vertical_band.as_mut().unwrap().lower_m = Some(f64::INFINITY);
        assert!(Restriction::new(value).is_err());
        let mut admitted = Restriction::new(restriction()).unwrap();
        admitted.record_version = 0;
        assert!(
            serde_json::to_value(admitted).is_err(),
            "mutable progress cannot serialize invalid values"
        );
    }

    #[test]
    fn product_artifact_and_composition_parent_relationships_share_builder_and_decoder_admission() {
        let product = LayerProductValue {
            product_id: LayerProductId::new(),
            publication_id: LayerPublicationId::new(),
            layer_id: FeatureLayerId::new(),
            layer_revision: 0,
            format: LayerProductFormat::GeoJsonSeq,
            artifact_uri: veoveo_artifact_contract::ArtifactId::new().plane_uri(),
            mime_type: "application/geo+json-seq".into(),
            digest_sha256: "a".repeat(64),
            size_bytes: 0,
            feature_count: 0,
            created_by: veoveo_types::PrincipalId::parse("author").unwrap(),
            work_context: veoveo_types::WorkContextId::parse("work").unwrap(),
            created_at: chrono::Utc::now(),
        };
        let good = LayerProduct::new(product.clone()).unwrap();
        assert!(
            serde_json::from_value::<LayerProduct>(serde_json::to_value(good).unwrap()).is_ok()
        );
        let mut bad = product;
        bad.digest_sha256 = "bad".into();
        assert!(LayerProduct::new(bad.clone()).is_err());
        assert!(
            serde_json::from_value::<LayerProduct>(serde_json::to_value(bad).unwrap()).is_err()
        );
        let now = chrono::Utc::now();
        let id = MapCompositionId::new();
        let revision = MapCompositionRevision::new(MapCompositionRevisionValue {
            composition_revision_id: MapCompositionRevisionId::new(),
            composition_id: id.clone(),
            revision: 1,
            layers: vec![],
            view: CompositionView {
                center: Wgs84Position::new(0., 0., None).unwrap(),
                zoom: 1.,
                bearing_deg: 0.,
                pitch_deg: 0.,
            },
            created_by: veoveo_types::PrincipalId::parse("author").unwrap(),
            created_at: now,
        })
        .unwrap();
        let mut composition = MapCompositionValue {
            composition_id: id,
            title: "Map".into(),
            current: revision,
            owner: veoveo_types::AccessSubject::Principal(
                veoveo_types::PrincipalId::parse("author").unwrap(),
            ),
            created_by: veoveo_types::PrincipalId::parse("author").unwrap(),
            work_context: veoveo_types::WorkContextId::parse("work").unwrap(),
            classification: Default::default(),
            data_labels: Default::default(),
            archived_at: None,
            created_at: now,
            updated_at: now,
        };
        assert!(MapComposition::new(composition.clone()).is_ok());
        composition.composition_id = MapCompositionId::new();
        assert!(MapComposition::new(composition.clone()).is_err());
        assert!(
            serde_json::from_value::<MapComposition>(serde_json::to_value(composition).unwrap())
                .is_err()
        );
    }
}
