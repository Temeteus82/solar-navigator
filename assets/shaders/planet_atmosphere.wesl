import bevy_pbr::render::forward_io::VertexOutput;
import bevy_pbr::render::mesh_view_bindings::view;

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> tint: vec4<f32>;
// x = density, y = rim power, z = forward phase power, w = brightness
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var<uniform> params: vec4<f32>;
// xyz = the Sun's render-space position (floating origin: not the world origin)
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var<uniform> sun_position: vec4<f32>;
// x = -dot(normal, view) on the ray that grazes the planet's limb
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var<uniform> shell: vec4<f32>;

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len2 = max(dot(v, v), 1e-6);
    return v * inverseSqrt(len2);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world_position = in.world_position.xyz;
    let normal = safe_normalize(in.world_normal);

    let view_dir = safe_normalize(view.world_position - world_position);
    let light_dir = safe_normalize(sun_position.xyz - world_position);

    let n_dot_l = max(dot(normal, light_dir), 0.0);
    // The shell is drawn inside-out (front faces culled), so every visible
    // normal points away from the viewer. `depth` runs from 0 on the shell's
    // silhouette to 1 on the ray grazing the planet's limb, so the glow hugs
    // the limb and fades to nothing at the outer edge.
    let depth = clamp(-dot(normal, view_dir) / shell.x, 0.0, 1.0);
    let rim = pow(depth, params.y);
    let forward_phase = pow(max(dot(view_dir, light_dir), 0.0), params.z);

    let scatter = rim * (0.25 + 0.75 * n_dot_l) + forward_phase * 0.55;
    let alpha = clamp(scatter * params.x, 0.0, 1.0);
    let color = tint.rgb * scatter * params.w;

    return vec4(color, alpha);
}
