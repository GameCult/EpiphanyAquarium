#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

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

@group(0) @binding(0) var in_texture: texture_2d<f32>;
@group(0) @binding(1) var in_sampler: sampler;
@group(0) @binding(2) var<uniform> field: AquariumRaymarch;
@group(0) @binding(3) var<storage, read> sh_volume: ShVolume;
@group(0) @binding(4) var<storage, read> previous_sh_volume: ShVolume;
@group(0) @binding(5) var<storage, read_write> next_sh_volume: ShVolume;
@group(0) @binding(6) var<storage, read_write> light_bricks: BrickMap;
@group(0) @binding(7) var<storage, read_write> grid_height_field: GridHeightField;

const GRID_FIELD_SIZE: u32 = 128u;
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
const SH_L0_OFFSET: u32 = 0u;
const SH_L1X_OFFSET: u32 = LIGHT_GRID_COUNT;
const SH_L1Y_OFFSET: u32 = LIGHT_GRID_COUNT * 2u;
const SH_L1Z_OFFSET: u32 = LIGHT_GRID_COUNT * 3u;
const GRID_WEATHER_WORLD_SCALE: f32 = 42.0;
const GRID_LINE_WORLD_CELL: f32 = 2.0;

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

fn hash31(p0: vec3f) -> f32 {
    var p = fract(p0 * 0.1031);
    p += dot(p, p.yzx + vec3f(33.33));
    return fract((p.x + p.y) * p.z) * 2.0 - 1.0;
}

fn noised3(x: vec3f) -> vec4f {
    let p = floor(x);
    let w = fract(x);
    let u = w * w * w * (w * (w * 6.0 - 15.0) + vec3f(10.0));
    let du = 30.0 * w * w * (w * (w - vec3f(2.0)) + vec3f(1.0));

    let a = hash31(p + vec3f(0.0, 0.0, 0.0));
    let b = hash31(p + vec3f(1.0, 0.0, 0.0));
    let c = hash31(p + vec3f(0.0, 1.0, 0.0));
    let d = hash31(p + vec3f(1.0, 1.0, 0.0));
    let e = hash31(p + vec3f(0.0, 0.0, 1.0));
    let f = hash31(p + vec3f(1.0, 0.0, 1.0));
    let g = hash31(p + vec3f(0.0, 1.0, 1.0));
    let h = hash31(p + vec3f(1.0, 1.0, 1.0));

    let k0 = a;
    let k1 = b - a;
    let k2 = c - a;
    let k3 = e - a;
    let k4 = a - b - c + d;
    let k5 = a - c - e + g;
    let k6 = a - b - e + f;
    let k7 = -a + b + c - d + e - f - g + h;
    let value = k0
        + k1 * u.x
        + k2 * u.y
        + k3 * u.z
        + k4 * u.x * u.y
        + k5 * u.y * u.z
        + k6 * u.z * u.x
        + k7 * u.x * u.y * u.z;
    let gradient = du * vec3f(
        k1 + k4 * u.y + k6 * u.z + k7 * u.y * u.z,
        k2 + k5 * u.z + k4 * u.x + k7 * u.z * u.x,
        k3 + k6 * u.x + k5 * u.y + k7 * u.x * u.y
    );
    return vec4f(value, gradient);
}

fn ridge(value: f32) -> f32 {
    return 1.0 - abs(value * 2.0 - 1.0);
}

fn grid_weather_color(xy: vec2f, height: f32) -> vec3f {
    let local = grid_local(xy);
    let radius = length(local);
    let edge = grid_edge_fade(xy);
    let world_domain = xy / GRID_WEATHER_WORLD_SCALE;
    let low_warp = vec2f(
        noise4(vec4f(world_domain * 1.35, height * 0.10, field.time * 0.018)),
        noise4(vec4f(world_domain.yx * 1.17 + vec2f(4.7, -2.3), height * 0.08, field.time * -0.014)),
    ) * 0.36;
    let drift = vec2f(0.018, -0.011) * field.time;
    let warped = world_domain + low_warp + drift;
    let sheet = pow(clamp(fbm4(vec4f(warped * 2.65, height * 0.12, field.time * 0.025)) * 0.5 + 0.5, 0.0, 1.0), 1.25);
    let fine_warp = vec2f(
        noise4(vec4f(warped * 4.8 + vec2f(1.9, 7.1), height * 0.18, field.time * 0.041)),
        noise4(vec4f(warped.yx * 4.2 + vec2f(-3.4, 5.6), height * 0.16, field.time * -0.036)),
    ) * 0.075;
    let filament_noise = fbm4(vec4f((warped + fine_warp) * 9.5, height * 0.24, field.time * 0.052)) * 0.5 + 0.5;
    let filament = pow(clamp(ridge(filament_noise), 0.0, 1.0), 3.4);
    let horizon_dark = smoothstep(0.52, 1.02, radius);
    let deep = vec3f(0.006, 0.026, 0.045);
    let blue = vec3f(0.025, 0.20, 0.34);
    let teal = vec3f(0.09, 0.68, 0.62);
    let green = vec3f(0.55, 0.92, 0.36);
    var color = mix(deep, mix(blue, teal, sheet), edge);
    color += green * filament * edge * 0.18;
    color *= mix(1.0, 0.22, horizon_dark);
    return color;
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
    return max(8.0, half_extent);
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

fn grid_line_factor(xy: vec2f) -> f32 {
    let cell = GRID_LINE_WORLD_CELL;
    let grid = abs(fract(xy / cell) - vec2f(0.5));
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
    return max(sh_read(index, SH_L0_OFFSET) * y0
        + sh_read(index, SH_L1X_OFFSET) * (y1 * normal.x)
        + sh_read(index, SH_L1Y_OFFSET) * (y1 * normal.y)
        + sh_read(index, SH_L1Z_OFFSET) * (y1 * normal.z), vec3f(0.0));
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

fn self_body() -> vec4f {
    for (var body_index = 0u; body_index < 8u; body_index = body_index + 1u) {
        if (f32(body_index) >= field.body_count) {
            break;
        }
        if (field.colors[body_index].w > 0.5) {
            return field.bodies[body_index];
        }
    }
    return vec4f(field.grid_center, grid_height(field.grid_center) + 4.0, 1.25);
}

fn shade_diegetic(albedo: vec3f, point: vec3f, normal: vec3f, roughness: f32) -> vec3f {
    let irradiance = sample_sh_lighting(normalize(normal), point);
    let view_dir = normalize(field.camera_position.xyz - point);
    let fresnel = pow(1.0 - clamp(dot(normalize(normal), view_dir), 0.0, 1.0), 4.0);
    let grazing_scatter = sample_sh_lighting(normalize(normal + view_dir * 0.35), point) * fresnel * roughness * 0.08;
    return albedo * (irradiance + grazing_scatter);
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

fn sh_encode_directional_radiance(radiance: vec3f, direction: vec3f) -> array<vec4f, 4> {
    let d = normalize(direction);
    return array<vec4f, 4>(
        vec4f(radiance * 0.282095, 0.0),
        vec4f(radiance * (0.488603 * d.x), 0.0),
        vec4f(radiance * (0.488603 * d.y), 0.0),
        vec4f(radiance * (0.488603 * d.z), 0.0),
    );
}

fn body_occludes_light(point: vec3f) -> bool {
    for (var body_index = 0u; body_index < 8u; body_index = body_index + 1u) {
        if (f32(body_index) >= field.body_count) {
            break;
        }
        if (field.colors[body_index].w > 0.5) {
            continue;
        }
        let body = field.bodies[body_index];
        if (length(point - body.xyz) < body.w * 0.98) {
            return true;
        }
    }
    return false;
}

fn visibility_to_self(point: vec3f) -> f32 {
    let sun = self_body();
    let to_sun = sun.xyz - point;
    let distance_to_center = length(to_sun);
    let travel_distance = max(distance_to_center - sun.w, 0.05);
    let direction = to_sun / max(distance_to_center, 0.001);
    let top = grid_volume_top(field.grid_half_extent);
    let step_length = travel_distance / 12.0;
    var transmittance = 1.0;

    for (var step = 1u; step <= 12u; step = step + 1u) {
        let t = (f32(step) - 0.35) * step_length;
        let sample_point = point + direction * t;
        let edge = grid_edge_fade(sample_point.xy);
        let surface_clearance = sample_point.z - grid_height(sample_point.xy);
        if (edge <= 0.0 || surface_clearance < 0.025 || body_occludes_light(sample_point)) {
            return 0.0;
        }
        let fog_density = exp(-max(surface_clearance, 0.0) / max(top * 0.24, 0.5)) * edge * 0.035;
        transmittance *= exp(-fog_density * step_length);
    }

    return clamp(transmittance, 0.0, 1.0);
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
    let vertical_medium = exp(-height_above_grid / max(top * 0.34, 0.5));
    let terrain_medium = select(0.0, 0.18, (brick_flags & LIGHT_BRICK_TERRAIN) != 0u);
    let body_medium = select(0.0, 0.12, (brick_flags & (LIGHT_BRICK_BODY | LIGHT_BRICK_SELF)) != 0u);
    let scatter_density = edge_fade * clamp(0.045 + vertical_medium * 0.22 + terrain_medium + body_medium, 0.0, 0.75);

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

    var l0 = previous_coeff_sample(sample_position, SH_L0_OFFSET) * 0.62;
    var l1x = previous_coeff_sample(sample_position, SH_L1X_OFFSET) * 0.62;
    var l1y = previous_coeff_sample(sample_position, SH_L1Y_OFFSET) * 0.62;
    var l1z = previous_coeff_sample(sample_position, SH_L1Z_OFFSET) * 0.62;

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
            let scatter = mix(0.010, 0.038, scatter_density);
            l0 += previous_coeff_at(ni, SH_L0_OFFSET) * scatter;
            l1x += previous_coeff_at(ni, SH_L1X_OFFSET) * scatter;
            l1y += previous_coeff_at(ni, SH_L1Y_OFFSET) * scatter;
            l1z += previous_coeff_at(ni, SH_L1Z_OFFSET) * scatter;
        }
    }

    if ((brick_flags & (LIGHT_BRICK_BODY | LIGHT_BRICK_SELF)) != 0u) {
        let detail_phase = sin(dot(point, vec3f(0.31, -0.23, 0.47)) + field.time * 1.7);
        let twist = vec2f(-local.y, local.x) * (0.45 + 0.2 * detail_phase);
        let detailed_xy = xy - twist * field.grid_half_extent * 0.018;
        let detailed_sample = previous_grid_volume_position(detailed_xy, height_above_grid + detail_phase * top * 0.018);
        l0 += previous_coeff_sample(detailed_sample, SH_L0_OFFSET) * 0.10;
        l1x += previous_coeff_sample(detailed_sample, SH_L1X_OFFSET) * 0.10;
        l1y += previous_coeff_sample(detailed_sample, SH_L1Y_OFFSET) * 0.10;
        l1z += previous_coeff_sample(detailed_sample, SH_L1Z_OFFSET) * 0.10;
    }

    let sun = self_body();
    let to_sun = sun.xyz - point;
    let distance = max(length(to_sun), 0.001);
    let direction = to_sun / distance;
    let visibility = visibility_to_self(point);
    let solid_angle = (sun.w * sun.w) / max(distance * distance, sun.w * sun.w);
    let solar_radiance = vec3f(18.0, 9.4, 2.7) * solid_angle * visibility * edge_fade;
    let encoded = sh_encode_directional_radiance(solar_radiance, direction);
    l0 += encoded[0];
    l1x += encoded[1];
    l1y += encoded[2];
    l1z += encoded[3];

    let scatter_glow = sh_encode_directional_radiance(solar_radiance * scatter_density * 0.18, normalize(vec3f(direction.xy * 0.35, 0.65)));
    l0 += scatter_glow[0];
    l1x += scatter_glow[1] * 0.45;
    l1y += scatter_glow[2] * 0.45;
    l1z += scatter_glow[3] * 0.45;

    l0 *= edge_fade;
    l1x *= edge_fade;
    l1y *= edge_fade;
    l1z *= edge_fade;
    write_sh(index, l0, l1x, l1y, l1z);
}

struct SurfaceSample {
    hit: bool,
    kind: f32,
    point: vec3f,
    normal: vec3f,
    color: vec3f,
    t: f32,
};

fn body_displacement(body: vec4f, color: vec4f, point: vec3f) -> f32 {
    let radius = max(body.w, 0.001);
    let self_flag = color.w;
    let local = (point - body.xyz) / radius;
    let seed = vec3f(
        dot(body.xyz, vec3f(0.071, 0.113, 0.047)) + color.r * 3.7,
        dot(body.yzx, vec3f(0.097, -0.061, 0.083)) + color.g * 4.3,
        dot(body.zxy, vec3f(-0.053, 0.089, 0.127)) + color.b * 5.1
    );
    let low_domain = local * mix(0.74, 0.92, self_flag) + seed + vec3f(0.0, field.time * mix(0.018, 0.045, self_flag), 0.0);
    let low = noised3(low_domain);
    let warp = low.yzw * mix(0.10, 0.18, self_flag);
    let warped = local + warp;
    let first = noised3(warped * mix(1.18, 1.42, self_flag) + seed.yzx + vec3f(field.time * mix(0.012, 0.035, self_flag), 0.0, 0.0)).x;
    let second = noised3(warped * mix(2.05, 2.46, self_flag) + seed.zxy - vec3f(0.0, 0.0, field.time * mix(0.022, 0.052, self_flag))).x;
    let amplitude = body_displacement_amplitude(color);
    return clamp(first * 0.68 + second * 0.32, -1.0, 1.0) * amplitude;
}

fn body_displacement_amplitude(color: vec4f) -> f32 {
    return mix(0.035, 0.14, color.w);
}

fn body_bound_radius(body: vec4f, color: vec4f) -> f32 {
    return max(body.w, 0.001) * (1.0 + body_displacement_amplitude(color) + 0.02);
}

fn body_sdf(body: vec4f, color: vec4f, point: vec3f) -> f32 {
    let radius = max(body.w, 0.001);
    let displaced_radius = radius * (1.0 + body_displacement(body, color, point));
    return length(point - body.xyz) - displaced_radius;
}

fn body_normal(body: vec4f, color: vec4f, point: vec3f) -> vec3f {
    let epsilon = max(body.w * 0.012, 0.006);
    let dx = vec3f(epsilon, 0.0, 0.0);
    let dy = vec3f(0.0, epsilon, 0.0);
    let dz = vec3f(0.0, 0.0, epsilon);
    return normalize(vec3f(
        body_sdf(body, color, point + dx) - body_sdf(body, color, point - dx),
        body_sdf(body, color, point + dy) - body_sdf(body, color, point - dy),
        body_sdf(body, color, point + dz) - body_sdf(body, color, point - dz)
    ));
}

fn surface_sample(ray_origin: vec3f, ray_dir: vec3f, uv: vec2f, jitter: f32) -> SurfaceSample {
    let terrain = terrain_hit(ray_origin, ray_dir, jitter);
    var sample = SurfaceSample(
        false,
        0.0,
        vec3f(0.0),
        vec3f(0.0, 0.0, 1.0),
        vec3f(0.0),
        field.depth_far + 1.0,
    );

    if (terrain.alpha > 0.0) {
        let point = ray_origin + ray_dir * terrain.t;
        let field_sample = grid_sample(point.xy);
        let lines = grid_line_factor(point.xy);
        sample.hit = true;
        sample.kind = 1.0;
        sample.point = point;
        sample.normal = grid_normal_from_sample(field_sample);
        let albedo = clamp(terrain.color + vec3f(0.82, 0.92, 0.84) * lines * 0.12, vec3f(0.0), vec3f(0.82));
        let shaded = shade_diegetic(albedo, point, sample.normal, 0.82);
        sample.color = shaded;
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
            let broad_radius = body_bound_radius(body, color);
            let oc = ray_origin - body.xyz;
            let b = dot(oc, ray_dir);
            let c = dot(oc, oc) - broad_radius * broad_radius;
            let h = b * b - c;
            if (h < 0.0) {
                continue;
            }
            let root = sqrt(h);
            let t0 = -b - root;
            let t1 = -b + root;
            let start_t = max(t0, field.depth_near);
            let end_t = min(min(t1, sample.t), field.depth_far);
            if (end_t <= start_t) {
                continue;
            }

            var previous_t = start_t;
            var previous_sdf = body_sdf(body, color, ray_origin + ray_dir * previous_t);
            var hit_low = start_t;
            var hit_high = end_t + 1.0;
            for (var trace_step = 1u; trace_step <= 14u; trace_step = trace_step + 1u) {
                let progress = (f32(trace_step) - 0.35 + jitter * 0.35) / 14.0;
                let t = mix(start_t, end_t, clamp(progress, 0.0, 1.0));
                let point = ray_origin + ray_dir * t;
                let sdf = body_sdf(body, color, point);
                if (previous_sdf > 0.0 && sdf <= 0.0) {
                    hit_low = previous_t;
                    hit_high = t;
                    break;
                }
                previous_t = t;
                previous_sdf = sdf;
            }

            if (hit_high > end_t) {
                continue;
            }

            for (var refine = 0u; refine < 6u; refine = refine + 1u) {
                let mid_t = (hit_low + hit_high) * 0.5;
                let mid_point = ray_origin + ray_dir * mid_t;
                let mid_sdf = body_sdf(body, color, mid_point);
                if (mid_sdf > 0.0) {
                    hit_low = mid_t;
                } else {
                    hit_high = mid_t;
                }
            }

            let displaced_t = hit_high;
            let point = ray_origin + ray_dir * displaced_t;
            let normal = body_normal(body, color, point);
            let local = (point - body.xyz) / max(radius, 0.001);
            let plasma = pow(max(fbm4(vec4f(local * mix(1.35, 2.15, self_flag), field.time * 0.24)) * 0.5 + 0.5, 0.0), mix(2.4, 5.4, self_flag));
            let view_dir = normalize(ray_origin - point);
            let fresnel = pow(1.0 - clamp(dot(normal, view_dir), 0.0, 1.0), 3.6);
            let albedo = clamp(color.rgb, vec3f(0.0), vec3f(0.92));
            let solar = vec3f(5.4, 2.35, 0.58) * (0.72 + plasma * 1.75) + vec3f(1.0, 0.72, 0.24) * fresnel * 1.8;
            sample.hit = true;
            sample.kind = 2.0;
            sample.point = point;
            sample.normal = normal;
            let shaded = shade_diegetic(albedo, point, normal, 0.74);
            sample.color = mix(shaded, solar, self_flag);
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

fn debug_irradiance_luminance(sample: SurfaceSample) -> f32 {
    let light = sample_sh_lighting(sample.normal, sample.point);
    return clamp(dot(light, vec3f(0.2126, 0.7152, 0.0722)) / 4.0, 0.0, 1.0);
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
        let occupancy = debug_froxel_occupancy(uv, sample.t);
        return mix(vec3f(0.02, 0.02, 0.04), vec3f(0.12, 0.95, 0.48), occupancy);
    }
    if (mode == 5u) {
        let luminance = debug_irradiance_luminance(sample);
        return mix(vec3f(0.01, 0.015, 0.04), vec3f(1.0, 0.76, 0.18), luminance);
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
    return debug_sample;
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
                let base = grid_weather_color(plane_point.xy, 0.0) + vec3f(0.82, 0.92, 0.84) * lines * 0.20;
                let alpha = clamp(edge_fade * (0.14 + lines * 0.42), 0.0, 0.62);
                return TerrainHit(clamp(base, vec3f(0.0), vec3f(0.82)), alpha, plane_t);
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
    let weather = grid_weather_color(surface_point.xy, height);
    let grid_base = weather + vec3f(0.82, 0.92, 0.84) * lines * (0.24 + field_energy * 0.08);
    let hot = mix(grid_base, vec3f(0.82, 0.42, 0.18), field_energy * 0.22);
    let albedo = clamp(hot * (0.78 + fresnel * 0.08), vec3f(0.0), vec3f(0.86));
    let alpha = clamp(edge_fade * (0.18 + lines * 0.42 + field_energy * 0.14 + fresnel * 0.08), 0.0, 0.68);
    return TerrainHit(albedo, alpha, surface_t);
}

@fragment
fn fs_main(input: FullscreenVertexOutput) -> @location(0) vec4f {
    let base = textureSample(in_texture, in_sampler, input.uv);
    let pixel = input.uv * vec2f(textureDimensions(in_texture));
    let jitter = interleaved_noise(pixel + field.time);
    let ray_origin = field.camera_position.xyz;
    let ray_dir = camera_ray(input.uv);
    let sample = debug_surface_sample(surface_sample(ray_origin, ray_dir, input.uv, jitter), input.uv);
    if (!sample.hit) {
        return base;
    }
    let alpha = select(0.92, 0.98, sample.kind > 1.5);
    return vec4f(mix(base.rgb, sample.color, alpha), 1.0);
}
