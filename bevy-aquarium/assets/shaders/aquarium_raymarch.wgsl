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
    froxel_masks: array<vec4u, 576>,
    sh_l0: array<vec4f, 320>,
    sh_l1x: array<vec4f, 320>,
    sh_l1y: array<vec4f, 320>,
    sh_l1z: array<vec4f, 320>,
};

@group(0) @binding(0) var in_texture: texture_2d<f32>;
@group(0) @binding(1) var in_sampler: sampler;
@group(0) @binding(2) var<uniform> field: AquariumRaymarch;

const FROXEL_WIDTH: u32 = 16u;
const FROXEL_HEIGHT: u32 = 9u;
const FROXEL_DEPTH: u32 = 16u;
const LIGHT_FROXEL_WIDTH: u32 = 8u;
const LIGHT_FROXEL_HEIGHT: u32 = 5u;
const LIGHT_FROXEL_DEPTH: u32 = 8u;

struct TerrainHit {
    color: vec3f,
    alpha: f32,
    t: f32,
};

fn camera_ray(uv: vec2f) -> vec3f {
    let top = mix(field.ray00.xyz, field.ray10.xyz, uv.x);
    let bottom = mix(field.ray01.xyz, field.ray11.xyz, uv.x);
    return normalize(mix(top, bottom, uv.y));
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

fn depth_window_fade(distance_value: f32) -> f32 {
    let edge = max(field.depth_span * 0.08, 0.08);
    let near_fade = smoothstep(field.depth_near, field.depth_near + edge, distance_value);
    let far_fade = 1.0 - smoothstep(field.depth_far - edge, field.depth_far, distance_value);
    return clamp(near_fade * far_fade, 0.0, 1.0);
}

fn froxel_mask(uv: vec2f, depth_progress: f32) -> u32 {
    let x = clamp(u32(uv.x * f32(FROXEL_WIDTH)), 0u, FROXEL_WIDTH - 1u);
    let y = clamp(u32(uv.y * f32(FROXEL_HEIGHT)), 0u, FROXEL_HEIGHT - 1u);
    let z = clamp(u32(depth_progress * f32(FROXEL_DEPTH)), 0u, FROXEL_DEPTH - 1u);
    let index = z * FROXEL_WIDTH * FROXEL_HEIGHT + y * FROXEL_WIDTH + x;
    return field.froxel_masks[index / 4u][index % 4u];
}

fn light_froxel_index(uv: vec2f, depth_progress: f32) -> u32 {
    let x = clamp(u32(uv.x * f32(LIGHT_FROXEL_WIDTH)), 0u, LIGHT_FROXEL_WIDTH - 1u);
    let y = clamp(u32(uv.y * f32(LIGHT_FROXEL_HEIGHT)), 0u, LIGHT_FROXEL_HEIGHT - 1u);
    let z = clamp(u32(depth_progress * f32(LIGHT_FROXEL_DEPTH)), 0u, LIGHT_FROXEL_DEPTH - 1u);
    return z * LIGHT_FROXEL_WIDTH * LIGHT_FROXEL_HEIGHT + y * LIGHT_FROXEL_WIDTH + x;
}

fn sample_sh_lighting(normal: vec3f, uv: vec2f, depth_progress: f32) -> vec3f {
    let index = light_froxel_index(uv, depth_progress);
    let y0 = 0.282095;
    let y1 = 0.488603;
    let lit = field.sh_l0[index].rgb * y0
        + field.sh_l1x[index].rgb * (y1 * normal.x)
        + field.sh_l1y[index].rgb * (y1 * normal.y)
        + field.sh_l1z[index].rgb * (y1 * normal.z);
    return max(lit, vec3f(0.0));
}

fn atmosphere_sample(point: vec3f) -> f32 {
    let surface_height = grid_height(point.xy);
    let height_above_grid = max(point.z - surface_height, 0.0);
    return exp(-height_above_grid * 0.62) * 0.018;
}

fn aces(color: vec3f) -> vec3f {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((color * (a * color + b)) / (color * (c * color + d) + e), vec3f(0.0), vec3f(1.0));
}

fn terrain_hit(ray_origin: vec3f, ray_dir: vec3f, uv: vec2f) -> TerrainHit {
    var previous_t = field.depth_near;
    var previous_point = ray_origin + ray_dir * previous_t;
    var previous_sdf = previous_point.z - grid_height(previous_point.xy);
    var hit_low = field.depth_near;
    var hit_high = field.depth_far + 1.0;

    for (var step = 1u; step <= 56u; step = step + 1u) {
        let progress = f32(step) / 56.0;
        let t = field.depth_near + progress * field.depth_span;
        let point = ray_origin + ray_dir * t;
        let edge = grid_edge_fade(point.xy);
        let sdf = point.z - grid_height(point.xy);
        if (edge > 0.0 && previous_sdf > 0.0 && sdf <= 0.0) {
            hit_low = previous_t;
            hit_high = t;
            break;
        }
        previous_t = t;
        previous_sdf = sdf;
    }

    if (hit_high > field.depth_far) {
        return TerrainHit(vec3f(0.0), 0.0, field.depth_far + 1.0);
    }

    for (var refine = 0u; refine < 6u; refine = refine + 1u) {
        let mid_t = (hit_low + hit_high) * 0.5;
        let mid_point = ray_origin + ray_dir * mid_t;
        let mid_sdf = mid_point.z - grid_height(mid_point.xy);
        if (mid_sdf > 0.0) {
            hit_low = mid_t;
        } else {
            hit_high = mid_t;
        }
    }

    let surface_t = hit_high;
    let surface_point = ray_origin + ray_dir * surface_t;
    let height = grid_height(surface_point.xy);
    let edge_fade = grid_edge_fade(surface_point.xy) * depth_window_fade(surface_t);
    if (edge_fade <= 0.0) {
        return TerrainHit(vec3f(0.0), 0.0, field.depth_far + 1.0);
    }

    let eps = 0.05;
    let hx = grid_height(surface_point.xy + vec2f(eps, 0.0)) - grid_height(surface_point.xy - vec2f(eps, 0.0));
    let hy = grid_height(surface_point.xy + vec2f(0.0, eps)) - grid_height(surface_point.xy - vec2f(0.0, eps));
    let normal = normalize(vec3f(-hx, -hy, 2.0 * eps));
    let view_dir = normalize(ray_origin - surface_point);
    let fresnel = pow(1.0 - clamp(dot(normal, view_dir), 0.0, 1.0), 2.6);
    let lines = grid_line_factor(surface_point.xy);
    let field_energy = clamp(abs(height) * 1.15, 0.0, 1.0);
    let depth_progress = clamp((surface_t - field.depth_near) / max(field.depth_span, 0.001), 0.0, 1.0);
    let light = sample_sh_lighting(normal, uv, depth_progress);
    let grid_base = mix(vec3f(0.015, 0.18, 0.16), vec3f(0.58, 1.0, 0.84), lines);
    let hot = mix(grid_base, vec3f(1.0, 0.58, 0.24), field_energy * 0.62);
    let lit = hot * edge_fade * (light * (0.34 + fresnel * 0.35) + lines * 0.035);
    let alpha = clamp(edge_fade * (0.22 + lines * 0.58 + field_energy * 0.22 + fresnel * 0.12), 0.0, 0.88);
    return TerrainHit(lit, alpha, surface_t);
}

@fragment
fn fs_main(input: FullscreenVertexOutput) -> @location(0) vec4f {
    let base = textureSample(in_texture, in_sampler, input.uv);
    let ray_origin = field.camera_position.xyz;
    let ray_dir = camera_ray(input.uv);
    let terrain = terrain_hit(ray_origin, ray_dir, input.uv);

    var best_t = field.depth_far;
    var best_color = vec3f(0.0);
    var best_alpha = 0.0;
    var tested_mask = 0u;

    for (var step = 0u; step < FROXEL_DEPTH; step = step + 1u) {
        let progress = (f32(step) + 0.5) / f32(FROXEL_DEPTH);
        let mask = froxel_mask(input.uv, progress) & ~tested_mask;
        tested_mask |= mask;
        if (mask == 0u) {
            continue;
        }
        for (var i = 0u; i < 8u; i = i + 1u) {
            if (f32(i) >= field.body_count || (mask & (1u << i)) == 0u) {
                continue;
            }
            let body = field.bodies[i];
            let color = field.colors[i];
            let self_flag = color.w;
            let radius = body.w;
            let oc = ray_origin - body.xyz;
            let b = dot(oc, ray_dir);
            let c = dot(oc, oc) - radius * radius;
            let h = b * b - c;
            if (h < 0.0) {
                continue;
            }
            let root = sqrt(h);
            let t0 = -b - root;
            let t1 = -b + root;
            let hit_t = select(t1, t0, t0 > field.depth_near);
            if (hit_t <= field.depth_near || hit_t >= best_t || hit_t > field.depth_far) {
                continue;
            }
            let hit = ray_origin + ray_dir * hit_t;
            let local = (hit - body.xyz) / max(radius, 0.001);
            let displacement = fbm4(vec4f(local * mix(0.72, 1.12, self_flag), field.time * mix(0.06, 0.16, self_flag))) * mix(0.035, 0.14, self_flag);
            let displaced_radius = radius * (1.0 + displacement);
            let displaced_c = dot(oc, oc) - displaced_radius * displaced_radius;
            let displaced_h = b * b - displaced_c;
            if (displaced_h < 0.0) {
                continue;
            }
            let displaced_root = sqrt(displaced_h);
            let displaced_t0 = -b - displaced_root;
            let displaced_t1 = -b + displaced_root;
            let displaced_t = select(displaced_t1, displaced_t0, displaced_t0 > field.depth_near);
            if (displaced_t <= field.depth_near || displaced_t >= best_t || displaced_t > field.depth_far) {
                continue;
            }
            let displaced_hit = ray_origin + ray_dir * displaced_t;
            let normal = normalize((displaced_hit - body.xyz) / max(displaced_radius, 0.001));
            let view_dir = normalize(ray_origin - displaced_hit);
            let fresnel = pow(1.0 - clamp(dot(normal, view_dir), 0.0, 1.0), 4.0);
            let plasma = pow(max(fbm4(vec4f(local * mix(1.35, 2.15, self_flag), field.time * 0.24)) * 0.5 + 0.5, 0.0), mix(2.4, 5.4, self_flag));
            let depth_progress = clamp((displaced_t - field.depth_near) / max(field.depth_span, 0.001), 0.0, 1.0);
            let light = sample_sh_lighting(normal, input.uv, depth_progress);
            let chrome = color.rgb * (0.025 + light * (0.36 + fresnel * 0.42)) + light * fresnel * 0.45;
            let solar = vec3f(4.2, 2.1, 0.55) * (0.8 + plasma * 1.5);
            best_color = mix(chrome, solar, self_flag) * depth_window_fade(displaced_t);
            best_alpha = 0.96 * depth_window_fade(displaced_t);
            best_t = displaced_t;
        }
    }

    var field_color = terrain.color * terrain.alpha;
    var field_alpha = terrain.alpha;
    if (best_alpha > 0.0 && best_t <= terrain.t) {
        field_color = best_color * best_alpha;
        field_alpha = best_alpha;
    }
    var atmosphere = vec3f(0.0);
    var transmittance = 1.0;
    var previous_t = field.depth_near;
    let atmosphere_end = min(min(terrain.t, best_t), field.depth_far);
    for (var i = 1u; i <= 24u; i = i + 1u) {
        let progress = f32(i) / 24.0;
        let t = field.depth_near + progress * progress * max(atmosphere_end - field.depth_near, 0.0);
        let step_size = max(t - previous_t, 0.0001);
        previous_t = t;
        let point = ray_origin + ray_dir * t;
        let depth_progress = clamp((t - field.depth_near) / max(field.depth_span, 0.001), 0.0, 1.0);
        let density = atmosphere_sample(point) * depth_window_fade(t);
        let light = sample_sh_lighting(vec3f(0.0, 0.0, 1.0), input.uv, depth_progress);
        let extinction = density * 2.8;
        let step_transmittance = exp(-extinction * step_size);
        atmosphere += transmittance * light * density * step_size * vec3f(0.48, 0.62, 0.78);
        transmittance *= step_transmittance;
    }
    field_color = field_color * transmittance + atmosphere;
    field_alpha = max(field_alpha, clamp(1.0 - transmittance, 0.0, 0.62));
    let mapped = aces(field_color);
    return vec4f(mix(base.rgb, mapped, clamp(field_alpha, 0.0, 0.96)), 1.0);
}
