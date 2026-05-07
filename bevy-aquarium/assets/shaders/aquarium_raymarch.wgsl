#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput
#import bevy_pbr::{
    pbr_deferred_types as deferred_types,
    rgb9e5,
    utils::octahedral_encode,
}

struct AquariumRaymarch {
    time: f32,
    body_count: f32,
    debug_mode: f32,
    debug_pad: f32,
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
    clip_from_world: mat4x4f,
    previous_clip_from_world: mat4x4f,
    bodies: array<vec4f, 8>,
    colors: array<vec4f, 8>,
    froxel_masks: array<vec4u, 576>,
};

struct ShVolume {
    values: array<vec4f>,
};

struct BrickMap {
    flags: array<u32>,
};

struct GridHeightField {
    samples: array<vec4f>,
};

struct FogHistory {
    samples: array<vec4f>,
};

@group(0) @binding(0) var in_texture: texture_2d<f32>;
@group(0) @binding(1) var in_sampler: sampler;
@group(0) @binding(2) var<uniform> field: AquariumRaymarch;
@group(0) @binding(3) var<storage, read> sh_volume: ShVolume;
@group(0) @binding(4) var<storage, read> previous_sh_volume: ShVolume;
@group(0) @binding(5) var<storage, read_write> next_sh_volume: ShVolume;
@group(0) @binding(6) var<storage, read_write> light_bricks: BrickMap;
@group(0) @binding(7) var<storage, read_write> grid_height_field: GridHeightField;
@group(0) @binding(8) var<storage, read> previous_fog_history: FogHistory;
@group(0) @binding(9) var<storage, read_write> next_fog_history: FogHistory;

const GRID_FIELD_SIZE: u32 = 128u;
const FOG_HISTORY_WIDTH: u32 = 64u;
const FOG_HISTORY_HEIGHT: u32 = 64u;
const FOG_HISTORY_COUNT: u32 = FOG_HISTORY_WIDTH * FOG_HISTORY_HEIGHT;
const FROXEL_WIDTH: u32 = 16u;
const FROXEL_HEIGHT: u32 = 9u;
const FROXEL_DEPTH: u32 = 16u;
const LIGHT_GRID_WIDTH: u32 = 32u;
const LIGHT_GRID_HEIGHT: u32 = 32u;
const LIGHT_GRID_DEPTH: u32 = 12u;
const LIGHT_GRID_COUNT: u32 = LIGHT_GRID_WIDTH * LIGHT_GRID_HEIGHT * LIGHT_GRID_DEPTH;
const LIGHT_BRICK_WIDTH: u32 = 8u;
const LIGHT_BRICK_HEIGHT: u32 = 8u;
const LIGHT_BRICK_DEPTH: u32 = 4u;
const LIGHT_BRICK_COUNT: u32 = LIGHT_BRICK_WIDTH * LIGHT_BRICK_HEIGHT * LIGHT_BRICK_DEPTH;
const LIGHT_BRICK_TERRAIN: u32 = 1u;
const LIGHT_BRICK_BODY: u32 = 2u;
const LIGHT_BRICK_SELF: u32 = 4u;
const LIGHT_BRICK_FLARE: u32 = 8u;
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

fn analytic_grid_height(xy: vec2f) -> f32 {
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

fn grid_field_index(x: u32, y: u32) -> u32 {
    return y * GRID_FIELD_SIZE + x;
}

fn fog_history_index(x: u32, y: u32) -> u32 {
    return y * FOG_HISTORY_WIDTH + x;
}

fn grid_height(xy: vec2f) -> f32 {
    return grid_sample(xy).x;
}

fn grid_sample(xy: vec2f) -> vec4f {
    let local = clamp(grid_local(xy) * 0.5 + vec2f(0.5), vec2f(0.0), vec2f(1.0));
    let p = local * f32(GRID_FIELD_SIZE - 1u);
    let base = vec2u(floor(p));
    let next = min(base + vec2u(1u), vec2u(GRID_FIELD_SIZE - 1u));
    let f = fract(p);
    let s00 = grid_height_field.samples[grid_field_index(base.x, base.y)];
    let s10 = grid_height_field.samples[grid_field_index(next.x, base.y)];
    let s01 = grid_height_field.samples[grid_field_index(base.x, next.y)];
    let s11 = grid_height_field.samples[grid_field_index(next.x, next.y)];
    return mix(mix(s00, s10, f.x), mix(s01, s11, f.x), f.y);
}

fn grid_normal_from_sample(sample: vec4f) -> vec3f {
    let cell_world = max((field.grid_half_extent * 2.0) / f32(GRID_FIELD_SIZE - 1u), 0.001);
    return normalize(vec3f(-sample.y, -sample.z, cell_world * 2.0));
}

fn fog_history_density_at(local: vec2f) -> vec4f {
    let uv = clamp(local * 0.5 + vec2f(0.5), vec2f(0.0), vec2f(1.0));
    let p = uv * vec2f(f32(FOG_HISTORY_WIDTH - 1u), f32(FOG_HISTORY_HEIGHT - 1u));
    let base = vec2u(floor(p));
    let next = min(base + vec2u(1u), vec2u(FOG_HISTORY_WIDTH - 1u, FOG_HISTORY_HEIGHT - 1u));
    let f = fract(p);
    let s00 = previous_fog_history.samples[fog_history_index(base.x, base.y)];
    let s10 = previous_fog_history.samples[fog_history_index(next.x, base.y)];
    let s01 = previous_fog_history.samples[fog_history_index(base.x, next.y)];
    let s11 = previous_fog_history.samples[fog_history_index(next.x, next.y)];
    return mix(mix(s00, s10, f.x), mix(s01, s11, f.x), f.y);
}

fn debug_fog_history_at(xy: vec2f) -> vec4f {
    let local = grid_local(xy);
    let uv = clamp(local * 0.5 + vec2f(0.5), vec2f(0.0), vec2f(1.0));
    let p = uv * vec2f(f32(FOG_HISTORY_WIDTH - 1u), f32(FOG_HISTORY_HEIGHT - 1u));
    let base = vec2u(floor(p));
    let next = min(base + vec2u(1u), vec2u(FOG_HISTORY_WIDTH - 1u, FOG_HISTORY_HEIGHT - 1u));
    let f = fract(p);
    let s00 = previous_fog_history.samples[fog_history_index(base.x, base.y)];
    let s10 = previous_fog_history.samples[fog_history_index(next.x, base.y)];
    let s01 = previous_fog_history.samples[fog_history_index(base.x, next.y)];
    let s11 = previous_fog_history.samples[fog_history_index(next.x, next.y)];
    return mix(mix(s00, s10, f.x), mix(s01, s11, f.x), f.y);
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

fn light_brick_index_xyz(x: u32, y: u32, z: u32) -> u32 {
    return z * LIGHT_BRICK_WIDTH * LIGHT_BRICK_HEIGHT + y * LIGHT_BRICK_WIDTH + x;
}

fn light_brick_for_cell(x: u32, y: u32, z: u32) -> u32 {
    let bx = min(x * LIGHT_BRICK_WIDTH / LIGHT_GRID_WIDTH, LIGHT_BRICK_WIDTH - 1u);
    let by = min(y * LIGHT_BRICK_HEIGHT / LIGHT_GRID_HEIGHT, LIGHT_BRICK_HEIGHT - 1u);
    let bz = min(z * LIGHT_BRICK_DEPTH / LIGHT_GRID_DEPTH, LIGHT_BRICK_DEPTH - 1u);
    return light_brick_index_xyz(bx, by, bz);
}

fn light_brick_center_and_half_extents(index: u32) -> array<vec3f, 2> {
    let bz = index / (LIGHT_BRICK_WIDTH * LIGHT_BRICK_HEIGHT);
    let rem = index - bz * LIGHT_BRICK_WIDTH * LIGHT_BRICK_HEIGHT;
    let by = rem / LIGHT_BRICK_WIDTH;
    let bx = rem - by * LIGHT_BRICK_WIDTH;
    let brick_uv = vec2f((f32(bx) + 0.5) / f32(LIGHT_BRICK_WIDTH), (f32(by) + 0.5) / f32(LIGHT_BRICK_HEIGHT));
    let local = brick_uv * 2.0 - 1.0;
    let xy = field.grid_center + local * field.grid_half_extent;
    let top = grid_volume_top(field.grid_half_extent);
    let height_above_grid = ((f32(bz) + 0.5) / f32(LIGHT_BRICK_DEPTH)) * top;
    let center = vec3f(xy, grid_height(xy) + height_above_grid);
    let half_extent = vec3f(
        field.grid_half_extent / f32(LIGHT_BRICK_WIDTH),
        field.grid_half_extent / f32(LIGHT_BRICK_HEIGHT),
        top / f32(LIGHT_BRICK_DEPTH) * 0.5,
    );
    return array<vec3f, 2>(center, half_extent);
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

fn fog_source_density(xy: vec2f) -> f32 {
    let sample = grid_sample(xy);
    let slope = length(sample.yz) / max((field.grid_half_extent * 2.0) / f32(GRID_FIELD_SIZE - 1u), 0.001);
    let cup = smoothstep(0.02, 0.9, max(-sample.x, 0.0));
    let slope_lift = smoothstep(0.05, 0.45, slope);
    return clamp(sample.w * (0.025 + cup * 0.55 + slope_lift * 0.18), 0.0, 1.0);
}

@compute @workgroup_size(64)
fn cs_update_grid_height(@builtin(global_invocation_id) id: vec3u) {
    let index = id.x;
    if (index >= GRID_FIELD_SIZE * GRID_FIELD_SIZE) {
        return;
    }
    let y = index / GRID_FIELD_SIZE;
    let x = index - y * GRID_FIELD_SIZE;
    let uv = vec2f(
        f32(x) / f32(GRID_FIELD_SIZE - 1u),
        f32(y) / f32(GRID_FIELD_SIZE - 1u),
    );
    let local = uv * 2.0 - vec2f(1.0);
    let xy = field.grid_center + local * field.grid_half_extent;
    let height = analytic_grid_height(xy);
    let cell_world = max((field.grid_half_extent * 2.0) / f32(GRID_FIELD_SIZE - 1u), 0.001);
    let hx = analytic_grid_height(xy + vec2f(cell_world, 0.0)) - analytic_grid_height(xy - vec2f(cell_world, 0.0));
    let hy = analytic_grid_height(xy + vec2f(0.0, cell_world)) - analytic_grid_height(xy - vec2f(0.0, cell_world));
    let edge = grid_edge_fade(xy);
    grid_height_field.samples[index] = vec4f(height, hx, hy, edge);
}

@compute @workgroup_size(64)
fn cs_update_fog_history(@builtin(global_invocation_id) id: vec3u) {
    let index = id.x;
    if (index >= FOG_HISTORY_COUNT) {
        return;
    }
    let y = index / FOG_HISTORY_WIDTH;
    let x = index - y * FOG_HISTORY_WIDTH;
    let uv = vec2f(
        (f32(x) + 0.5) / f32(FOG_HISTORY_WIDTH),
        (f32(y) + 0.5) / f32(FOG_HISTORY_HEIGHT),
    );
    let local = uv * 2.0 - vec2f(1.0);
    let xy = field.grid_center + local * field.grid_half_extent;
    let density = fog_source_density(xy);
    let previous_local = (xy - field.previous_grid_center) / max(field.previous_grid_half_extent, 0.001);
    let previous = fog_history_density_at(previous_local);
    let inside_previous = select(0.0, 1.0, all(abs(previous_local) <= vec2f(1.0)));
    let rejection = clamp(abs(density - previous.x) * 2.2 + (1.0 - inside_previous), 0.0, 1.0);
    let history_weight = (1.0 - rejection) * exp(-field.delta_time * 1.8);
    let blended = mix(density, previous.x, clamp(history_weight, 0.0, 0.92));
    next_fog_history.samples[index] = vec4f(blended, density, rejection, inside_previous);
}

@compute @workgroup_size(64)
fn cs_update_light_bricks(@builtin(global_invocation_id) id: vec3u) {
    let index = id.x;
    if (index >= LIGHT_BRICK_COUNT) {
        return;
    }

    let brick = light_brick_center_and_half_extents(index);
    let center = brick[0];
    let half_extent = brick[1];
    let local_radius = length(grid_local(center.xy));
    var flags = select(0u, LIGHT_BRICK_TERRAIN, local_radius < 1.05 && center.z - half_extent.z <= grid_height(center.xy) + 0.55);

    for (var body_index = 0u; body_index < 8u; body_index = body_index + 1u) {
        if (f32(body_index) >= field.body_count) {
            break;
        }
        let body = field.bodies[body_index];
        let delta = abs(body.xyz - center) - half_extent;
        let outside_distance = length(max(delta, vec3f(0.0)));
        let self_flag = field.colors[body_index].w;
        let influence_radius = body.w + mix(2.6, field.grid_half_extent * 0.18, self_flag);
        if (outside_distance <= influence_radius) {
            flags |= LIGHT_BRICK_BODY;
        }
        if (self_flag > 0.5 && outside_distance <= influence_radius * 1.8) {
            flags |= LIGHT_BRICK_SELF;
        }
        let flare_phase = fract(field.time / 2.15);
        let impulse = flare_impulse(flare_phase);
        let flare_radius = field.grid_half_extent * (0.16 + 0.72 * flare_phase);
        let flare_width = max(field.grid_half_extent * 0.16, 2.0);
        let distance_to_self = distance(center.xy, body.xy);
        if (self_flag > 0.5 && impulse > 0.0 && abs(distance_to_self - flare_radius) <= flare_width) {
            flags |= LIGHT_BRICK_FLARE;
        }
    }

    light_bricks.flags[index] = flags;
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
    let brick_flags = light_bricks.flags[light_brick_for_cell(x, y, z)];
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

    if (brick_flags == 0u) {
        write_sh(index, l0 * edge_fade * 0.72, l1x * edge_fade * 0.72, l1y * edge_fade * 0.72, l1z * edge_fade * 0.72);
        return;
    }

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
            let scatter = select(0.016, 0.026, (brick_flags & (LIGHT_BRICK_BODY | LIGHT_BRICK_SELF | LIGHT_BRICK_FLARE)) != 0u);
            l0 += previous_coeff_at(ni, SH_L0_OFFSET) * scatter;
            l1x += previous_coeff_at(ni, SH_L1X_OFFSET) * scatter;
            l1y += previous_coeff_at(ni, SH_L1Y_OFFSET) * scatter;
            l1z += previous_coeff_at(ni, SH_L1Z_OFFSET) * scatter;
        }
    }

    if ((brick_flags & (LIGHT_BRICK_BODY | LIGHT_BRICK_SELF | LIGHT_BRICK_FLARE)) != 0u) {
        let detail_phase = sin(dot(point, vec3f(0.31, -0.23, 0.47)) + field.time * 1.7);
        let twist = vec2f(-local.y, local.x) * (0.45 + 0.2 * detail_phase);
        let detailed_xy = xy - twist * field.grid_half_extent * 0.018;
        let detailed_sample = previous_grid_volume_position(detailed_xy, height_above_grid + detail_phase * top * 0.018);
        l0 += previous_coeff_sample(detailed_sample, SH_L0_OFFSET) * 0.10;
        l1x += previous_coeff_sample(detailed_sample, SH_L1X_OFFSET) * 0.10;
        l1y += previous_coeff_sample(detailed_sample, SH_L1Y_OFFSET) * 0.10;
        l1z += previous_coeff_sample(detailed_sample, SH_L1Z_OFFSET) * 0.10;
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

struct DeferredPrepassOutput {
    @location(0) normal: vec4f,
    @location(1) motion_vector: vec2f,
    @location(2) deferred: vec4u,
    @location(3) deferred_lighting_pass_id: u32,
    @builtin(frag_depth) depth: f32,
};

struct SurfaceSample {
    hit: bool,
    kind: f32,
    point: vec3f,
    normal: vec3f,
    color: vec3f,
    emissive: vec3f,
    unlit: bool,
    roughness: f32,
    metallic: f32,
    t: f32,
};

fn clip_depth(point: vec3f) -> f32 {
    let clip = field.clip_from_world * vec4f(point, 1.0);
    return clamp(clip.z / max(clip.w, 0.0001), 0.0, 1.0);
}

fn motion_vector(point: vec3f) -> vec2f {
    let clip = field.clip_from_world * vec4f(point, 1.0);
    let previous_clip = field.previous_clip_from_world * vec4f(point, 1.0);
    let current_position = clip.xy / max(clip.w, 0.0001);
    let previous_position = previous_clip.xy / max(previous_clip.w, 0.0001);
    return (current_position - previous_position) * vec2f(0.5, -0.5);
}

fn deferred_flags(unlit: bool) -> u32 {
    return select(0u, 1u, unlit);
}

fn pack_deferred_gbuffer(sample: SurfaceSample) -> vec4u {
    let base_color_srgb = pow(clamp(sample.color, vec3f(0.0), vec3f(1.0)), vec3f(1.0 / 2.2));
    let base_rough = deferred_types::pack_unorm4x8_(vec4f(base_color_srgb, sample.roughness));
    var visible_payload = sample.emissive;
    if (sample.unlit) {
        visible_payload = max(sample.color, sample.emissive);
    }
    let emissive = rgb9e5::vec3_to_rgb9e5_(visible_payload);
    let props = deferred_types::pack_unorm4x8_(vec4f(0.5, sample.metallic, 1.0, 0.0));
    let oct_normal = octahedral_encode(normalize(sample.normal));
    let normal_flags = deferred_types::pack_24bit_normal_and_flags(oct_normal, deferred_flags(sample.unlit));
    return vec4u(base_rough, emissive, props, normal_flags);
}

fn surface_sample(ray_origin: vec3f, ray_dir: vec3f, uv: vec2f, jitter: f32) -> SurfaceSample {
    let terrain = terrain_hit(ray_origin, ray_dir, jitter);
    var sample = SurfaceSample(
        false,
        0.0,
        vec3f(0.0),
        vec3f(0.0, 0.0, 1.0),
        vec3f(0.0),
        vec3f(0.0),
        false,
        0.72,
        0.0,
        field.depth_far + 1.0,
    );

    if (terrain.alpha > 0.0) {
        let point = ray_origin + ray_dir * terrain.t;
        let field_sample = grid_sample(point.xy);
        sample.hit = true;
        sample.kind = 1.0;
        sample.point = point;
        sample.normal = grid_normal_from_sample(field_sample);
        sample.color = mix(vec3f(0.015, 0.18, 0.16), vec3f(0.58, 1.0, 0.84), grid_line_factor(point.xy));
        sample.emissive = max(terrain.color, sample.color * 0.28);
        sample.unlit = true;
        sample.roughness = 0.82;
        sample.t = terrain.t;
    }

    var tested_mask = 0u;
    for (var step = 0u; step < FROXEL_DEPTH; step = step + 1u) {
        let progress = (f32(step) + 0.5) / f32(FROXEL_DEPTH);
        let mask = froxel_mask(uv, progress) & ~tested_mask;
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
            if (hit_t <= field.depth_near || hit_t >= sample.t || hit_t > field.depth_far) {
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
            if (displaced_t <= field.depth_near || displaced_t >= sample.t || displaced_t > field.depth_far) {
                continue;
            }
            let point = ray_origin + ray_dir * displaced_t;
            let normal = normalize((point - body.xyz) / max(displaced_radius, 0.001));
            let plasma = pow(max(fbm4(vec4f(local * mix(1.35, 2.15, self_flag), field.time * 0.24)) * 0.5 + 0.5, 0.0), mix(2.4, 5.4, self_flag));
            sample.hit = true;
            sample.kind = 2.0;
            sample.point = point;
            sample.normal = normal;
            sample.color = color.rgb;
            sample.emissive = max(color.rgb * 0.9, vec3f(4.2, 2.1, 0.55) * self_flag * (0.6 + plasma));
            sample.unlit = true;
            sample.roughness = mix(0.18, 0.38, self_flag);
            sample.metallic = 1.0 - self_flag * 0.45;
            sample.t = displaced_t;
        }
    }

    return sample;
}

fn debug_mode() -> u32 {
    return u32(field.debug_mode + 0.5);
}

fn debug_froxel_occupancy(uv: vec2f, sample_t: f32) -> f32 {
    let progress = clamp((sample_t - field.depth_near) / max(field.depth_span, 0.001), 0.0, 1.0);
    let mask = froxel_mask(uv, progress);
    var count = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        if ((mask & (1u << i)) != 0u) {
            count += 1.0;
        }
    }
    return count / 8.0;
}

fn debug_sh_luminance(sample: SurfaceSample) -> f32 {
    let light = sample_sh_lighting(sample.normal, sample.point);
    return clamp(dot(light, vec3f(0.2126, 0.7152, 0.0722)) / 4.0, 0.0, 1.0);
}

fn debug_fog_history(sample: SurfaceSample) -> vec3f {
    let history = debug_fog_history_at(sample.point.xy);
    let density_color = vec3f(0.08, 0.42, 0.95) * history.x;
    let rejection_color = vec3f(1.0, 0.22, 0.05) * history.z;
    let invalid_color = vec3f(0.5, 0.0, 0.9) * (1.0 - history.w);
    return density_color + rejection_color + invalid_color;
}

fn debug_color(sample: SurfaceSample, uv: vec2f) -> vec3f {
    let mode = debug_mode();
    if (mode == 1u) {
        return select(vec3f(0.0, 0.72, 0.95), vec3f(1.0, 0.72, 0.12), sample.kind > 1.5);
    }
    if (mode == 2u) {
        let depth_value = clamp((sample.t - field.depth_near) / max(field.depth_span, 0.001), 0.0, 1.0);
        return vec3f(depth_value);
    }
    if (mode == 3u) {
        return sample.normal * 0.5 + vec3f(0.5);
    }
    if (mode == 4u) {
        let velocity = length(motion_vector(sample.point)) * 32.0;
        return mix(vec3f(0.02, 0.05, 0.18), vec3f(1.0, 0.22, 0.04), clamp(velocity, 0.0, 1.0));
    }
    if (mode == 5u) {
        let occupancy = debug_froxel_occupancy(uv, sample.t);
        return mix(vec3f(0.02, 0.02, 0.04), vec3f(0.12, 0.95, 0.48), occupancy);
    }
    if (mode == 6u) {
        let luminance = debug_sh_luminance(sample);
        return mix(vec3f(0.01, 0.015, 0.04), vec3f(1.0, 0.76, 0.18), luminance);
    }
    if (mode == 7u) {
        return debug_fog_history(sample);
    }
    return sample.color;
}

fn debug_surface_sample(sample: SurfaceSample, uv: vec2f) -> SurfaceSample {
    if (debug_mode() == 0u) {
        return sample;
    }
    var debug_sample = sample;
    let color = debug_color(sample, uv);
    debug_sample.color = color;
    debug_sample.emissive = color;
    debug_sample.unlit = true;
    debug_sample.roughness = 1.0;
    debug_sample.metallic = 0.0;
    return debug_sample;
}

@fragment
fn fs_deferred_prepass(input: FullscreenVertexOutput) -> DeferredPrepassOutput {
    let jitter = interleaved_noise(input.position.xy + field.time);
    let ray_origin = field.camera_position.xyz;
    let ray_dir = camera_ray(input.uv);
    let sample = debug_surface_sample(surface_sample(ray_origin, ray_dir, input.uv, jitter), input.uv);
    if (!sample.hit) {
        discard;
    }

    return DeferredPrepassOutput(
        vec4f(sample.normal * 0.5 + vec3f(0.5), 1.0),
        motion_vector(sample.point),
        pack_deferred_gbuffer(sample),
        1u,
        clip_depth(sample.point),
    );
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
        if (ray_dir.z < -0.0001) {
            let plane_t = (0.0 - ray_origin.z) / ray_dir.z;
            let plane_point = ray_origin + ray_dir * plane_t;
            let edge_fade = grid_edge_fade(plane_point.xy) * depth_window_fade(plane_t);
            if (plane_t > field.depth_near && plane_t < field.depth_far && edge_fade > 0.0) {
                let lines = grid_line_factor(plane_point.xy);
                let base = mix(vec3f(0.012, 0.09, 0.085), vec3f(0.50, 0.92, 0.78), lines);
                let alpha = clamp(edge_fade * (0.18 + lines * 0.58), 0.0, 0.74);
                return TerrainHit(base * edge_fade, alpha, plane_t);
            }
        }
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
    let field_sample = grid_sample(surface_point.xy);
    let height = field_sample.x;
    let edge_fade = grid_edge_fade(surface_point.xy) * depth_window_fade(surface_t);
    if (edge_fade <= 0.0) {
        return TerrainHit(vec3f(0.0), 0.0, field.depth_far + 1.0);
    }

    let normal = grid_normal_from_sample(field_sample);
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
