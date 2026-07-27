use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use anyhow::Context as _;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64_STANDARD};
use chrono::{DateTime, Utc};
use glam::{DMat4, DQuat, DVec3};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};
use veoveo_artifact_client::HttpArtifactPlane;
use veoveo_mcp_contract::{ArtifactPlane, PlaneCaller};

use crate::{
    contract::{
        CreateSceneCompositionRequest, GLB_MIME_TYPE, GovernedSceneInput,
        MAX_COMPOSITION_ARTIFACT_BYTES, MAX_OVERLAY_ARTIFACT_BYTES, OVERLAY_ARTIFACT_MIME_TYPE,
        OverlayColor, SCENE_COMPOSITION_ALGORITHM_REVISION, SceneComposition,
        SceneCompositionAuthority, SceneInputId, SceneOverlay, SceneOverlayGeometry,
        SceneOverlayGeometrySource, ScenePosition, Sha256Digest, validate_artifact_geometry,
    },
    decode::{CpuMaterial, CpuPrimitive, CpuSampler, CpuTileContent, decode_glb},
    geodesy::{camera_ecef_basis, camera_world_transform, geodetic_to_ecef, world_from_ecef},
    renderer::RenderTile,
    uris,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSceneComposition {
    pub record: SceneComposition,
    pub resolved_overlays: Vec<ResolvedSceneOverlay>,
    pub artifact_bytes: BTreeMap<SceneInputId, ResolvedArtifactBytes>,
}

#[derive(Debug, Clone)]
pub struct ResolvedArtifactBytes(Vec<u8>);

impl ResolvedArtifactBytes {
    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl Serialize for ResolvedArtifactBytes {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&BASE64_STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for ResolvedArtifactBytes {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        let bytes = BASE64_STANDARD.decode(encoded).map_err(D::Error::custom)?;
        if bytes.len() as u64 > MAX_OVERLAY_ARTIFACT_BYTES {
            return Err(D::Error::custom(
                "resolved overlay artifact exceeds its byte limit",
            ));
        }
        Ok(Self(bytes))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSceneOverlay {
    pub overlay: SceneOverlay,
    pub geometry: SceneOverlayGeometry,
}

pub async fn resolve_scene_composition(
    mut request: CreateSceneCompositionRequest,
    authority: SceneCompositionAuthority,
    created_at: DateTime<Utc>,
    artifacts: &HttpArtifactPlane,
    caller: &PlaneCaller,
) -> anyhow::Result<ResolvedSceneComposition> {
    request.validate()?;
    request
        .governed_inputs
        .sort_by(|left, right| left.input_id.cmp(&right.input_id));
    let input_by_id = request
        .governed_inputs
        .iter()
        .map(|input| (&input.input_id, input))
        .collect::<BTreeMap<_, _>>();
    let mut artifact_bytes = BTreeMap::new();
    let mut resolved_overlays = Vec::with_capacity(request.overlays.len());
    for overlay in &request.overlays {
        let geometry = match &overlay.geometry {
            SceneOverlayGeometrySource::Inline { geometry } => geometry.clone(),
            SceneOverlayGeometrySource::Artifact { input_id } => {
                let input = input_by_id
                    .get(input_id)
                    .context("validated overlay geometry input is absent")?;
                let bytes = resolve_artifact_input(
                    input,
                    OVERLAY_ARTIFACT_MIME_TYPE,
                    artifacts,
                    caller,
                    &mut artifact_bytes,
                )
                .await?;
                let geometry: SceneOverlayGeometry = serde_json::from_slice(&bytes)
                    .context("decoding governed overlay geometry artifact")?;
                validate_artifact_geometry(&geometry, &input_by_id)?;
                geometry
            }
        };
        if let SceneOverlayGeometry::OrientedMeshInstance { mesh_input_id, .. } = &geometry {
            let input = input_by_id
                .get(mesh_input_id)
                .context("validated mesh input is absent")?;
            let bytes = resolve_artifact_input(
                input,
                GLB_MIME_TYPE,
                artifacts,
                caller,
                &mut artifact_bytes,
            )
            .await?;
            decode_glb(&bytes).context("validating governed GLB overlay")?;
        }
        resolved_overlays.push(ResolvedSceneOverlay {
            overlay: overlay.clone(),
            geometry,
        });
    }

    let request_digest = digest_json(&request)?;
    let authority_digest = digest_json(&authority)?;
    let stable_key = format!(
        "{}:{}:{}",
        SCENE_COMPOSITION_ALGORITHM_REVISION, authority_digest, request_digest
    );
    let composition_id =
        crate::contract::SceneCompositionId::from_stable_key(stable_key.as_bytes());
    let mut record = SceneComposition {
        schema_version: request.schema_version,
        composition_uri: uris::composition(&composition_id),
        composition_id,
        revision: 1,
        base_layer: request.base_layer,
        map_releases: request.map_releases,
        local_frame: request.local_frame,
        style_id: request.style_id,
        governed_inputs: request.governed_inputs,
        overlays: request.overlays,
        algorithm_revision: SCENE_COMPOSITION_ALGORITHM_REVISION.to_owned(),
        request_digest_sha256: request_digest,
        composition_digest_sha256: Sha256Digest::parse("0".repeat(64))
            .expect("zero digest is structurally valid"),
        authority,
        created_at,
    };
    record.composition_digest_sha256 = composition_digest(&record)?;
    Ok(ResolvedSceneComposition {
        record,
        resolved_overlays,
        artifact_bytes,
    })
}

async fn resolve_artifact_input(
    input: &GovernedSceneInput,
    expected_media_type: &str,
    artifacts: &HttpArtifactPlane,
    caller: &PlaneCaller,
    cache: &mut BTreeMap<SceneInputId, ResolvedArtifactBytes>,
) -> anyhow::Result<Vec<u8>> {
    if let Some(bytes) = cache.get(&input.input_id) {
        return Ok(bytes.0.clone());
    }
    let object = artifacts
        .resolve(caller, input.resource_uri.as_str())
        .await
        .map_err(|error| anyhow::anyhow!("artifact resolution failed: {error}"))?;
    anyhow::ensure!(
        object.bytes.len() as u64 <= MAX_OVERLAY_ARTIFACT_BYTES,
        "overlay artifact exceeds the {} byte limit",
        MAX_OVERLAY_ARTIFACT_BYTES
    );
    let retained_bytes = cache
        .values()
        .map(|bytes| bytes.0.len() as u64)
        .sum::<u64>()
        .saturating_add(object.bytes.len() as u64);
    anyhow::ensure!(
        retained_bytes <= MAX_COMPOSITION_ARTIFACT_BYTES,
        "scene composition artifacts exceed the {} byte aggregate limit",
        MAX_COMPOSITION_ARTIFACT_BYTES
    );
    anyhow::ensure!(
        object.metadata.mime_type.as_deref() == Some(expected_media_type),
        "overlay artifact media type must be {expected_media_type}"
    );
    anyhow::ensure!(
        input.media_type.as_deref() == Some(expected_media_type),
        "governed input media type must be {expected_media_type}"
    );
    let actual_digest = Sha256Digest::from_bytes(&object.bytes);
    anyhow::ensure!(
        actual_digest == input.digest_sha256,
        "overlay artifact digest does not match governed input"
    );
    cache.insert(
        input.input_id.clone(),
        ResolvedArtifactBytes(object.bytes.clone()),
    );
    Ok(object.bytes)
}

fn composition_digest(record: &SceneComposition) -> anyhow::Result<Sha256Digest> {
    let mut value = serde_json::to_value(record)?;
    let object = value
        .as_object_mut()
        .context("scene composition did not serialize as an object")?;
    object.remove("composition_digest_sha256");
    object.remove("created_at");
    digest_json(&value)
}

fn digest_json(value: &impl Serialize) -> anyhow::Result<Sha256Digest> {
    Ok(Sha256Digest::from_bytes(&serde_json::to_vec(value)?))
}

pub fn composition_render_tiles(
    composition: &ResolvedSceneComposition,
    scene_time: DateTime<Utc>,
    camera: &crate::contract::GeodeticCameraPose,
) -> anyhow::Result<Vec<RenderTile>> {
    let camera_ecef = geodetic_to_ecef(camera.position);
    composition
        .resolved_overlays
        .iter()
        .filter(|resolved| overlay_is_visible(resolved, composition, scene_time, camera_ecef))
        .map(|resolved| render_overlay(composition, resolved, camera))
        .collect()
}

fn overlay_is_visible(
    resolved: &ResolvedSceneOverlay,
    composition: &ResolvedSceneComposition,
    scene_time: DateTime<Utc>,
    camera_ecef: DVec3,
) -> bool {
    if !resolved.overlay.visibility.visible
        || resolved
            .overlay
            .validity
            .as_ref()
            .is_some_and(|validity| !validity.contains(scene_time))
    {
        return false;
    }
    let Some(position) = resolved.geometry.positions().next() else {
        return false;
    };
    let Ok(anchor) = position_ecef(*position, composition) else {
        return false;
    };
    let distance = camera_ecef.distance(anchor);
    resolved
        .overlay
        .visibility
        .minimum_camera_distance_meters
        .is_none_or(|minimum| distance >= minimum)
        && resolved
            .overlay
            .visibility
            .maximum_camera_distance_meters
            .is_none_or(|maximum| distance <= maximum)
}

fn render_overlay(
    composition: &ResolvedSceneComposition,
    resolved: &ResolvedSceneOverlay,
    camera: &crate::contract::GeodeticCameraPose,
) -> anyhow::Result<RenderTile> {
    let (content, ecef_from_content) = match &resolved.geometry {
        SceneOverlayGeometry::Marker { position } => marker_content(
            position_ecef(*position, composition)?,
            &resolved.overlay.style,
        ),
        SceneOverlayGeometry::Polyline {
            positions, closed, ..
        } => polyline_content(positions, *closed, composition, &resolved.overlay.style)?,
        SceneOverlayGeometry::Polygon {
            positions,
            triangle_indices,
        } => polygon_content(
            positions,
            triangle_indices,
            composition,
            &resolved.overlay.style,
        )?,
        SceneOverlayGeometry::OrientedMeshInstance {
            position,
            orientation,
            scale,
            mesh_input_id,
        } => {
            let bytes = composition
                .artifact_bytes
                .get(mesh_input_id)
                .context("resolved mesh artifact bytes are absent")?;
            let content = decode_glb(bytes.as_slice()).context("decoding governed GLB overlay")?;
            let transform = mesh_transform(*position, *orientation, *scale, composition)?;
            (content, transform)
        }
        SceneOverlayGeometry::Label { position, text } => label_content(
            position_ecef(*position, composition)?,
            text,
            camera,
            &resolved.overlay.style,
        ),
    };
    let cache_material = serde_json::to_vec(&(
        &composition.record.composition_digest_sha256,
        &resolved.overlay.overlay_id,
        &resolved.overlay.style,
        camera,
    ))?;
    Ok(RenderTile {
        cache_key: format!("overlay:{}", Sha256Digest::from_bytes(&cache_material)),
        ecef_from_content,
        content: Arc::new(content),
    })
}

fn position_ecef(
    position: ScenePosition,
    composition: &ResolvedSceneComposition,
) -> anyhow::Result<DVec3> {
    match position {
        ScenePosition::Wgs84 { position } => Ok(geodetic_to_ecef(position)),
        ScenePosition::LocalMeters { xyz_meters } => {
            let binding = composition
                .record
                .local_frame
                .as_ref()
                .context("validated local frame binding is absent")?;
            let transform = DMat4::from_cols_array(&binding.ecef_from_frame);
            Ok(transform.transform_point3(DVec3::from_array(xyz_meters)))
        }
    }
}

fn marker_content(
    anchor: DVec3,
    style: &crate::contract::SceneOverlayStyle,
) -> (CpuTileContent, DMat4) {
    let radius = style.marker_size_meters as f32 / 2.0;
    let positions = vec![
        [radius, 0.0, 0.0],
        [-radius, 0.0, 0.0],
        [0.0, radius, 0.0],
        [0.0, -radius, 0.0],
        [0.0, 0.0, radius],
        [0.0, 0.0, -radius],
    ];
    let indices = vec![
        0, 2, 4, 2, 1, 4, 1, 3, 4, 3, 0, 4, 2, 0, 5, 1, 2, 5, 3, 1, 5, 0, 3, 5,
    ];
    let content = content_from_mesh(
        positions,
        indices,
        style.fill_color.unwrap_or(style.stroke_color),
        true,
    );
    (content, DMat4::from_translation(anchor))
}

fn polyline_content(
    positions: &[ScenePosition],
    closed: bool,
    composition: &ResolvedSceneComposition,
    style: &crate::contract::SceneOverlayStyle,
) -> anyhow::Result<(CpuTileContent, DMat4)> {
    let points = positions
        .iter()
        .map(|position| position_ecef(*position, composition))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let anchor = points[0];
    let segment_count = if closed {
        points.len()
    } else {
        points.len() - 1
    };
    let mut vertices = Vec::with_capacity(segment_count * 4);
    let mut indices = Vec::with_capacity(segment_count * 6);
    for index in 0..segment_count {
        let left = points[index];
        let right = points[(index + 1) % points.len()];
        let direction = (right - left).normalize();
        let radial = (left + right).normalize();
        let side = direction.cross(radial).normalize_or(DVec3::X) * (style.line_width_meters / 2.0);
        let base = vertices.len() as u32;
        for point in [left + side, left - side, right - side, right + side] {
            vertices.push((point - anchor).as_vec3().to_array());
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Ok((
        content_from_mesh(vertices, indices, style.stroke_color, true),
        DMat4::from_translation(anchor),
    ))
}

fn polygon_content(
    positions: &[ScenePosition],
    triangle_indices: &[u32],
    composition: &ResolvedSceneComposition,
    style: &crate::contract::SceneOverlayStyle,
) -> anyhow::Result<(CpuTileContent, DMat4)> {
    let points = positions
        .iter()
        .map(|position| position_ecef(*position, composition))
        .collect::<anyhow::Result<Vec<_>>>()?;
    let anchor = points[0];
    let vertices = points
        .into_iter()
        .map(|point| (point - anchor).as_vec3().to_array())
        .collect();
    Ok((
        content_from_mesh(
            vertices,
            triangle_indices.to_vec(),
            style.fill_color.unwrap_or(style.stroke_color),
            true,
        ),
        DMat4::from_translation(anchor),
    ))
}

fn label_content(
    anchor: DVec3,
    text: &str,
    camera: &crate::contract::GeodeticCameraPose,
    style: &crate::contract::SceneOverlayStyle,
) -> (CpuTileContent, DMat4) {
    let (_, right, up) = camera_ecef_basis(camera);
    let height = style.label_height_meters;
    let pixel = height / 7.0;
    let advance = pixel * 6.0;
    let total_width = advance * text.chars().count() as f64;
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    for (character_index, character) in text.chars().enumerate() {
        let rows = glyph_rows(character);
        for (row, bits) in rows.iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) == 0 {
                    continue;
                }
                let x =
                    character_index as f64 * advance + column as f64 * pixel - total_width / 2.0;
                let y = (6 - row) as f64 * pixel;
                let origin = right * x + up * y;
                let corners = [
                    origin,
                    origin + right * pixel,
                    origin + right * pixel + up * pixel,
                    origin + up * pixel,
                ];
                let base = positions.len() as u32;
                positions.extend(
                    corners
                        .into_iter()
                        .map(|corner| corner.as_vec3().to_array()),
                );
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
    }
    (
        content_from_mesh(positions, indices, style.stroke_color, true),
        DMat4::from_translation(anchor),
    )
}

fn glyph_rows(character: char) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        'C' => [0x0f, 0x10, 0x10, 0x10, 0x10, 0x10, 0x0f],
        'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        'G' => [0x0f, 0x10, 0x10, 0x17, 0x11, 0x11, 0x0f],
        'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        'J' => [0x07, 0x02, 0x02, 0x02, 0x12, 0x12, 0x0c],
        'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        '0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        '1' => [0x04, 0x0c, 0x14, 0x04, 0x04, 0x04, 0x1f],
        '2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        '3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        '4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        '5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        '6' => [0x0f, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        '7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        '9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x1e],
        ' ' => [0; 7],
        '-' => [0, 0, 0, 0x1f, 0, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0x0c, 0x0c],
        _ => [0x1f, 0x11, 0x01, 0x02, 0x04, 0, 0x04],
    }
}

fn mesh_transform(
    position: ScenePosition,
    orientation: crate::contract::HeadingPitchRoll,
    scale: [f64; 3],
    composition: &ResolvedSceneComposition,
) -> anyhow::Result<DMat4> {
    let local_rotation = view_local_orientation(orientation);
    let scale = DMat4::from_scale(DVec3::from_array(scale));
    match position {
        ScenePosition::Wgs84 { position } => {
            let pose = crate::contract::GeodeticCameraPose {
                position,
                orientation,
                vertical_fov_degrees: 45.0,
            };
            Ok(world_from_ecef(position).inverse()
                * camera_world_transform(&pose, position)
                * scale)
        }
        ScenePosition::LocalMeters { xyz_meters } => {
            let binding = composition
                .record
                .local_frame
                .as_ref()
                .context("validated local frame binding is absent")?;
            let ecef_from_frame = DMat4::from_cols_array(&binding.ecef_from_frame);
            Ok(ecef_from_frame
                * DMat4::from_translation(DVec3::from_array(xyz_meters))
                * local_rotation
                * scale)
        }
    }
}

fn view_local_orientation(orientation: crate::contract::HeadingPitchRoll) -> DMat4 {
    let heading = DQuat::from_axis_angle(DVec3::Y, -orientation.heading_degrees.to_radians());
    let pitch = DQuat::from_axis_angle(DVec3::X, orientation.pitch_degrees.to_radians());
    let roll = DQuat::from_axis_angle(DVec3::Z, -orientation.roll_degrees.to_radians());
    DMat4::from_quat(heading * pitch * roll)
}

fn content_from_mesh(
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    color: OverlayColor,
    double_sided: bool,
) -> CpuTileContent {
    let estimated_bytes = (positions.len() * std::mem::size_of::<[f32; 3]>()
        + indices.len() * std::mem::size_of::<u32>()) as u64;
    let normals = positions
        .iter()
        .map(|position| {
            let normal = glam::Vec3::from_array(*position).normalize_or(glam::Vec3::Y);
            normal.to_array()
        })
        .collect::<Vec<_>>();
    let colors = vec![color.as_array(); positions.len()];
    CpuTileContent {
        primitives: vec![CpuPrimitive {
            node_transform: DMat4::IDENTITY,
            texcoords: vec![[0.0, 0.0]; positions.len()],
            positions,
            normals,
            colors,
            indices,
            material: CpuMaterial {
                base_color: OverlayColor::WHITE.as_array(),
                base_color_texture: None,
                unlit: true,
                double_sided,
                alpha_blend: color.alpha < 1.0,
                sampler: CpuSampler::default(),
            },
        }],
        rtc_center_ecef: None,
        attribution: Vec::new(),
        estimated_bytes,
    }
}

pub fn composition_attribution(
    composition: &ResolvedSceneComposition,
    rendered_overlay_ids: &BTreeSet<crate::contract::SceneOverlayId>,
) -> BTreeSet<String> {
    let input_by_id = composition
        .record
        .governed_inputs
        .iter()
        .map(|input| (&input.input_id, input))
        .collect::<BTreeMap<_, _>>();
    composition
        .resolved_overlays
        .iter()
        .filter(|resolved| rendered_overlay_ids.contains(&resolved.overlay.overlay_id))
        .flat_map(|resolved| resolved.overlay.governed_input_ids.iter())
        .filter_map(|input_id| input_by_id.get(input_id))
        .map(|input| input.attribution.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{SceneOverlayStyle, SceneStyleId, Wgs84Position3d};

    #[test]
    fn marker_mesh_is_bounded_and_gpu_ready() {
        let style = SceneOverlayStyle::default();
        let (content, transform) = marker_content(DVec3::new(1.0, 2.0, 3.0), &style);
        assert_eq!(content.primitives.len(), 1);
        assert_eq!(content.primitives[0].indices.len(), 24);
        assert_eq!(transform.w_axis.truncate(), DVec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn label_glyphs_create_gpu_triangles() {
        let camera = crate::contract::GeodeticCameraPose {
            position: Wgs84Position3d {
                latitude_degrees: 0.0,
                longitude_degrees: 0.0,
                ellipsoidal_height_meters: 1_000.0,
            },
            orientation: crate::contract::HeadingPitchRoll {
                heading_degrees: 0.0,
                pitch_degrees: -45.0,
                roll_degrees: 0.0,
            },
            vertical_fov_degrees: 45.0,
        };
        let (content, _) = label_content(
            geodetic_to_ecef(camera.position),
            "A1",
            &camera,
            &SceneOverlayStyle::default(),
        );
        assert!(!content.primitives[0].positions.is_empty());
        assert_eq!(content.primitives[0].indices.len() % 6, 0);
    }

    #[test]
    fn style_ids_remain_controlled() {
        assert!(SceneStyleId::new("operations:1").is_ok());
        assert!(SceneStyleId::new("operations/1").is_err());
    }

    #[test]
    fn recoverable_artifact_snapshots_use_bounded_base64() {
        let bytes = ResolvedArtifactBytes(vec![0, 1, 2, 253, 254, 255]);
        let value = serde_json::to_value(&bytes).unwrap();
        assert!(
            value.is_string(),
            "task snapshots must not expand bytes into arrays"
        );
        let recovered: ResolvedArtifactBytes = serde_json::from_value(value).unwrap();
        assert_eq!(recovered.as_slice(), bytes.as_slice());
    }
}
