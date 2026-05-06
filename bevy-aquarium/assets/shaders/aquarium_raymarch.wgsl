#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct AquariumRaymarch {
    time: f32,
    body_count: f32,
    grid_center: vec2f,
    grid_half_extent: f32,
    previous_grid_center: vec2f,
    previous_grid_half_extent: f32,
    delta_time: f32,
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
};

struct ShVolume {
    values: array<vec4f>,
};

@group(0) @binding(0) var in_texture: texture_2d<f32>;
@group(0) @binding(1) var in_sampler: sampler;
@group(0) @binding(2) var<uniform> field: AquariumRaymarch;
@group(0) @binding(3) var<storage, read> sh_volume: ShVolume;
@group(0) @binding(4) var<storage, read> previous_sh_volume: ShVolume;
@group(0) @binding(5) var<storage, read_write> next_sh_volume: ShVolume;

const FROXEL_WIDTH: u32 = 16u;
const FROXEL_HEIGHT: u32 = 9u;
const FROXEL_DEPTH: u32 = 16u;
const LIGHT_GRID_WIDTH: u32 = 32u;
const LIGHT_GRID_HEIGHT: u32 = 32u;
const LIGHT_GRID_DEPTH: u32 = 12u;
const LIGHT_GRID_COUNT: u32 = LIGHT_GRID_WIDTH * LIGHT_GRID_HEIGHT * LIGHT_GRID_DEPTH;
const SH_L0_OFFSET: u32 = 0u;
const SH_L1X_OFFSET: u32 = LIGHT_GRID_COUNT;
const SH_L1Y_OFFSET: u32 = LIGHT_GRID_COUNT * 2u;
const SH_L1Z_OFFSET: u32 = LIGHT_GRID_COUNT * 3u;

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
    let radius = length(grid_local(xy));
    return 1.0 - smoothstep(0.70, 1.0, radius);
}

fn grid_volume_top(half_extent: f32) -> f32 {
    return max(8.0, half_extent * 0.18);
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

fn light_grid_index_xyz(x: u32, y: u32, z: u32) -> u32 {
    return z * LIGHT_GRID_WIDTH * LIGHT_GRID_HEIGHT + y * LIGHT_GRID_WIDTH + x;
}

fn sh_read(volume_index: u32, offset: u32) -> vec3f {
    return sh_volume.values[offset + volume_index].rgb;
}

fn sh_lighting_at(index: u32, normal: vec3f) -> vec3f {
    let y0 = 0.282095;
    let y1 = 0.488603;
    return sh_read(index, SH_L0_OFFSET) * y0
        + sh_read(index, SH_L1X_OFFSET) * (y1 * normal.x)
        + sh_read(index, SH_L1Y_OFFSET) * (y1 * normal.y)
        + sh_read(index, SH_L1Z_OFFSET) * (y1 * normal.z);
}

fn grid_volume_position(point: vec3f) -> vec3f {
    let local = grid_local(point.xy);
    let surface_height = grid_height(point.xy);
    let height_above_grid = max(point.z - surface_height, 0.0);
    let top = grid_volume_top(field.grid_half_extent);
    return vec3f(
        (local.x * 0.5 + 0.5) * f32(LIGHT_GRID_WIDTH) - 0.5,
        (local.y * 0.5 + 0.5) * f32(LIGHT_GRID_HEIGHT) - 0.5,
        clamp(height_above_grid / top, 0.0, 1.0) * f32(LIGHT_GRID_DEPTH) - 0.5,
    );
}

fn sample_sh_lighting(normal: vec3f, point: vec3f) -> vec3f {
    let edge_fade = grid_edge_fade(point.xy);
    let height_above_grid = point.z - grid_height(point.xy);
    if (edge_fade <= 0.0 || height_above_grid < -0.35 || height_above_grid > grid_volume_top(field.grid_half_extent)) {
        return vec3f(0.0);
    }
    let p = clamp(
        grid_volume_position(point),
        vec3f(0.0),
        vec3f(f32(LIGHT_GRID_WIDTH - 1u), f32(LIGHT_GRID_HEIGHT - 1u), f32(LIGHT_GRID_DEPTH - 1u)),
    );
    let base = vec3u(floor(p));
    let next = min(base + vec3u(1u), vec3u(LIGHT_GRID_WIDTH - 1u, LIGHT_GRID_HEIGHT - 1u, LIGHT_GRID_DEPTH - 1u));
    let f = fract(p);

    let c000 = sh_lighting_at(light_grid_index_xyz(base.x, base.y, base.z), normal);
    let c100 = sh_lighting_at(light_grid_index_xyz(next.x, base.y, base.z), normal);
    let c010 = sh_lighting_at(light_grid_index_xyz(base.x, next.y, base.z), normal);
    let c110 = sh_lighting_at(light_grid_index_xyz(next.x, next.y, base.z), normal);
    let c001 = sh_lighting_at(light_grid_index_xyz(base.x, base.y, next.z), normal);
    let c101 = sh_lighting_at(light_grid_index_xyz(next.x, base.y, next.z), normal);
    let c011 = sh_lighting_at(light_grid_index_xyz(base.x, next.y, next.z), normal);
    let c111 = sh_lighting_at(light_grid_index_xyz(next.x, next.y, next.z), normal);
    let xy0 = mix(mix(c000, c100, f.x), mix(c010, c110, f.x), f.y);
    let xy1 = mix(mix(c001, c101, f.x), mix(c011, c111, f.x), f.y);
    let lit = mix(xy0, xy1, f.z);
    return max(lit, vec3f(0.0)) * edge_fade;
}

fn interleaved_noise(pixel: vec2f) -> f32 {
    return fract(52.9829189 * fract(dot(floor(pixel), vec2f(0.06711056, 0.00583715))));
}

fn previous_coeff_at(index: u32, offset: u32) -> vec4f {
    return previous_sh_volume.values[offset + index];
}

fn previous_coeff_sample(position: vec3f, offset: u32) -> vec4f {
    let p = clamp(
        position,
        vec3f(0.0),
        vec3f(f32(LIGHT_GRID_WIDTH - 1u), f32(LIGHT_GRID_HEIGHT - 1u), f32(LIGHT_GRID_DEPTH - 1u)),
    );
    let base = vec3u(floor(p));
    let next = min(base + vec3u(1u), vec3u(LIGHT_GRID_WIDTH - 1u, LIGHT_GRID_HEIGHT - 1u, LIGHT_GRID_DEPTH - 1u));
    let f = fract(p);
    let c000 = previous_coeff_at(light_grid_index_xyz(base.x, base.y, base.z), offset);
    let c100 = previous_coeff_at(light_grid_index_xyz(next.x, base.y, base.z), offset);
    let c010 = previous_coeff_at(light_grid_index_xyz(base.x, next.y, base.z), offset);
    let c110 = previous_coeff_at(light_grid_index_xyz(next.x, next.y, base.z), offset);
    let c001 = previous_coeff_at(light_grid_index_xyz(base.x, base.y, next.z), offset);
    let c101 = previous_coeff_at(light_grid_index_xyz(next.x, base.y, next.z), offset);
    let c011 = previous_coeff_at(light_grid_index_xyz(base.x, next.y, next.z), offset);
    let c111 = previous_coeff_at(light_grid_index_xyz(next.x, next.y, next.z), offset);
    let xy0 = mix(mix(c000, c100, f.x), mix(c010, c110, f.x), f.y);
    let xy1 = mix(mix(c001, c101, f.x), mix(c011, c111, f.x), f.y);
    return mix(xy0, xy1, f.z);
}

fn previous_grid_volume_position(current_xy: vec2f, height_above_grid: f32) -> vec3f {
    let previous_half_extent = max(field.previous_grid_half_extent, 0.001);
    let previous_local = (current_xy - field.previous_grid_center) / previous_half_extent;
    let previous_top = grid_volume_top(previous_half_extent);
    return vec3f(
        (previous_local.x * 0.5 + 0.5) * f32(LIGHT_GRID_WIDTH) - 0.5,
        (previous_local.y * 0.5 + 0.5) * f32(LIGHT_GRID_HEIGHT) - 0.5,
        clamp(height_above_grid / previous_top, 0.0, 1.0) * f32(LIGHT_GRID_DEPTH) - 0.5,
    );
}

fn write_sh(index: u32, l0: vec4f, l1x: vec4f, l1y: vec4f, l1z: vec4f) {
    next_sh_volume.values[SH_L0_OFFSET + index] = vec4f(clamp(l0.rgb, vec3f(0.0), vec3f(18.0)), 0.0);
    next_sh_volume.values[SH_L1X_OFFSET + index] = vec4f(clamp(l1x.rgb, vec3f(0.0), vec3f(18.0)), 0.0);
    next_sh_volume.values[SH_L1Y_OFFSET + index] = vec4f(clamp(l1y.rgb, vec3f(0.0), vec3f(18.0)), 0.0);
    next_sh_volume.values[SH_L1Z_OFFSET + index] = vec4f(clamp(l1z.rgb, vec3f(0.0), vec3f(18.0)), 0.0);
}

fn flare_impulse(phase: f32) -> f32 {
    let rise = smoothstep(0.015, 0.08, phase);
    let fall = 1.0 - smoothstep(0.12, 0.32, phase);
    return rise * fall;
}

@compute @workgroup_size(64)
fn cs_grid_lighting(@builtin(global_invocation_id) id: vec3u) {
    let index = id.x;
    if (index >= LIGHT_GRID_COUNT) {
        return;
    }
    let z = index / (LIGHT_GRID_WIDTH * LIGHT_GRID_HEIGHT);
    let rem = index - z * LIGHT_GRID_WIDTH * LIGHT_GRID_HEIGHT;
    let y = rem / LIGHT_GRID_WIDTH;
    let x = rem - y * LIGHT_GRID_WIDTH;

    let uv = vec2f((f32(x) + 0.5) / f32(LIGHT_GRID_WIDTH), (f32(y) + 0.5) / f32(LIGHT_GRID_HEIGHT));
    let local = uv * 2.0 - 1.0;
    let xy = field.grid_center + local * field.grid_half_extent;
    let edge_fade = grid_edge_fade(xy);
    let top = grid_volume_top(field.grid_half_extent);
    let height_above_grid = ((f32(z) + 0.5) / f32(LIGHT_GRID_DEPTH)) * top;
    let point = vec3f(xy, grid_height(xy) + height_above_grid);

    let swirl = vec2f(-local.y, local.x) * (0.18 + 0.05 * sin(field.time * 0.21 + point.z * 0.17));
    let drift = vec2f(
        sin(point.y * 0.08 + point.z * 0.13 + field.time * 0.31),
        cos(point.x * 0.07 - point.z * 0.11 - field.time * 0.27),
    ) * 0.16;
    let dt = clamp(field.delta_time, 1.0 / 240.0, 1.0 / 15.0);
    let flow_world_xy = (swirl + drift) * field.grid_half_extent * 0.085;
    let previous_xy = xy - flow_world_xy * dt;
    let flow_z = sin(dot(local, vec2f(2.1, -1.7)) + field.time * 0.24) * 0.24;
    let sample_position = previous_grid_volume_position(previous_xy, height_above_grid) - vec3f(0.0, 0.0, flow_z * dt);

    var l0 = previous_coeff_sample(sample_position, SH_L0_OFFSET) * 0.78;
    var l1x = previous_coeff_sample(sample_position, SH_L1X_OFFSET) * 0.78;
    var l1y = previous_coeff_sample(sample_position, SH_L1Y_OFFSET) * 0.78;
    var l1z = previous_coeff_sample(sample_position, SH_L1Z_OFFSET) * 0.78;

    let neighbors = array<vec3i, 6>(
        vec3i(-1, 0, 0),
        vec3i(1, 0, 0),
        vec3i(0, -1, 0),
        vec3i(0, 1, 0),
        vec3i(0, 0, -1),
        vec3i(0, 0, 1),
    );
    for (var i = 0u; i < 6u; i = i + 1u) {
        let n = vec3i(i32(x), i32(y), i32(z)) + neighbors[i];
        if (all(n >= vec3i(0)) && n.x < i32(LIGHT_GRID_WIDTH) && n.y < i32(LIGHT_GRID_HEIGHT) && n.z < i32(LIGHT_GRID_DEPTH)) {
            let ni = light_grid_index_xyz(u32(n.x), u32(n.y), u32(n.z));
            l0 += previous_coeff_at(ni, SH_L0_OFFSET) * 0.018;
            l1x += previous_coeff_at(ni, SH_L1X_OFFSET) * 0.018;
            l1y += previous_coeff_at(ni, SH_L1Y_OFFSET) * 0.018;
            l1z += previous_coeff_at(ni, SH_L1Z_OFFSET) * 0.018;
        }
    }

    for (var body_index = 0u; body_index < 8u; body_index = body_index + 1u) {
        if (f32(body_index) >= field.body_count || field.colors[body_index].w < 0.5) {
            continue;
        }
        let sun_position = field.bodies[body_index].xyz;
        let to_sun = sun_position - point;
        let distance = max(length(to_sun), 0.001);
        let direction = to_sun / distance;
        let outward = -direction;
        let strength = exp(-distance * 0.045) * 8.6 * edge_fade;
        let flare_phase = fract(field.time / 2.15);
        let impulse = flare_impulse(flare_phase);
        let push_distance = field.grid_half_extent * (0.11 + 0.045 * sin(distance * 0.72 + height_above_grid * 0.34)) * impulse;
        let pushed_from_xy = point.xy - outward.xy * push_distance;
        let pushed_sample = previous_grid_volume_position(pushed_from_xy, height_above_grid);
        let carried_l0 = previous_coeff_sample(pushed_sample, SH_L0_OFFSET);
        let carried_l1x = previous_coeff_sample(pushed_sample, SH_L1X_OFFSET);
        let carried_l1y = previous_coeff_sample(pushed_sample, SH_L1Y_OFFSET);
        let carried_l1z = previous_coeff_sample(pushed_sample, SH_L1Z_OFFSET);
        let wave_gain = impulse * edge_fade * smoothstep(0.5, field.grid_half_extent * 0.92, distance);
        l0 += carried_l0 * wave_gain * 0.42;
        l1x += carried_l1x * wave_gain * 0.42;
        l1y += carried_l1y * wave_gain * 0.42;
        l1z += carried_l1z * wave_gain * 0.42;

        let source_core = exp(-pow(distance / max(field.grid_half_extent * 0.08, 0.75), 2.0));
        let source_ripple = 0.72 + 0.28 * sin(distance * 2.35 - field.time * 18.0 + height_above_grid * 0.9);
        let source_flare = impulse * source_core * source_ripple * exp(-height_above_grid / max(top * 0.42, 0.001)) * edge_fade;
        let radiance = vec3f(4.4, 2.35, 0.72) * min(strength, 9.0)
            + vec3f(8.6, 2.85, 0.44) * max(source_flare, 0.0) * 8.0;
        let rgb = vec4f(radiance, 0.0);
        l0 += rgb * 0.282095;
        let flare_direction = normalize(mix(direction, outward, impulse * source_core));
        l1x += rgb * (0.488603 * flare_direction.x);
        l1y += rgb * (0.488603 * flare_direction.y);
        l1z += rgb * (0.488603 * flare_direction.z);
    }

    l0 *= edge_fade;
    l1x *= edge_fade;
    l1y *= edge_fade;
    l1z *= edge_fade;
    write_sh(index, l0, l1x, l1y, l1z);
}

fn atmosphere_sample(point: vec3f) -> f32 {
    let edge_fade = grid_edge_fade(point.xy);
    if (edge_fade <= 0.0) {
        return 0.0;
    }
    let surface_height = grid_height(point.xy);
    let height_above_grid = max(point.z - surface_height, 0.0);
    return exp(-height_above_grid * 0.62) * 0.018 * edge_fade;
}

fn aces(color: vec3f) -> vec3f {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((color * (a * color + b)) / (color * (c * color + d) + e), vec3f(0.0), vec3f(1.0));
}

fn terrain_hit(ray_origin: vec3f, ray_dir: vec3f, jitter: f32) -> TerrainHit {
    var previous_t = field.depth_near;
    var previous_point = ray_origin + ray_dir * previous_t;
    var previous_sdf = previous_point.z - grid_height(previous_point.xy);
    var hit_low = field.depth_near;
    var hit_high = field.depth_far + 1.0;

    for (var step = 1u; step <= 72u; step = step + 1u) {
        let progress = clamp((f32(step) - 0.35 + jitter * 0.7) / 72.0, 0.0, 1.0);
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
    let light = sample_sh_lighting(normal, surface_point);
    let grid_base = mix(vec3f(0.015, 0.18, 0.16), vec3f(0.58, 1.0, 0.84), lines);
    let hot = mix(grid_base, vec3f(1.0, 0.58, 0.24), field_energy * 0.62);
    let lit = hot * edge_fade * (light * (0.34 + fresnel * 0.35) + lines * 0.035);
    let alpha = clamp(edge_fade * (0.22 + lines * 0.58 + field_energy * 0.22 + fresnel * 0.12), 0.0, 0.88);
    return TerrainHit(lit, alpha, surface_t);
}

@fragment
fn fs_main(input: FullscreenVertexOutput) -> @location(0) vec4f {
    let base = textureSample(in_texture, in_sampler, input.uv);
    let pixel = input.uv * vec2f(textureDimensions(in_texture));
    let jitter = interleaved_noise(pixel + field.time);
    let ray_origin = field.camera_position.xyz;
    let ray_dir = camera_ray(input.uv);
    let terrain = terrain_hit(ray_origin, ray_dir, jitter);

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
            let light = sample_sh_lighting(normal, displaced_hit);
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
        let progress = clamp((f32(i) - 0.5 + jitter) / 24.0, 0.0, 1.0);
        let t = field.depth_near + progress * progress * max(atmosphere_end - field.depth_near, 0.0);
        let step_size = max(t - previous_t, 0.0001);
        previous_t = t;
        let point = ray_origin + ray_dir * t;
        let density = atmosphere_sample(point) * depth_window_fade(t);
        let light = sample_sh_lighting(vec3f(0.0, 0.0, 1.0), point);
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
