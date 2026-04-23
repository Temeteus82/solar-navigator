#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> tint: vec4<f32>;
// x = density, y = rim power, z = forward phase power, w = brightness
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len2 = max(dot(v, v), 1e-6);
    return v * inverseSqrt(len2);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world_position = in.world_position.xyz;
    let normal = safe_normalize(in.world_normal);

    let view_dir = safe_normalize(view.world_position - world_position);
    // Sun is fixed at world origin in this scene.
    let light_dir = safe_normalize(-world_position);

    let n_dot_l = max(dot(normal, light_dir), 0.0);
    let rim = pow(clamp(1.0 - dot(normal, view_dir), 0.0, 1.0), params.y);
    let forward_phase = pow(max(dot(view_dir, light_dir), 0.0), params.z);

    let scatter = rim * (0.25 + 0.75 * n_dot_l) + forward_phase * 0.55;
    let alpha = clamp(scatter * params.x, 0.0, 1.0);
    let color = tint.rgb * scatter * params.w;

    return vec4(color, alpha);
}
