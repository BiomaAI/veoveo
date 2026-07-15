use std::{collections::HashMap, sync::Arc};

use glam::{DMat4, DVec3};
use gltf::{mesh::Mode, texture::WrappingMode};

#[derive(Debug, Clone)]
pub struct CpuTileContent {
    pub primitives: Vec<CpuPrimitive>,
    pub rtc_center_ecef: Option<DVec3>,
    pub attribution: Vec<String>,
    pub estimated_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct CpuPrimitive {
    pub node_transform: DMat4,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub texcoords: Vec<[f32; 2]>,
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material: CpuMaterial,
}

#[derive(Debug, Clone)]
pub struct CpuMaterial {
    pub base_color: [f32; 4],
    pub base_color_texture: Option<Arc<CpuImage>>,
    pub unlit: bool,
    pub double_sided: bool,
    pub alpha_blend: bool,
    pub sampler: CpuSampler,
}

#[derive(Debug, Clone)]
pub struct CpuImage {
    pub width: u32,
    pub height: u32,
    pub rgba8: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub struct CpuSampler {
    pub wrap_u: CpuWrapMode,
    pub wrap_v: CpuWrapMode,
}

impl Default for CpuSampler {
    fn default() -> Self {
        Self {
            wrap_u: CpuWrapMode::Repeat,
            wrap_v: CpuWrapMode::Repeat,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum CpuWrapMode {
    ClampToEdge,
    MirroredRepeat,
    Repeat,
}

pub fn decode_glb(bytes: &[u8]) -> Result<CpuTileContent, DecodeError> {
    if let Some(kind) = legacy_content_kind(bytes) {
        return Err(DecodeError::LegacyContentUnsupported(kind));
    }
    let prepared = prepare_glb(bytes)?;
    let gltf::Gltf { document, blob } = gltf::Gltf::from_slice_without_validation(&prepared.bytes)?;
    let buffer_data = gltf::import_buffers(&document, None, blob)?;
    let image_data = gltf::import_images(&document, None, &buffer_data)?;
    let mut import = draco_gltf::Import {
        document,
        buffers: buffer_data.into_iter().map(|data| data.0).collect(),
        images: image_data,
    };
    import.decompress_in_place()?;

    let image_cache = import
        .images
        .iter()
        .enumerate()
        .map(|(index, image)| Ok((index, Arc::new(convert_image(image)?))))
        .collect::<Result<HashMap<_, _>, DecodeError>>()?;

    let mut primitives = Vec::new();
    let scenes: Vec<_> = import.document.scenes().collect();
    for scene in scenes {
        for node in scene.nodes() {
            decode_node(
                &import,
                &image_cache,
                node,
                DMat4::IDENTITY,
                &mut primitives,
            )?;
        }
    }

    if primitives.is_empty() {
        return Err(DecodeError::NoTrianglePrimitives);
    }

    let estimated_bytes = primitives
        .iter()
        .map(|primitive| {
            (primitive.positions.len() * std::mem::size_of::<[f32; 3]>()
                + primitive.normals.len() * std::mem::size_of::<[f32; 3]>()
                + primitive.texcoords.len() * std::mem::size_of::<[f32; 2]>()
                + primitive.colors.len() * std::mem::size_of::<[f32; 4]>()
                + primitive.indices.len() * std::mem::size_of::<u32>()) as u64
        })
        .sum::<u64>()
        + image_cache
            .values()
            .map(|image| image.rgba8.len() as u64)
            .sum::<u64>();

    Ok(CpuTileContent {
        primitives,
        rtc_center_ecef: prepared.rtc_center_ecef,
        attribution: prepared.attribution,
        estimated_bytes,
    })
}

fn legacy_content_kind(bytes: &[u8]) -> Option<&'static str> {
    ["b3dm", "i3dm", "pnts"]
        .into_iter()
        .find(|kind| bytes.starts_with(kind.as_bytes()))
}

fn decode_node(
    import: &draco_gltf::Import,
    images: &HashMap<usize, Arc<CpuImage>>,
    node: gltf::Node<'_>,
    parent_transform: DMat4,
    output: &mut Vec<CpuPrimitive>,
) -> Result<(), DecodeError> {
    let matrix = node.transform().matrix();
    let local = DMat4::from_cols_array(&[
        f64::from(matrix[0][0]),
        f64::from(matrix[0][1]),
        f64::from(matrix[0][2]),
        f64::from(matrix[0][3]),
        f64::from(matrix[1][0]),
        f64::from(matrix[1][1]),
        f64::from(matrix[1][2]),
        f64::from(matrix[1][3]),
        f64::from(matrix[2][0]),
        f64::from(matrix[2][1]),
        f64::from(matrix[2][2]),
        f64::from(matrix[2][3]),
        f64::from(matrix[3][0]),
        f64::from(matrix[3][1]),
        f64::from(matrix[3][2]),
        f64::from(matrix[3][3]),
    ]);
    let transform = parent_transform * local;

    if let Some(mesh) = node.mesh() {
        for primitive in mesh.primitives() {
            if primitive.mode() != Mode::Triangles {
                continue;
            }
            let reader =
                primitive.reader(|buffer| import.buffers.get(buffer.index()).map(Vec::as_slice));
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .ok_or(DecodeError::MissingPositions)?
                .collect();
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|indices| indices.into_u32().collect::<Vec<_>>())
                .unwrap_or_else(|| (0..positions.len() as u32).collect::<Vec<_>>());
            let mut normals: Vec<[f32; 3]> = reader
                .read_normals()
                .map(Iterator::collect)
                .unwrap_or_default();
            if normals.len() != positions.len() {
                normals = calculate_normals(&positions, &indices);
            }
            let texcoords = reader
                .read_tex_coords(0)
                .map(|coordinates| coordinates.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0, 0.0]; positions.len()]);
            let colors = reader
                .read_colors(0)
                .map(|colors| colors.into_rgba_f32().collect())
                .unwrap_or_else(|| vec![[1.0; 4]; positions.len()]);
            let material = decode_material(primitive.material(), images);
            output.push(CpuPrimitive {
                node_transform: transform,
                positions,
                normals,
                texcoords,
                colors,
                indices,
                material,
            });
        }
    }

    for child in node.children() {
        decode_node(import, images, child, transform, output)?;
    }
    Ok(())
}

fn decode_material(
    material: gltf::Material<'_>,
    images: &HashMap<usize, Arc<CpuImage>>,
) -> CpuMaterial {
    let pbr = material.pbr_metallic_roughness();
    let texture = pbr.base_color_texture();
    let sampler = texture
        .as_ref()
        .map(|info| CpuSampler {
            wrap_u: convert_wrap(info.texture().sampler().wrap_s()),
            wrap_v: convert_wrap(info.texture().sampler().wrap_t()),
        })
        .unwrap_or_default();
    let base_color_texture =
        texture.and_then(|info| images.get(&info.texture().source().index()).map(Arc::clone));
    CpuMaterial {
        base_color: pbr.base_color_factor(),
        base_color_texture,
        unlit: material.unlit(),
        double_sided: material.double_sided(),
        alpha_blend: !matches!(material.alpha_mode(), gltf::material::AlphaMode::Opaque),
        sampler,
    }
}

fn convert_wrap(mode: WrappingMode) -> CpuWrapMode {
    match mode {
        WrappingMode::ClampToEdge => CpuWrapMode::ClampToEdge,
        WrappingMode::MirroredRepeat => CpuWrapMode::MirroredRepeat,
        WrappingMode::Repeat => CpuWrapMode::Repeat,
    }
}

fn convert_image(image: &gltf::image::Data) -> Result<CpuImage, DecodeError> {
    use gltf::image::Format;

    let pixel_count = (image.width as usize)
        .checked_mul(image.height as usize)
        .ok_or(DecodeError::ImageTooLarge)?;
    let mut rgba8 = Vec::with_capacity(pixel_count.saturating_mul(4));
    match image.format {
        Format::R8 => {
            for &r in &image.pixels {
                rgba8.extend_from_slice(&[r, r, r, 255]);
            }
        }
        Format::R8G8 => {
            for pixel in image.pixels.chunks_exact(2) {
                rgba8.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        Format::R8G8B8 => {
            for pixel in image.pixels.chunks_exact(3) {
                rgba8.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        Format::R8G8B8A8 => rgba8.extend_from_slice(&image.pixels),
        other => return Err(DecodeError::UnsupportedImageFormat(format!("{other:?}"))),
    }
    Ok(CpuImage {
        width: image.width,
        height: image.height,
        rgba8,
    })
}

fn calculate_normals(positions: &[[f32; 3]], indices: &[u32]) -> Vec<[f32; 3]> {
    let mut normals = vec![glam::Vec3::ZERO; positions.len()];
    for triangle in indices.chunks_exact(3) {
        let [a, b, c] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        if a >= positions.len() || b >= positions.len() || c >= positions.len() {
            continue;
        }
        let pa = glam::Vec3::from_array(positions[a]);
        let pb = glam::Vec3::from_array(positions[b]);
        let pc = glam::Vec3::from_array(positions[c]);
        let normal = (pb - pa).cross(pc - pa);
        normals[a] += normal;
        normals[b] += normal;
        normals[c] += normal;
    }
    normals
        .into_iter()
        .map(|normal| normal.normalize_or(glam::Vec3::Y).to_array())
        .collect()
}

struct PreparedGlb {
    bytes: Vec<u8>,
    rtc_center_ecef: Option<DVec3>,
    attribution: Vec<String>,
}

fn prepare_glb(bytes: &[u8]) -> Result<PreparedGlb, DecodeError> {
    let (mut json, binary) = split_glb(bytes)?;
    let attribution = json["asset"]["copyright"]
        .as_str()
        .map(|value| {
            value
                .split(';')
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut rtc_center = json["extensions"]["CESIUM_RTC"]["center"]
        .as_array()
        .and_then(|values| {
            let values: Vec<f64> = values
                .iter()
                .filter_map(serde_json::Value::as_f64)
                .collect();
            (values.len() == 3).then(|| DVec3::new(values[0], values[1], values[2]))
        });
    if let Some(extensions) = json
        .get_mut("extensions")
        .and_then(serde_json::Value::as_object_mut)
    {
        extensions.remove("CESIUM_RTC");
    }
    remove_extension_name(&mut json, "CESIUM_RTC");

    if rtc_center.is_none() {
        rtc_center = extract_planetary_root_translation(&mut json);
    }
    let bytes = rebuild_glb(&json, binary)?;
    Ok(PreparedGlb {
        bytes,
        rtc_center_ecef: rtc_center,
        attribution,
    })
}

fn remove_extension_name(json: &mut serde_json::Value, name: &str) {
    for key in ["extensionsUsed", "extensionsRequired"] {
        if let Some(values) = json.get_mut(key).and_then(serde_json::Value::as_array_mut) {
            values.retain(|value| value.as_str() != Some(name));
        }
    }
}

fn extract_planetary_root_translation(json: &mut serde_json::Value) -> Option<DVec3> {
    const PLANETARY_TRANSLATION_M: f64 = 2_000_000.0;
    let root_indices = scene_root_indices(json);
    let nodes = json.get_mut("nodes")?.as_array_mut()?;
    let center = root_indices.iter().find_map(|&index| {
        let node = nodes.get(index)?;
        node_translation(node).filter(|translation| translation.length() > PLANETARY_TRANSLATION_M)
    })?;
    for index in root_indices {
        if let Some(node) = nodes.get_mut(index) {
            subtract_node_translation(node, center);
        }
    }
    Some(center)
}

fn scene_root_indices(json: &serde_json::Value) -> Vec<usize> {
    let scene_index = json
        .get("scene")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0) as usize;
    json.get("scenes")
        .and_then(serde_json::Value::as_array)
        .and_then(|scenes| scenes.get(scene_index))
        .and_then(|scene| scene.get("nodes"))
        .and_then(serde_json::Value::as_array)
        .map(|nodes| {
            nodes
                .iter()
                .filter_map(serde_json::Value::as_u64)
                .map(|index| index as usize)
                .collect()
        })
        .unwrap_or_default()
}

fn node_translation(node: &serde_json::Value) -> Option<DVec3> {
    if let Some(matrix) = node.get("matrix").and_then(serde_json::Value::as_array) {
        return Some(DVec3::new(
            matrix.get(12)?.as_f64()?,
            matrix.get(13)?.as_f64()?,
            matrix.get(14)?.as_f64()?,
        ));
    }
    let translation = node.get("translation")?.as_array()?;
    Some(DVec3::new(
        translation.first()?.as_f64()?,
        translation.get(1)?.as_f64()?,
        translation.get(2)?.as_f64()?,
    ))
}

fn subtract_node_translation(node: &mut serde_json::Value, center: DVec3) {
    if let Some(matrix) = node
        .get_mut("matrix")
        .and_then(serde_json::Value::as_array_mut)
    {
        if matrix.len() >= 15 {
            matrix[12] = serde_json::Value::from(matrix[12].as_f64().unwrap_or(0.0) - center.x);
            matrix[13] = serde_json::Value::from(matrix[13].as_f64().unwrap_or(0.0) - center.y);
            matrix[14] = serde_json::Value::from(matrix[14].as_f64().unwrap_or(0.0) - center.z);
        }
    } else if let Some(translation) = node
        .get_mut("translation")
        .and_then(serde_json::Value::as_array_mut)
        && translation.len() >= 3
    {
        translation[0] = serde_json::Value::from(translation[0].as_f64().unwrap_or(0.0) - center.x);
        translation[1] = serde_json::Value::from(translation[1].as_f64().unwrap_or(0.0) - center.y);
        translation[2] = serde_json::Value::from(translation[2].as_f64().unwrap_or(0.0) - center.z);
    }
}

fn split_glb(bytes: &[u8]) -> Result<(serde_json::Value, &[u8]), DecodeError> {
    if bytes.len() < 20 || &bytes[..4] != b"glTF" {
        return Err(DecodeError::NotGlb);
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().expect("four bytes"));
    if version != 2 {
        return Err(DecodeError::UnsupportedGlbVersion(version));
    }
    let declared = u32::from_le_bytes(bytes[8..12].try_into().expect("four bytes")) as usize;
    if declared > bytes.len() {
        return Err(DecodeError::TruncatedGlb);
    }
    let mut offset = 12;
    let mut json = None;
    let mut binary = &[][..];
    while offset + 8 <= declared {
        let length =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("four bytes")) as usize;
        let kind = u32::from_le_bytes(
            bytes[offset + 4..offset + 8]
                .try_into()
                .expect("four bytes"),
        );
        offset += 8;
        let end = offset
            .checked_add(length)
            .ok_or(DecodeError::TruncatedGlb)?;
        if end > declared {
            return Err(DecodeError::TruncatedGlb);
        }
        match kind {
            0x4E4F534A => json = Some(serde_json::from_slice(&bytes[offset..end])?),
            0x004E4942 => binary = &bytes[offset..end],
            _ => {}
        }
        offset = end;
    }
    Ok((json.ok_or(DecodeError::MissingJsonChunk)?, binary))
}

fn rebuild_glb(json: &serde_json::Value, binary: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut json_bytes = serde_json::to_vec(json)?;
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let mut binary = binary.to_vec();
    while !binary.len().is_multiple_of(4) {
        binary.push(0);
    }
    let total = 12usize
        .checked_add(8 + json_bytes.len())
        .and_then(|value| {
            value.checked_add(if binary.is_empty() {
                0
            } else {
                8 + binary.len()
            })
        })
        .ok_or(DecodeError::ImageTooLarge)?;
    let mut output = Vec::with_capacity(total);
    output.extend_from_slice(b"glTF");
    output.extend_from_slice(&2u32.to_le_bytes());
    output.extend_from_slice(&(total as u32).to_le_bytes());
    output.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    output.extend_from_slice(&0x4E4F534Au32.to_le_bytes());
    output.extend_from_slice(&json_bytes);
    if !binary.is_empty() {
        output.extend_from_slice(&(binary.len() as u32).to_le_bytes());
        output.extend_from_slice(&0x004E4942u32.to_le_bytes());
        output.extend_from_slice(&binary);
    }
    Ok(output)
}

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("legacy 3D Tiles content `{0}` is unsupported; provide GLB content")]
    LegacyContentUnsupported(&'static str),
    #[error("content is not a GLB 2.0 file")]
    NotGlb,
    #[error("GLB version {0} is unsupported")]
    UnsupportedGlbVersion(u32),
    #[error("GLB is truncated")]
    TruncatedGlb,
    #[error("GLB JSON chunk is missing")]
    MissingJsonChunk,
    #[error("GLB JSON is invalid: {0}")]
    Json(#[from] serde_json::Error),
    #[error("glTF import failed: {0}")]
    Gltf(#[from] gltf::Error),
    #[error("Draco glTF decode failed: {0}")]
    Draco(#[from] draco_gltf::Error),
    #[error("triangle primitive has no positions")]
    MissingPositions,
    #[error("tile contains no triangle primitives")]
    NoTrianglePrimitives,
    #[error("decoded image dimensions overflow")]
    ImageTooLarge,
    #[error("decoded image format {0} is unsupported")]
    UnsupportedImageFormat(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glb_with_json(json: serde_json::Value) -> Vec<u8> {
        rebuild_glb(&json, &[]).unwrap()
    }

    #[test]
    fn preprocessing_extracts_planetary_root_translation_without_f32_loss() {
        let bytes = glb_with_json(serde_json::json!({
            "asset": {"version": "2.0", "copyright": "Google; Test Source"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"matrix": [1,0,0,0, 0,1,0,0, 0,0,1,0, 4000000.25,3000000.5,5000000.75,1]}]
        }));
        let prepared = prepare_glb(&bytes).unwrap();
        assert_eq!(
            prepared.rtc_center_ecef,
            Some(DVec3::new(4_000_000.25, 3_000_000.5, 5_000_000.75))
        );
        assert_eq!(prepared.attribution, vec!["Google", "Test Source"]);
        let (json, _) = split_glb(&prepared.bytes).unwrap();
        assert_eq!(json["nodes"][0]["matrix"][12], 0.0);
    }

    #[test]
    fn malformed_bytes_fail_as_not_glb() {
        assert!(matches!(decode_glb(b"nope"), Err(DecodeError::NotGlb)));
    }

    #[test]
    fn legacy_container_has_typed_failure() {
        assert!(matches!(
            decode_glb(b"b3dm"),
            Err(DecodeError::LegacyContentUnsupported("b3dm"))
        ));
    }
}
