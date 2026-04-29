#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> tint: vec4<f32>;
// x = inner_radius (scene units, informational)
// y = outer_radius (scene units, informational)
// z = planet_radius (scene units — drives the umbra width)
// w = ring_brightness (overall multiplier on the lit term)
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var<uniform> params: vec4<f32>;
// x = forward scatter strength (sun behind ring, viewer in front)
// y = back scatter / opposition surge (sun behind viewer)
// z = specular strength
// w = ambient floor
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var<uniform> lighting: vec4<f32>;
// xyz = parent planet world-space position (sun is fixed at world origin)
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var<uniform> planet_position: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var color_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(5) var color_sampler: sampler;

fn safe_normalize(v: vec3<f32>) -> vec3<f32> {
    let len2 = max(dot(v, v), 1e-6);
    return v * inverseSqrt(len2);
}

// Sigmoid-style soft step that gives the lit/unlit transition a sharper but
// still smooth edge than a raw `max(dot, 0)`. `k` controls the sharpness.
fn sigmoid_terminator(x: f32, k: f32) -> f32 {
    return 1.0 / (1.0 + exp(-k * x));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let world_pos = in.world_position.xyz;
    // Ring mesh normals point along +Y in local space; the ring is two-sided,
    // so use the absolute dot-product for lighting and pick whichever face
    // is currently pointing toward the sun.
    let raw_normal = safe_normalize(in.world_normal);

    // Sun is fixed at the world origin (see app::setup::setup_scene).
    let to_sun_vec = -world_pos;
    let to_sun = safe_normalize(to_sun_vec);
    let to_view = safe_normalize(view.world_position - world_pos);

    // Sample the color/alpha strip radially. ring_mesh emits UV.y = 0 at the
    // inner edge and 1 at the outer edge; the standard solarsystemscope ring
    // texture is a 1-D radial strip, so collapse to vec2(uv.y, 0.5).
    let radial_uv = vec2<f32>(in.uv.y, 0.5);
    let tex_sample = textureSample(color_texture, color_sampler, radial_uv);
    let base_color = tex_sample.rgb * tint.rgb;
    let base_alpha = tex_sample.a * tint.a;

    // ---- Eclipse: cylindrical shadow cast by the planet onto the ring ----
    //
    // The sun is a point at the origin. The fragment lies on the ray
    //     r(t) = t * dir,  dir = world_pos / |world_pos|
    // at parameter `frag_dist`. The planet is in shadow of the fragment iff
    // the planet centre is close to that ray AND lies between the sun and
    // the fragment.
    let frag_dist = length(world_pos);
    let frag_dir = world_pos / max(frag_dist, 1e-6);
    let t_closest = dot(planet_position.xyz, frag_dir);
    let closest_point = frag_dir * t_closest;
    let perp_dist = length(planet_position.xyz - closest_point);

    let in_shadow_range = step(0.0, t_closest) * step(t_closest, frag_dist);
    let umbra_radius = max(params.z, 1e-4);
    // Soft penumbra band: full shadow inside ~85% of the umbra, fading to
    // none at ~120% of it.
    let shadow_intensity = in_shadow_range
        * (1.0 - smoothstep(umbra_radius * 0.85, umbra_radius * 1.20, perp_dist));
    let illumination = 1.0 - shadow_intensity;

    // ---- Lambertian term with sigmoid-sharpened terminator ----
    let n_dot_l_signed = dot(raw_normal, to_sun);
    let n_dot_l = abs(n_dot_l_signed);
    // Soft transition centred around n_dot_l ~ 0.05 so the unlit face fades in.
    let sharpened = sigmoid_terminator(n_dot_l - 0.05, 18.0);
    let lit = sharpened * n_dot_l * illumination;

    // ---- View-dependent scatter ----
    // Forward scatter: when the viewer is on the opposite side of the ring
    // from the sun, fine particles glow as light is scattered through them.
    let forward = max(-dot(to_view, to_sun), 0.0);
    let forward_scatter = pow(forward, 3.5) * lighting.x * base_alpha;

    // Back scatter / opposition surge: when the sun is behind the viewer,
    // ring particles light up brightly along that line of sight.
    let back = max(dot(to_view, to_sun), 0.0);
    let back_scatter = pow(back, 6.0) * lighting.y * illumination;

    // ---- Narrow specular off icy particles ----
    let half_dir = safe_normalize(to_sun + to_view);
    let spec_dot = abs(dot(raw_normal, half_dir));
    let specular = pow(spec_dot, 48.0) * lighting.z * illumination;

    let ambient = lighting.w;
    let intensity = lit * params.w + ambient + back_scatter + specular + forward_scatter;
    let color = base_color * intensity;

    // Fade alpha slightly inside the planet shadow so the dark band reads
    // as a true gap rather than just dimmed material, and lift alpha a touch
    // along the back-scatter peak for the bright opposition surge.
    let alpha = clamp(
        base_alpha * (illumination * 0.85 + 0.15) + back_scatter * 0.35,
        0.0,
        1.0,
    );

    return vec4(color, alpha);
}
