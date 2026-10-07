// An unorm view reads stored sRGB bytes without applying the sampling transfer function.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> rgb: array<u32>;

@compute @workgroup_size(64)
fn pack(@builtin(global_invocation_id) id: vec3<u32>) {
    let dimensions = textureDimensions(source);
    let length = dimensions.x * dimensions.y * 3u;
    let word_index = id.y * 16384u * 64u + id.x;
    let first = word_index * 4u;
    if first >= length { return; }
    var word = 0u;
    for (var lane = 0u; lane < 4u; lane += 1u) {
        let offset = first + lane;
        if offset < length {
            let pixel = offset / 3u;
            let position = vec2<i32>(i32(pixel % dimensions.x), i32(pixel / dimensions.x));
            let channels = textureLoad(source, position, 0);
            let byte = u32(round(clamp(channels[offset % 3u], 0.0, 1.0) * 255.0));
            word |= byte << (lane * 8u);
        }
    }
    rgb[word_index] = word;
}
