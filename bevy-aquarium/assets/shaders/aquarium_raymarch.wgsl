#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct AquariumRaymarch {
    time: f32,
    body_count: f32,
    grid_center: vec2f,
    grid_half_extent: f32,
    depth_near: f32,
    depth_far: f32,
    depth_span: f32,
    camera_position: vec4f,
    ray00: vec4f,
    ray10: vec4f,
    ray01: vec4f,
    ray11: vec4f,
    bodies: array<vec4f, 8>,
    colors: array<vec4f, 8>,
};

@group(0) @binding(0) var in_texture: texture_2d<f32>;
@group(0) @binding(1) var in_sampler: sampler;
@group(0) @binding(2) var<uniform> field: AquariumRaymarch;

fn camera_ray(uv: vec2f) -> vec3f {
    let top = mix(field.ray00.xyz, field.ray10.xyz, uv.x);
    let bottom = mix(field.ray01.xyz, field.ray11.xyz, uv.x);
    return normalize(mix(top, bottom, uv.y));
}

fn hash(value: f32) -> f32 {
    return fract(sin(value * 12.9898) * 43758.5453);
}

fn noise4(p: vec4f) -> f32 {
    return sin(dot(p, vec4f(1.71, 2.43, 3.17, 1.19)))
        * cos(dot(p, vec4f(2.13, -1.37, 1.91, 2.61)))
        + 0.45 * sin(dot(p, vec4f(-1.11, 3.03, 2.07, 1.73)) + sin(p.w + p.x));
}

fn fbm4(p0: vec4f) -> f32 {
    var p = p0;
    var value = 0.0;
    var amplitude = 0.5;
    for (var i = 0; i < 4; i = i + 1) {
        value += noise4(p) * amplitude;
        p = vec4f(p.yzx * 2.03 + vec3f(3.7, 1.9, 5.1), p.w * 1.37 + 2.11);
        amplitude *= 0.52;
    }
    return clamp(value, -1.0, 1.0);
}

fn power_pulse(distance_value: f32, radius: f32, power: f32) -> f32 {
    let normalized = clamp(distance_value / max(radius, 0.001), 0.0, 1.0);
    let shaped = pow(1.0 - normalized, power);
    return shaped * shaped * (3.0 - 2.0 * shaped);
}

fn grid_local(xy: vec2f) -> vec2f {
    return (xy - field.grid_center) / max(field.grid_half_extent, 0.001);
}

fn grid_edge_fade(xy: vec2f) -> f32 {
    let edge = max(abs(grid_local(xy)).x, abs(grid_local(xy)).y);
    return 1.0 - smoothstep(0.82, 1.0, edge);
}

fn grid_height(xy: vec2f) -> f32 {
    var positive = 0.0;
    var negative = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        if (f32(i) >= field.body_count) {
            break;
        }
        let source = field.bodies[i];
        let self_flag = field.colors[i].w;
        let delta = xy - source.xy;
        let well = power_pulse(length(delta), mix(3.8, 8.5, self_flag), mix(2.1, 2.85, self_flag));
        let wave = sin(length(delta) * mix(2.4, 1.2, self_flag) - field.time * mix(1.35, 0.74, self_flag));
        let signed_height = -well * mix(0.42, 1.34, self_flag) + wave * well * mix(0.022, 0.055, self_flag);
        positive += max(signed_height, 0.0);
        negative += max(-signed_height, 0.0);
    }
    let slow = sin((xy.x * 0.08 + xy.y * 0.06) + field.time * 0.27)
        * sin((xy.x * -0.04 + xy.y * 0.07) - field.time * 0.19) * 0.035;
    positive += max(slow, 0.0);
    negative += max(-slow, 0.0);
    return positive - negative;
}

fn grid_line_factor(xy: vec2f) -> f32 {
    let cell = max(field.grid_half_extent / 36.0, 0.22);
    let grid = abs(fract((xy - field.grid_center) / cell) - vec2f(0.5));
    let line = 1.0 - smoothstep(0.018, 0.042, min(grid.x, grid.y) * cell);
    return line * grid_edge_fade(xy);
}

fn environment_color(direction: vec3f) -> vec3f {
    let up = vec3f(0.506, 0.516, 0.533);
    let horizon = vec3f(0.152, 0.178, 0.194);
    let down = vec3f(0.411, 0.531, 0.694);
    let key = vec3f(6.0, 7.625, 9.875);
    let vertical = clamp(direction.z * 0.5 + 0.5, 0.0, 1.0);
    let base = mix(down, up, vertical);
    let side = smoothstep(0.1, 0.82, abs(direction.x) + abs(direction.y) * 0.55);
    let key_lobe = pow(max(dot(normalize(direction), normalize(vec3f(0.36, -0.28, 0.89))), 0.0), 18.0);
    return mix(base, horizon, side * 0.38) + key * key_lobe * 0.22;
}

fn grid_fog_sample(point: vec3f) -> vec4f {
    let height = grid_height(point.xy);
    let edge_fade = grid_edge_fade(point.xy);
    let fog_height = point.z - height;
    let lower_shelf = exp(-abs(fog_height - 0.22) * 2.35);
    let upper_shelf = exp(-abs(fog_height - 0.82) * 4.0) * 0.22;
    let flow_noise = fbm4(vec4f(point.xy * 0.08, fog_height * 0.7, field.time * 0.035)) * 0.5 + 0.5;
    let vertical_window = smoothstep(-0.18, 0.18, fog_height) * (1.0 - smoothstep(1.42, 1.86, fog_height));
    let fog_bank = (lower_shelf + upper_shelf) * edge_fade * vertical_window * (0.52 + flow_noise * 0.76);
    let density = fog_bank * (0.075 + max(-height, 0.0) * 0.045);
    let albedo = mix(vec3f(0.62, 0.82, 0.9), vec3f(0.86, 1.0, 0.94), clamp(max(-height, 0.0), 0.0, 1.0));
    return vec4f(albedo, density);
}

fn henyey_greenstein(cos_theta: f32, anisotropy: f32) -> f32 {
    let g2 = anisotropy * anisotropy;
    return (1.0 - g2) / max(pow(1.0 + g2 - 2.0 * anisotropy * cos_theta, 1.5) * 12.56637, 0.0001);
}

fn blue_noise_offset(pixel: vec2f, time: f32) -> f32 {
    let p = floor(pixel);
    let interleaved = fract(52.9829189 * fract(dot(p, vec2f(0.06711056, 0.00583715))));
    let temporal = fract(time * 0.61803398875);
    return -fract(interleaved + temporal);
}

fn depth_window_fade(distance_value: f32) -> f32 {
    let edge = max(field.depth_span * 0.08, 0.08);
    let near_fade = smoothstep(field.depth_near, field.depth_near + edge, distance_value);
    let far_fade = 1.0 - smoothstep(field.depth_far - edge, field.depth_far, distance_value);
    return clamp(near_fade * far_fade, 0.0, 1.0);
}

fn aces(color: vec3f) -> vec3f {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((color * (a * color + b)) / (color * (c * color + d) + e), vec3f(0.0), vec3f(1.0));
}

@fragment
fn fs_main(input: FullscreenVertexOutput) -> @location(0) vec4f {
    let base = textureSample(in_texture, in_sampler, input.uv);
    let dims = vec2f(textureDimensions(in_texture));
    let pixel = input.uv * dims;
    let ray_origin = field.camera_position.xyz;
    let ray_dir = camera_ray(input.uv);

    var transmittance = 1.0;
    var scattering = vec3f(0.0);
    var solid = vec3f(0.0);
    var solid_alpha = 0.0;
    var solid_transmittance = 1.0;
    var surface = vec3f(0.0);
    var surface_alpha = 0.0;
    var surface_transmittance = 1.0;
    var accumulated_density = 0.0;

    if (abs(ray_dir.z) > 0.001) {
        let base_t = (0.0 - ray_origin.z) / ray_dir.z;
        let base_point = ray_origin + ray_dir * base_t;
        if (base_t > 0.0 && grid_edge_fade(base_point.xy) > 0.0) {
            let height = grid_height(base_point.xy);
            let surface_t = (height - ray_origin.z) / ray_dir.z;
            let surface_point = ray_origin + ray_dir * surface_t;
            let edge_fade = grid_edge_fade(surface_point.xy) * depth_window_fade(surface_t);
            if (surface_t > 0.0 && edge_fade > 0.0) {
                let eps = 0.05;
                let hx = grid_height(surface_point.xy + vec2f(eps, 0.0)) - grid_height(surface_point.xy - vec2f(eps, 0.0));
                let hy = grid_height(surface_point.xy + vec2f(0.0, eps)) - grid_height(surface_point.xy - vec2f(0.0, eps));
                let normal = normalize(vec3f(-hx, -hy, 2.0 * eps));
                let view_dir = normalize(ray_origin - surface_point);
                let fresnel = pow(1.0 - clamp(dot(normal, view_dir), 0.0, 1.0), 2.6);
                let lines = grid_line_factor(surface_point.xy);
                let field_energy = clamp(abs(height) * 1.15, 0.0, 1.0);
                let glow = max(lines, field_energy * 0.55);
                let grid_base = mix(vec3f(0.015, 0.18, 0.16), vec3f(0.58, 1.0, 0.84), lines);
                let hot = mix(grid_base, vec3f(1.0, 0.58, 0.24), field_energy * 0.62);
                surface = hot * edge_fade * (0.18 + glow * 1.15 + fresnel * 0.38);
                surface_alpha = clamp(edge_fade * (0.16 + lines * 0.54 + field_energy * 0.18 + fresnel * 0.16), 0.0, 0.82);
            }
        }
    }

    var previous_ray_distance = field.depth_near;
    let raymarch_offset = blue_noise_offset(pixel, field.time);
    for (var step = 1u; step <= 56u; step = step + 1u) {
        let exponential_progress = clamp((f32(step) + raymarch_offset) / 56.0, 0.0, 1.0);
        let ray_distance = field.depth_near + exponential_progress * exponential_progress * field.depth_span;
        let step_size = max(ray_distance - previous_ray_distance, 0.0001);
        let bounds_fade = depth_window_fade(ray_distance);
        let previous_sample_point = ray_origin + ray_dir * previous_ray_distance;
        let sample_point = ray_origin + ray_dir * ray_distance;
        let mid_sample_point = (previous_sample_point + sample_point) * 0.5;
        previous_ray_distance = ray_distance;

        var density = 0.0;
        var tint = vec3f(0.0);
        var saturated_sdf_color = vec3f(0.0);
        var saturated_sdf_weight = 0.0;

        let previous_fog = grid_fog_sample(previous_sample_point);
        let mid_fog = grid_fog_sample(mid_sample_point);
        let current_fog = grid_fog_sample(sample_point);
        let fog_density = (previous_fog.a + mid_fog.a * 4.0 + current_fog.a) / 6.0 * bounds_fade;
        if (fog_density > 0.0001) {
            let fog_albedo = (previous_fog.rgb * previous_fog.a + mid_fog.rgb * mid_fog.a * 4.0 + current_fog.rgb * current_fog.a)
                / max(previous_fog.a + mid_fog.a * 4.0 + current_fog.a, 0.0001);
            let light_dir = normalize(vec3f(0.36, -0.28, 0.89));
            let phase = henyey_greenstein(dot(ray_dir, light_dir), 0.42);
            let sky_light = environment_color(vec3f(0.12, -0.08, 1.0)) * 0.72 + environment_color(-ray_dir) * 0.18;
            let key_light = environment_color(light_dir) * phase * 3.2;
            density += fog_density;
            tint += fog_albedo * fog_density * (sky_light + key_light);
        }

        for (var i = 0u; i < 8u; i = i + 1u) {
            if (f32(i) >= field.body_count) {
                break;
            }
            let body = field.bodies[i];
            let color = field.colors[i];
            let self_flag = color.w;
            let radius = max(body.w, 0.001);
            let previous_local = (previous_sample_point - body.xyz) / radius;
            let mid_local = (mid_sample_point - body.xyz) / radius;
            let local = (sample_point - body.xyz) / radius;
            let previous_displacement = fbm4(vec4f(previous_local * mix(0.72, 1.12, self_flag), field.time * mix(0.06, 0.16, self_flag))) * mix(0.035, 0.14, self_flag);
            let mid_displacement = fbm4(vec4f(mid_local * mix(0.72, 1.12, self_flag), field.time * mix(0.06, 0.16, self_flag))) * mix(0.035, 0.14, self_flag);
            let displacement = fbm4(vec4f(local * mix(0.72, 1.12, self_flag), field.time * mix(0.06, 0.16, self_flag))) * mix(0.035, 0.14, self_flag);
            let previous_sdf = length(previous_local) - (1.0 + previous_displacement);
            let mid_sdf = length(mid_local) - (1.0 + mid_displacement);
            let sdf = length(local) - (1.0 + displacement);
            let plasma = pow(max(fbm4(vec4f(local * mix(1.35, 2.15, self_flag), field.time * 0.24)) * 0.5 + 0.5, 0.0), mix(2.4, 5.4, self_flag));
            let previous_atmosphere = exp(-max(previous_sdf, 0.0) * mix(4.6, 2.25, self_flag));
            let mid_atmosphere = exp(-max(mid_sdf, 0.0) * mix(4.6, 2.25, self_flag));
            let atmosphere = (previous_atmosphere + mid_atmosphere * 4.0 + exp(-max(sdf, 0.0) * mix(4.6, 2.25, self_flag))) / 6.0;
            let segment_sdf = min(sdf, min(previous_sdf, mid_sdf));
            let normalized_step = step_size / radius;
            let solid_core = 1.0 - smoothstep(-0.08, 0.06, segment_sdf);
            let solid_coverage = clamp((0.12 - segment_sdf) / max(normalized_step, 0.001), 0.0, 1.0);
            let normal = normalize(mid_local + vec3f(0.0001, 0.0002, 0.0003));
            let view_dir = normalize(ray_origin - sample_point);
            let reflected = reflect(-view_dir, normal);
            let fresnel = pow(1.0 - clamp(dot(normal, view_dir), 0.0, 1.0), 4.0);
            let studio_reflection = environment_color(reflected);
            let diffuse_wrap = environment_color(normal) * (0.08 + 0.18 * color.rgb);
            let chrome = diffuse_wrap + mix(color.rgb * 0.18, studio_reflection, 0.82 + fresnel * 0.16);
            let solar = vec3f(4.2, 2.1, 0.55) * (0.8 + plasma * 1.5);
            let sdf_color = mix(chrome, solar, self_flag);
            let solid_density = solid_core * solid_coverage * mix(12.0, 24.0, self_flag);
            let local_density = atmosphere * (0.004 + radius * 0.18 + self_flag * 0.055) * (0.55 + plasma * mix(0.42, 1.45, self_flag));
            density += (local_density + solid_density) * bounds_fade;
            tint += (local_density * mix(color.rgb, vec3f(3.8, 1.85, 0.42), self_flag) + solid_density * sdf_color) * bounds_fade;
            saturated_sdf_color += sdf_color * solid_core * bounds_fade;
            saturated_sdf_weight += solid_core * bounds_fade;
        }

        let extinction = density * 4.6;
        let step_transmittance = exp(-extinction * step_size);
        let luminance = tint * 0.86;
        let incoming_transmittance = transmittance;
        scattering += transmittance * (luminance - luminance * step_transmittance) / max(extinction, 0.0001);
        let optical_depth = extinction * step_size;
        accumulated_density += optical_depth;
        transmittance *= step_transmittance;
        if (accumulated_density > 1.05) {
            let volume_color = luminance / max(density, 0.0001);
            let sdf_color = saturated_sdf_color / max(saturated_sdf_weight, 0.0001);
            let sdf_blend = clamp(saturated_sdf_weight, 0.0, 1.0);
            solid = mix(volume_color, sdf_color, sdf_blend);
            solid_alpha = 0.96;
            solid_transmittance = incoming_transmittance;
            break;
        }
    }

    let fog_alpha = clamp(1.0 - transmittance, 0.0, 0.72);
    var field_color = surface * surface_alpha * surface_transmittance + scattering;
    var field_alpha = max(fog_alpha, surface_alpha);
    if (solid_alpha > 0.0) {
        field_color = solid * solid_alpha * solid_transmittance + scattering;
        field_alpha = max(field_alpha, solid_alpha);
    }
    let mapped = aces(field_color);
    return vec4f(mix(base.rgb, mapped, clamp(field_alpha, 0.0, 0.92)), 1.0);
}
