import type { AquariumAgentProjection } from "./aquariumFluid";

type StardustProjection = AquariumAgentProjection & {
  color?: string;
  glow?: string;
};

type GpuApi = {
  requestAdapter: () => Promise<any>;
};

const particleCount = new URLSearchParams(globalThis.location?.search ?? "").has("smoke") ? 16_384 : 1_000_000;
const particleStrideFloats = 12;
const maxStardustAgents = 8;

const stardustComputeShader = /* wgsl */ `
struct Particle {
  position: vec2f,
  velocity: vec2f,
  color: vec4f,
  life: f32,
  size: f32,
  seed: f32,
  pad: f32,
};

struct SimUniforms {
  time: f32,
  dt: f32,
  width: f32,
  height: f32,
  count: f32,
  particleCount: f32,
  flowGain: f32,
  alpha: f32,
};

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<storage, read> agents: array<vec4f>;
@group(0) @binding(2) var<uniform> uniforms: SimUniforms;

fn hash(value: f32) -> f32 {
  return fract(sin(value * 12.9898) * 43758.5453);
}

fn hash2(cell: vec2f) -> f32 {
  return hash(dot(cell, vec2f(127.1, 311.7)));
}

fn wrap2(value: vec2f, size: vec2f) -> vec2f {
  return fract(value / size) * size;
}

fn proceduralCurl(point: vec2f, seed: f32) -> vec2f {
  let angle =
    sin(point.x * 0.013 + uniforms.time * 0.21 + seed * 6.28318) +
    cos(point.y * 0.017 - uniforms.time * 0.16 + seed * 4.71);
  return vec2f(cos(angle * 3.14159), sin(angle * 3.14159));
}

@compute @workgroup_size(128)
fn updateParticles(@builtin(global_invocation_id) id: vec3u) {
  let index = id.x;
  if (index >= u32(uniforms.particleCount)) {
    return;
  }

  var particle = particles[index];
  let pairIndex = index / 2u;
  let companion = index - pairIndex * 2u;
  let span = u32(ceil(sqrt(uniforms.particleCount * 0.5)));
  let cell = vec2f(f32(pairIndex % span), f32(pairIndex / span));
  let center = vec2f(uniforms.width, uniforms.height) * 0.5;
  let spacing = max(min(uniforms.width, uniforms.height) / f32(span), 14.0);
  let scroll = vec2f(uniforms.time * 28.0, uniforms.time * -17.0);
  let gridOffset = floor(scroll / spacing);
  let seededCell = cell + gridOffset;
  let seed = hash2(seededCell);
  let period = 5.2;
  let lifetime = fract(uniforms.time / period + seed);
  let pairPhase = seed * 6.28318;
  let jitter = vec2f(
    hash2(seededCell + vec2f(17.0, 3.0)),
    hash2(seededCell + vec2f(5.0, 29.0))
  ) - vec2f(0.5);
  let pairedOffset = select(-1.0, 1.0, companion == 1u) * vec2f(cos(pairPhase), sin(pairPhase)) * spacing * 0.26;
  var base = (cell - vec2f(f32(span) * 0.5)) * spacing + center + scroll + jitter * spacing * 0.82 + pairedOffset;
  base = wrap2(base, vec2f(uniforms.width, uniforms.height));
  var flow = proceduralCurl(base, seed) * 9.0;

  for (var i = 0u; i < ${maxStardustAgents}u; i = i + 1u) {
    if (f32(i) >= uniforms.count) {
      break;
    }
    let agent = agents[i];
    let delta = agent.xy - base;
    let dist2 = max(dot(delta, delta), 1.0);
    let influence = exp(-dist2 * 0.000018);
    let tangent = normalize(vec2f(-delta.y, delta.x) + vec2f(0.001, 0.0));
    flow += agent.zw * influence * 1.1 + tangent * influence * (16.0 + length(agent.zw) * 0.7);
  }

  particle.velocity = flow * uniforms.flowGain;
  particle.position = wrap2(base - particle.velocity * lifetime * period, vec2f(uniforms.width, uniforms.height));
  particle.life = lifetime;
  particle.seed = seed;
  let speed = clamp(length(particle.velocity) / 80.0, 0.0, 1.0);
  particle.color = vec4f(0.62 + speed * 0.62, 0.92 + speed * 0.32, 0.78 + speed * 0.58, uniforms.alpha * (0.006 + speed * 0.024));
  particle.size = mix(0.18, 0.56, hash(seed * 19.0)) * (0.62 + speed * 0.28) * (1.0 - abs(lifetime - 0.5) * 0.82);
  particles[index] = particle;
}
`;

const stardustRenderShader = /* wgsl */ `
struct Particle {
  position: vec2f,
  velocity: vec2f,
  color: vec4f,
  life: f32,
  size: f32,
  seed: f32,
  pad: f32,
};

struct SimUniforms {
  time: f32,
  dt: f32,
  width: f32,
  height: f32,
  count: f32,
  particleCount: f32,
  flowGain: f32,
  alpha: f32,
};

@group(0) @binding(0) var<storage, read> particles: array<Particle>;
@group(0) @binding(2) var<uniform> uniforms: SimUniforms;

struct VertexOut {
  @builtin(position) position: vec4f,
  @location(0) color: vec4f,
  @location(1) local: vec2f,
};

@vertex
fn vertexMain(@builtin(vertex_index) vertexIndex: u32, @builtin(instance_index) instanceIndex: u32) -> VertexOut {
  let corners = array<vec2f, 6>(
    vec2f(-1.0, -1.0),
    vec2f(1.0, -1.0),
    vec2f(-1.0, 1.0),
    vec2f(-1.0, 1.0),
    vec2f(1.0, -1.0),
    vec2f(1.0, 1.0)
  );
  let particle = particles[instanceIndex];
  let local = corners[vertexIndex];
  let position = particle.position + local * particle.size;
  var out: VertexOut;
  out.position = vec4f((position.x / uniforms.width) * 2.0 - 1.0, 1.0 - (position.y / uniforms.height) * 2.0, 0.0, 1.0);
  out.color = particle.color * (1.0 - abs(particle.life - 0.5) * 1.35);
  out.local = local;
  return out;
}

@fragment
fn fragmentMain(input: VertexOut) -> @location(0) vec4f {
  let falloff = exp(-dot(input.local, input.local) * 3.6);
  return vec4f(input.color.rgb * falloff * 1.8, input.color.a * falloff);
}
`;

const stardustPostShader = /* wgsl */ `
@group(0) @binding(0) var hdrSampler: sampler;
@group(0) @binding(1) var hdrTexture: texture_2d<f32>;

struct VertexOut {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vertexMain(@builtin(vertex_index) vertexIndex: u32) -> VertexOut {
  let positions = array<vec2f, 3>(
    vec2f(-1.0, -1.0),
    vec2f(3.0, -1.0),
    vec2f(-1.0, 3.0)
  );
  let position = positions[vertexIndex];
  var out: VertexOut;
  out.position = vec4f(position, 0.0, 1.0);
  out.uv = position * vec2f(0.5, -0.5) + vec2f(0.5);
  return out;
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
fn fragmentMain(input: VertexOut) -> @location(0) vec4f {
  let hdr = textureSample(hdrTexture, hdrSampler, input.uv);
  let mapped = aces(hdr.rgb);
  return vec4f(mapped, clamp(hdr.a, 0.0, 0.48));
}
`;

const froxelWidth = 96;
const froxelHeight = 54;
const froxelDepth = 24;
const froxelCount = froxelWidth * froxelHeight * froxelDepth;
const froxelShStrideFloats = 16;
const studioEnvironmentData = new Float32Array([
  0.506, 0.516, 0.533, 1,
  0.152, 0.178, 0.194, 1,
  0.411, 0.531, 0.694, 1,
  6.0, 7.625, 9.875, 1,
]);

const froxelMaskComputeShader = /* wgsl */ `
struct SimUniforms {
  time: f32,
  width: f32,
  height: f32,
  count: f32,
  froxelWidth: f32,
  froxelHeight: f32,
  froxelDepth: f32,
  pad: f32,
};

@group(0) @binding(0) var<storage, read_write> primitiveMasks: array<u32>;
@group(0) @binding(1) var<storage, read> agents: array<vec4f>;
@group(0) @binding(2) var<uniform> uniforms: SimUniforms;

@compute @workgroup_size(128)
fn buildPrimitiveMasks(@builtin(global_invocation_id) id: vec3u) {
  let index = id.x;
  let total = u32(uniforms.froxelWidth * uniforms.froxelHeight * uniforms.froxelDepth);
  if (index >= total) {
    return;
  }

  let fw = u32(uniforms.froxelWidth);
  let fh = u32(uniforms.froxelHeight);
  let sliceSize = fw * fh;
  let z = index / sliceSize;
  let rem = index - z * sliceSize;
  let y = rem / fw;
  let x = rem - y * fw;

  let pixel = vec2f(
    (f32(x) + 0.5) / uniforms.froxelWidth * uniforms.width,
    (f32(y) + 0.5) / uniforms.froxelHeight * uniforms.height
  );
  let depth = (f32(z) + 0.5) / uniforms.froxelDepth;
  var mask = 0u;

  for (var i = 0u; i < ${maxStardustAgents}u; i = i + 1u) {
    if (f32(i) >= uniforms.count) {
      break;
    }
    let agent = agents[i];
    let radius = agent.z;
    let atmosphere = radius * mix(1.55, 2.65, agent.w);
    let dz = abs(depth - agent.w * 0.16 - 0.34);
    let depthRadius = mix(0.08, 0.2, agent.w) + radius / max(uniforms.width, uniforms.height) * 1.8;
    let screenHit = distance(pixel, agent.xy) <= atmosphere + max(uniforms.width, uniforms.height) * 0.014;
    let depthHit = dz <= depthRadius;
    if (screenHit && depthHit) {
      mask = mask | (1u << i);
    }
  }
  primitiveMasks[index] = mask;
}
`;

const froxelLightingComputeShader = /* wgsl */ `
struct SimUniforms {
  time: f32,
  width: f32,
  height: f32,
  count: f32,
  froxelWidth: f32,
  froxelHeight: f32,
  froxelDepth: f32,
  pad: f32,
};

@group(0) @binding(0) var<storage, read> primitiveMasks: array<u32>;
@group(0) @binding(1) var<storage, read> agents: array<vec4f>;
@group(0) @binding(2) var<storage, read> colors: array<vec4f>;
@group(0) @binding(3) var<storage, read> environment: array<vec4f>;
@group(0) @binding(4) var<storage, read> previousLighting: array<vec4f>;
@group(0) @binding(5) var<storage, read_write> nextLighting: array<vec4f>;
@group(0) @binding(6) var<uniform> uniforms: SimUniforms;

fn index3(x: u32, y: u32, z: u32) -> u32 {
  return z * u32(uniforms.froxelWidth) * u32(uniforms.froxelHeight) + y * u32(uniforms.froxelWidth) + x;
}

fn lightingBase(index: u32) -> u32 {
  return index * 4u;
}

fn writeLobe(base: ptr<function, array<vec3f, 4>>, direction: vec3f, radiance: vec3f, weight: f32) {
  let dir = normalize(direction + vec3f(0.0001, 0.0002, 0.0003));
  (*base)[0] += radiance * weight;
  (*base)[1] += radiance * dir.x * weight;
  (*base)[2] += radiance * dir.y * weight;
  (*base)[3] += radiance * dir.z * weight;
}

fn readLighting(index: u32) -> array<vec3f, 4> {
  let base = lightingBase(index);
  return array<vec3f, 4>(
    previousLighting[base].rgb,
    previousLighting[base + 1u].rgb,
    previousLighting[base + 2u].rgb,
    previousLighting[base + 3u].rgb
  );
}

@compute @workgroup_size(128)
fn propagateLighting(@builtin(global_invocation_id) id: vec3u) {
  let index = id.x;
  let total = u32(uniforms.froxelWidth * uniforms.froxelHeight * uniforms.froxelDepth);
  if (index >= total) {
    return;
  }

  let fw = u32(uniforms.froxelWidth);
  let fh = u32(uniforms.froxelHeight);
  let fd = u32(uniforms.froxelDepth);
  let sliceSize = fw * fh;
  let z = index / sliceSize;
  let rem = index - z * sliceSize;
  let y = rem / fw;
  let x = rem - y * fw;

  var lighting = array<vec3f, 4>(
    vec3f(0.0),
    vec3f(0.0),
    vec3f(0.0),
    vec3f(0.0)
  );

  let current = readLighting(index);
  for (var c = 0u; c < 4u; c = c + 1u) {
    lighting[c] += current[c] * 0.78;
  }

  var neighborCount = 0.0;
  var neighbor = array<vec3f, 4>(vec3f(0.0), vec3f(0.0), vec3f(0.0), vec3f(0.0));
  if (x > 0u) {
    let sample = readLighting(index3(x - 1u, y, z));
    for (var c = 0u; c < 4u; c = c + 1u) { neighbor[c] += sample[c]; }
    neighborCount += 1.0;
  }
  if (x + 1u < fw) {
    let sample = readLighting(index3(x + 1u, y, z));
    for (var c = 0u; c < 4u; c = c + 1u) { neighbor[c] += sample[c]; }
    neighborCount += 1.0;
  }
  if (y > 0u) {
    let sample = readLighting(index3(x, y - 1u, z));
    for (var c = 0u; c < 4u; c = c + 1u) { neighbor[c] += sample[c]; }
    neighborCount += 1.0;
  }
  if (y + 1u < fh) {
    let sample = readLighting(index3(x, y + 1u, z));
    for (var c = 0u; c < 4u; c = c + 1u) { neighbor[c] += sample[c]; }
    neighborCount += 1.0;
  }
  if (z > 0u) {
    let sample = readLighting(index3(x, y, z - 1u));
    for (var c = 0u; c < 4u; c = c + 1u) { neighbor[c] += sample[c]; }
    neighborCount += 1.0;
  }
  if (z + 1u < fd) {
    let sample = readLighting(index3(x, y, z + 1u));
    for (var c = 0u; c < 4u; c = c + 1u) { neighbor[c] += sample[c]; }
    neighborCount += 1.0;
  }
  if (neighborCount > 0.0) {
    for (var c = 0u; c < 4u; c = c + 1u) {
      lighting[c] += neighbor[c] * (0.14 / neighborCount);
    }
  }

  let edgeX = max(1.0 - min(f32(x), f32(fw - 1u - x)) / 4.0, 0.0);
  let edgeY = max(1.0 - min(f32(y), f32(fh - 1u - y)) / 4.0, 0.0);
  let edgeZ = max(1.0 - min(f32(z), f32(fd - 1u - z)) / 3.0, 0.0);
  let edge = max(max(edgeX, edgeY), edgeZ);
  if (edge > 0.0) {
    writeLobe(&lighting, vec3f(0.2, -0.1, 1.0), environment[0].rgb * 0.32, edge * 0.34);
    writeLobe(&lighting, vec3f(-0.8, 0.25, 0.18), environment[1].rgb * 0.48, edge * max(edgeX, edgeY) * 0.28);
    writeLobe(&lighting, vec3f(0.12, 0.08, -1.0), environment[2].rgb * 0.24, edgeZ * 0.22);
    writeLobe(&lighting, vec3f(0.36, -0.28, 0.89), environment[3].rgb * 0.16, edge * 0.18);
  }

  let pixel = vec2f(
    (f32(x) + 0.5) / uniforms.froxelWidth * uniforms.width,
    (f32(y) + 0.5) / uniforms.froxelHeight * uniforms.height
  );
  let depth = (f32(z) + 0.5) / uniforms.froxelDepth;
  let mask = primitiveMasks[index];
  for (var i = 0u; i < ${maxStardustAgents}u; i = i + 1u) {
    if (f32(i) >= uniforms.count) {
      break;
    }
    if ((mask & (1u << i)) == 0u) {
      continue;
    }
    let agent = agents[i];
    let color = colors[i];
    let selfFlag = color.w;
    let radius = max(agent.z, 1.0);
    let depthCenter = selfFlag * 0.16 + 0.34;
    let delta = vec3f((pixel - agent.xy) / radius, (depth - depthCenter) * 6.0);
    let dist = max(length(delta), 0.08);
    let falloff = exp(-dist * mix(3.8, 1.45, selfFlag));
    let emitter = mix(color.rgb * 0.52, vec3f(7.0, 3.2, 0.55), selfFlag);
    writeLobe(&lighting, delta, emitter, falloff * mix(0.08, 0.52, selfFlag));
  }

  let outBase = lightingBase(index);
  nextLighting[outBase] = vec4f(max(lighting[0], vec3f(0.0)), 1.0);
  nextLighting[outBase + 1u] = vec4f(lighting[1] * 0.92, 1.0);
  nextLighting[outBase + 2u] = vec4f(lighting[2] * 0.92, 1.0);
  nextLighting[outBase + 3u] = vec4f(lighting[3] * 0.92, 1.0);
}
`;

const froxelFieldRenderShader = /* wgsl */ `
struct SimUniforms {
  time: f32,
  width: f32,
  height: f32,
  count: f32,
  froxelWidth: f32,
  froxelHeight: f32,
  froxelDepth: f32,
  pad: f32,
};

@group(0) @binding(0) var<storage, read> primitiveMasks: array<u32>;
@group(0) @binding(1) var<storage, read> agents: array<vec4f>;
@group(0) @binding(2) var<storage, read> colors: array<vec4f>;
@group(0) @binding(3) var<uniform> uniforms: SimUniforms;
@group(0) @binding(4) var<storage, read> environment: array<vec4f>;
@group(0) @binding(5) var<storage, read> shLighting: array<vec4f>;

struct VertexOut {
  @builtin(position) position: vec4f,
  @location(0) uv: vec2f,
};

@vertex
fn vertexMain(@builtin(vertex_index) vertexIndex: u32) -> VertexOut {
  let positions = array<vec2f, 3>(
    vec2f(-1.0, -1.0),
    vec2f(3.0, -1.0),
    vec2f(-1.0, 3.0)
  );
  let position = positions[vertexIndex];
  var out: VertexOut;
  out.position = vec4f(position, 0.0, 1.0);
  out.uv = position * vec2f(0.5, -0.5) + vec2f(0.5);
  return out;
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

fn primitiveMask(pixel: vec2f, progress: f32) -> u32 {
  let x = clamp(u32(pixel.x / max(uniforms.width, 1.0) * uniforms.froxelWidth), 0u, u32(uniforms.froxelWidth) - 1u);
  let y = clamp(u32(pixel.y / max(uniforms.height, 1.0) * uniforms.froxelHeight), 0u, u32(uniforms.froxelHeight) - 1u);
  let z = clamp(u32(progress * uniforms.froxelDepth), 0u, u32(uniforms.froxelDepth) - 1u);
  return primitiveMasks[z * u32(uniforms.froxelWidth) * u32(uniforms.froxelHeight) + y * u32(uniforms.froxelWidth) + x];
}

fn froxelIndex(pixel: vec2f, progress: f32) -> u32 {
  let x = clamp(u32(pixel.x / max(uniforms.width, 1.0) * uniforms.froxelWidth), 0u, u32(uniforms.froxelWidth) - 1u);
  let y = clamp(u32(pixel.y / max(uniforms.height, 1.0) * uniforms.froxelHeight), 0u, u32(uniforms.froxelHeight) - 1u);
  let z = clamp(u32(progress * uniforms.froxelDepth), 0u, u32(uniforms.froxelDepth) - 1u);
  return z * u32(uniforms.froxelWidth) * u32(uniforms.froxelHeight) + y * u32(uniforms.froxelWidth) + x;
}

fn environmentColor(direction: vec3f) -> vec3f {
  let up = environment[0].rgb;
  let horizon = environment[1].rgb;
  let down = environment[2].rgb;
  let key = environment[3].rgb;
  let vertical = clamp(direction.z * 0.5 + 0.5, 0.0, 1.0);
  let base = mix(down, up, vertical);
  let side = smoothstep(0.1, 0.82, abs(direction.x) + abs(direction.y) * 0.55);
  let keyLobe = pow(max(dot(normalize(direction), normalize(vec3f(0.36, -0.28, 0.89))), 0.0), 18.0);
  return mix(base, horizon, side * 0.38) + key * keyLobe * 0.22;
}

fn froxelRadiance(pixel: vec2f, progress: f32, direction: vec3f) -> vec3f {
  let base = froxelIndex(pixel, progress) * 4u;
  let dir = normalize(direction + vec3f(0.0001, 0.0002, 0.0003));
  let l0 = shLighting[base].rgb;
  let lx = shLighting[base + 1u].rgb;
  let ly = shLighting[base + 2u].rgb;
  let lz = shLighting[base + 3u].rgb;
  return max(l0 + lx * dir.x + ly * dir.y + lz * dir.z, vec3f(0.0));
}

@fragment
fn fragmentMain(input: VertexOut) -> @location(0) vec4f {
  let pixel = input.uv * vec2f(uniforms.width, uniforms.height);
  var transmittance = 1.0;
  var scattering = vec3f(0.0);
  var solid = vec3f(0.0);
  var solidAlpha = 0.0;
  var solidTransmittance = 1.0;
  let jitter = hash(dot(pixel, vec2f(0.067, 0.131)) + uniforms.time * 3.1);

  for (var step = 0u; step < 32u; step = step + 1u) {
    let progress = (f32(step) + jitter) / 32.0;
    let mask = primitiveMask(pixel, progress);
    var density = 0.0;
    var tint = vec3f(0.0);

    for (var i = 0u; i < ${maxStardustAgents}u; i = i + 1u) {
      if (f32(i) >= uniforms.count) {
        break;
      }
      if ((mask & (1u << i)) == 0u) {
        continue;
      }
      let agent = agents[i];
      let color = colors[i];
      let selfFlag = color.w;
      let radius = agent.z;
      let depthCenter = selfFlag * 0.16 + 0.34;
      let local = vec3f((pixel - agent.xy) / max(radius, 0.001), (progress - depthCenter) * 6.0);
      let displacement = fbm4(vec4f(local * mix(0.72, 1.12, selfFlag), uniforms.time * mix(0.06, 0.16, selfFlag))) * mix(0.035, 0.14, selfFlag);
      let sdf = length(local) - (1.0 + displacement);
      let plasma = pow(max(fbm4(vec4f(local * mix(1.35, 2.15, selfFlag), uniforms.time * 0.24)) * 0.5 + 0.5, 0.0), mix(2.4, 5.4, selfFlag));
      if (sdf < 0.016) {
        let normal = normalize(local);
        let viewDir = normalize(vec3f(0.0, 0.0, 1.35) - local);
        let reflected = reflect(-viewDir, normal);
        let fresnel = pow(1.0 - clamp(dot(normal, viewDir), 0.0, 1.0), 4.0);
        let studioReflection = environmentColor(reflected);
        let volumeLight = froxelRadiance(pixel, progress, normal);
        let diffuseWrap = (environmentColor(normal) * 0.08 + volumeLight * 0.28) * (0.12 + 0.1 * color.rgb);
        let chrome = diffuseWrap + mix(color.rgb * 0.18, studioReflection, 0.82 + fresnel * 0.16);
        let solar = vec3f(4.2, 2.1, 0.55) * (0.8 + plasma * 1.5);
        solid = mix(chrome, solar, selfFlag);
        solidAlpha = 0.92;
        solidTransmittance = transmittance;
        break;
      }
      let atmosphere = exp(-max(sdf, 0.0) * mix(4.6, 2.25, selfFlag));
      let localDensity = atmosphere * (0.004 + radius / 520.0 + selfFlag * 0.055) * (0.55 + plasma * mix(0.42, 1.45, selfFlag));
      density += localDensity;
      tint += localDensity * mix(color.rgb, vec3f(3.8, 1.85, 0.42), selfFlag);
    }

    if (solidAlpha > 0.5) {
      break;
    }

    let stepSize = 1.0 / 32.0;
    let extinction = density * 4.6;
    let stepTransmittance = exp(-extinction * stepSize);
    let lighting = froxelRadiance(pixel, progress, vec3f(input.uv - vec2f(0.5), 0.72));
    let luminance = tint * (0.38 + lighting * 0.72);
    scattering += transmittance * (luminance - luminance * stepTransmittance) / max(extinction, 0.0001);
    transmittance *= stepTransmittance;
  }

  let fogAlpha = clamp(1.0 - transmittance, 0.0, 0.72);
  let color = solid * solidAlpha * solidTransmittance + scattering;
  return vec4f(color, max(fogAlpha, solidAlpha));
}
`;

export interface AquariumStardustOverlay {
  dispose(): void;
  setProjections(projections: StardustProjection[]): void;
}

export async function createAquariumStardustOverlay(canvas: HTMLCanvasElement): Promise<AquariumStardustOverlay | null> {
  const gpu = (navigator as unknown as { gpu?: GpuApi }).gpu;
  if (!gpu) {
    canvas.dataset.stardustParticles = "three-scene";
    canvas.dataset.stardustMode = "webgpu-unavailable";
    return null;
  }
  const adapter = await gpu.requestAdapter();
  if (!adapter) {
    canvas.dataset.stardustParticles = "three-scene";
    canvas.dataset.stardustMode = "webgpu-no-adapter";
    return null;
  }
  const device = await adapter.requestDevice();
  const context = canvas.getContext("webgpu");
  if (!context) {
    canvas.dataset.stardustParticles = "three-scene";
    canvas.dataset.stardustMode = "webgpu-no-context";
    return null;
  }
  return new WebGpuFroxelFieldOverlay(canvas, device, context);
}

class WebGpuFroxelFieldOverlay implements AquariumStardustOverlay {
  private agentData = new Float32Array(maxStardustAgents * 4);
  private agentsBuffer: any;
  private colorsBuffer: any;
  private colorData = new Float32Array(maxStardustAgents * 4);
  private computeBindGroup: any;
  private computePipeline: any;
  private disposed = false;
  private environmentBuffer: any;
  private format: any;
  private lightingBindGroups: any[] = [];
  private lightingBuffers: any[] = [];
  private lightingPipeline: any;
  private lightingReadIndex = 0;
  private maskBuffer: any;
  private pipeline: any;
  private raf = 0;
  private renderBindGroups: any[] = [];
  private uniforms = new Float32Array(8);
  private uniformBuffer: any;

  constructor(private canvas: HTMLCanvasElement, private device: any, private context: any) {
    const gpuBufferUsage = (globalThis as any).GPUBufferUsage;
    this.format = (navigator as any).gpu.getPreferredCanvasFormat();
    this.canvas.style.opacity = "1";
    this.canvas.style.zIndex = "2";
    this.context.configure({ alphaMode: "premultiplied", device, format: this.format });
    const computeModule = device.createShaderModule({ label: "froxel primitive mask compute", code: froxelMaskComputeShader });
    const lightingModule = device.createShaderModule({ label: "froxel SH lighting compute", code: froxelLightingComputeShader });
    const renderModule = device.createShaderModule({ label: "froxel field renderer", code: froxelFieldRenderShader });
    this.maskBuffer = device.createBuffer({
      label: "froxel primitive masks",
      size: froxelCount * 4,
      usage: gpuBufferUsage.STORAGE,
    });
    this.agentsBuffer = device.createBuffer({
      label: "froxel agents",
      size: maxStardustAgents * 4 * 4,
      usage: gpuBufferUsage.STORAGE | gpuBufferUsage.COPY_DST,
    });
    this.colorsBuffer = device.createBuffer({
      label: "froxel colors",
      size: maxStardustAgents * 4 * 4,
      usage: gpuBufferUsage.STORAGE | gpuBufferUsage.COPY_DST,
    });
    this.environmentBuffer = device.createBuffer({
      label: "studio HDR environment summary",
      size: studioEnvironmentData.byteLength,
      usage: gpuBufferUsage.STORAGE | gpuBufferUsage.COPY_DST,
    });
    this.lightingBuffers = [0, 1].map((index) => device.createBuffer({
      label: `froxel spherical harmonics lighting ${index}`,
      size: froxelCount * froxelShStrideFloats * 4,
      usage: gpuBufferUsage.STORAGE,
    }));
    this.uniformBuffer = device.createBuffer({
      label: "froxel uniforms",
      size: this.uniforms.byteLength,
      usage: gpuBufferUsage.UNIFORM | gpuBufferUsage.COPY_DST,
    });
    this.computePipeline = device.createComputePipeline({
      label: "build froxel primitive masks",
      layout: "auto",
      compute: { module: computeModule, entryPoint: "buildPrimitiveMasks" },
    });
    this.lightingPipeline = device.createComputePipeline({
      label: "propagate froxel SH lighting",
      layout: "auto",
      compute: { module: lightingModule, entryPoint: "propagateLighting" },
    });
    this.pipeline = device.createRenderPipeline({
      label: "render froxel field",
      layout: "auto",
      vertex: { module: renderModule, entryPoint: "vertexMain" },
      fragment: {
        module: renderModule,
        entryPoint: "fragmentMain",
        targets: [{ format: this.format, blend: alphaBlend() }],
      },
      primitive: { topology: "triangle-list" },
    });
    this.computeBindGroup = device.createBindGroup({
      label: "froxel compute bindings",
      layout: this.computePipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.maskBuffer } },
        { binding: 1, resource: { buffer: this.agentsBuffer } },
        { binding: 2, resource: { buffer: this.uniformBuffer } },
      ],
    });
    this.lightingBindGroups = [0, 1].map((readIndex) => device.createBindGroup({
      label: `froxel SH lighting bindings ${readIndex}`,
      layout: this.lightingPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.maskBuffer } },
        { binding: 1, resource: { buffer: this.agentsBuffer } },
        { binding: 2, resource: { buffer: this.colorsBuffer } },
        { binding: 3, resource: { buffer: this.environmentBuffer } },
        { binding: 4, resource: { buffer: this.lightingBuffers[readIndex] } },
        { binding: 5, resource: { buffer: this.lightingBuffers[1 - readIndex] } },
        { binding: 6, resource: { buffer: this.uniformBuffer } },
      ],
    }));
    this.renderBindGroups = [0, 1].map((lightingIndex) => device.createBindGroup({
      label: `froxel render bindings ${lightingIndex}`,
      layout: this.pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.maskBuffer } },
        { binding: 1, resource: { buffer: this.agentsBuffer } },
        { binding: 2, resource: { buffer: this.colorsBuffer } },
        { binding: 3, resource: { buffer: this.uniformBuffer } },
        { binding: 4, resource: { buffer: this.environmentBuffer } },
        { binding: 5, resource: { buffer: this.lightingBuffers[lightingIndex] } },
      ],
    }));
    this.device.queue.writeBuffer(this.environmentBuffer, 0, studioEnvironmentData);
    this.canvas.dataset.stardustMode = "webgpu-froxel-sh-primitive-map";
    this.canvas.dataset.stardustParticles = String(froxelCount);
    this.raf = requestAnimationFrame(this.render);
  }

  dispose() {
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    this.maskBuffer?.destroy?.();
    this.agentsBuffer?.destroy?.();
    this.colorsBuffer?.destroy?.();
    this.environmentBuffer?.destroy?.();
    this.lightingBuffers.forEach((buffer) => buffer?.destroy?.());
    this.uniformBuffer?.destroy?.();
  }

  setProjections(projections: StardustProjection[]) {
    const width = Math.max(this.canvas.width, 1);
    const height = Math.max(this.canvas.height, 1);
    this.agentData.fill(0);
    this.colorData.fill(0);
    projections.slice(0, maxStardustAgents).forEach((projection, index) => {
      const isSelf = projection.id === "coordinator" ? 1 : 0;
      const radius = (18 + projection.z * 16 + projection.expression * 14 + projection.acknowledgement * 20) * (isSelf ? 1.45 : 1);
      this.agentData[index * 4] = (projection.xPercent / 100) * width;
      this.agentData[index * 4 + 1] = (projection.yPercent / 100) * height;
      this.agentData[index * 4 + 2] = radius;
      this.agentData[index * 4 + 3] = isSelf;
      const color = parseColor(projection.color ?? "#8fffd3");
      const glow = parseColor(projection.glow ?? projection.color ?? "#8fffd3");
      this.colorData[index * 4] = isSelf ? 1.0 : (color[0] + glow[0]) * 0.5;
      this.colorData[index * 4 + 1] = isSelf ? 0.74 : (color[1] + glow[1]) * 0.5;
      this.colorData[index * 4 + 2] = isSelf ? 0.28 : (color[2] + glow[2]) * 0.5;
      this.colorData[index * 4 + 3] = isSelf;
    });
    this.uniforms[3] = Math.min(maxStardustAgents, projections.length);
    this.device.queue.writeBuffer(this.agentsBuffer, 0, this.agentData);
    this.device.queue.writeBuffer(this.colorsBuffer, 0, this.colorData);
  }

  private render = (millis: number) => {
    if (this.disposed) return;
    const rect = this.canvas.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 1.25);
    const width = Math.max(1, Math.floor(rect.width * dpr));
    const height = Math.max(1, Math.floor(rect.height * dpr));
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
    this.uniforms[0] = millis / 1000;
    this.uniforms[1] = width;
    this.uniforms[2] = height;
    this.uniforms[4] = froxelWidth;
    this.uniforms[5] = froxelHeight;
    this.uniforms[6] = froxelDepth;
    this.device.queue.writeBuffer(this.uniformBuffer, 0, this.uniforms);
    const encoder = this.device.createCommandEncoder({ label: "froxel field frame" });
    const computePass = encoder.beginComputePass();
    computePass.setPipeline(this.computePipeline);
    computePass.setBindGroup(0, this.computeBindGroup);
    computePass.dispatchWorkgroups(Math.ceil(froxelCount / 128));
    computePass.end();
    const lightingPass = encoder.beginComputePass();
    lightingPass.setPipeline(this.lightingPipeline);
    lightingPass.setBindGroup(0, this.lightingBindGroups[this.lightingReadIndex]);
    lightingPass.dispatchWorkgroups(Math.ceil(froxelCount / 128));
    lightingPass.end();
    const lightingWriteIndex = 1 - this.lightingReadIndex;
    const pass = encoder.beginRenderPass({
      colorAttachments: [{
        clearValue: { r: 0, g: 0, b: 0, a: 0 },
        loadOp: "clear",
        storeOp: "store",
        view: this.context.getCurrentTexture().createView(),
      }],
    });
    pass.setPipeline(this.pipeline);
    pass.setBindGroup(0, this.renderBindGroups[lightingWriteIndex]);
    pass.draw(3);
    pass.end();
    this.device.queue.submit([encoder.finish()]);
    this.lightingReadIndex = lightingWriteIndex;
    this.raf = requestAnimationFrame(this.render);
  };
}

class WebGpuStardustOverlay implements AquariumStardustOverlay {
  private agentData = new Float32Array(maxStardustAgents * 4);
  private agentsBuffer: any;
  private bindGroup: any;
  private computeBindGroup: any;
  private computePipeline: any;
  private disposed = false;
  private format: any;
  private hdrSize = { height: 0, width: 0 };
  private hdrTexture: any = null;
  private hdrView: any = null;
  private lastMillis = performance.now();
  private lastRenderMillis = 0;
  private particleBuffer: any;
  private postBindGroup: any = null;
  private postPipeline: any;
  private previousAgents = new Map<string, { x: number; y: number; time: number }>();
  private raf = 0;
  private renderPipeline: any;
  private sampler: any;
  private uniforms = new Float32Array(8);
  private uniformBuffer: any;

  constructor(private canvas: HTMLCanvasElement, private device: any, private context: any) {
    const gpuBufferUsage = (globalThis as any).GPUBufferUsage;
    this.format = (navigator as any).gpu.getPreferredCanvasFormat();
    this.context.configure({
      alphaMode: "premultiplied",
      device,
      format: this.format,
    });

    const computeModule = device.createShaderModule({ label: "Aetheria stardust compute", code: stardustComputeShader });
    const renderModule = device.createShaderModule({ label: "Aetheria stardust render", code: stardustRenderShader });
    const postModule = device.createShaderModule({ label: "Aetheria stardust ACES post", code: stardustPostShader });
    this.particleBuffer = device.createBuffer({
      label: "stardust particles",
      size: particleCount * particleStrideFloats * 4,
      usage: gpuBufferUsage.STORAGE,
    });
    this.agentsBuffer = device.createBuffer({
      label: "stardust agents",
      size: maxStardustAgents * 4 * 4,
      usage: gpuBufferUsage.STORAGE | gpuBufferUsage.COPY_DST,
    });
    this.uniformBuffer = device.createBuffer({
      label: "stardust uniforms",
      size: this.uniforms.byteLength,
      usage: gpuBufferUsage.UNIFORM | gpuBufferUsage.COPY_DST,
    });
    this.computePipeline = device.createComputePipeline({
      label: "stardust flow update",
      layout: "auto",
      compute: { module: computeModule, entryPoint: "updateParticles" },
    });
    this.renderPipeline = device.createRenderPipeline({
      label: "stardust hdr draw",
      layout: "auto",
      vertex: { module: renderModule, entryPoint: "vertexMain" },
      fragment: {
        module: renderModule,
        entryPoint: "fragmentMain",
        targets: [{ format: "rgba16float", blend: additiveBlend() }],
      },
      primitive: { topology: "triangle-list" },
    });
    this.postPipeline = device.createRenderPipeline({
      label: "stardust ACES tonemap",
      layout: "auto",
      vertex: { module: postModule, entryPoint: "vertexMain" },
      fragment: {
        module: postModule,
        entryPoint: "fragmentMain",
        targets: [{ format: this.format, blend: alphaBlend() }],
      },
      primitive: { topology: "triangle-list" },
    });
    this.sampler = device.createSampler({
      addressModeU: "clamp-to-edge",
      addressModeV: "clamp-to-edge",
      magFilter: "linear",
      minFilter: "linear",
    });
    this.bindGroup = device.createBindGroup({
      label: "stardust bindings",
      layout: this.renderPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.particleBuffer } },
        { binding: 2, resource: { buffer: this.uniformBuffer } },
      ],
    });
    const computeBindGroup = device.createBindGroup({
      label: "stardust compute bindings",
      layout: this.computePipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: this.particleBuffer } },
        { binding: 1, resource: { buffer: this.agentsBuffer } },
        { binding: 2, resource: { buffer: this.uniformBuffer } },
      ],
    });
    this.computeBindGroup = computeBindGroup;
    this.raf = requestAnimationFrame(this.render);
  }

  dispose() {
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    this.particleBuffer?.destroy?.();
    this.agentsBuffer?.destroy?.();
    this.hdrTexture?.destroy?.();
    this.uniformBuffer?.destroy?.();
  }

  setProjections(projections: StardustProjection[]) {
    const now = performance.now() / 1000;
    const width = Math.max(this.canvas.width, 1);
    const height = Math.max(this.canvas.height, 1);
    this.agentData.fill(0);
    projections.slice(0, maxStardustAgents).forEach((projection, index) => {
      const x = (projection.xPercent / 100) * width;
      const y = (projection.yPercent / 100) * height;
      const previous = this.previousAgents.get(projection.id);
      const dt = previous ? Math.max(now - previous.time, 0.001) : 1 / 60;
      const vx = previous ? (x - previous.x) / dt : 0;
      const vy = previous ? (y - previous.y) / dt : 0;
      this.agentData[index * 4] = x;
      this.agentData[index * 4 + 1] = y;
      this.agentData[index * 4 + 2] = clamp(vx, -260, 260);
      this.agentData[index * 4 + 3] = clamp(vy, -260, 260);
      this.previousAgents.set(projection.id, { x, y, time: now });
    });
    this.uniforms[4] = Math.min(maxStardustAgents, projections.length);
    this.device.queue.writeBuffer(this.agentsBuffer, 0, this.agentData);
  }

  private render = (millis: number) => {
    if (this.disposed) return;
    if (millis - this.lastRenderMillis < 33) {
      this.raf = requestAnimationFrame(this.render);
      return;
    }
    this.lastRenderMillis = millis;
    const rect = this.canvas.getBoundingClientRect();
    const dpr = Math.min(window.devicePixelRatio || 1, 1.5);
    const width = Math.max(1, Math.floor(rect.width * dpr));
    const height = Math.max(1, Math.floor(rect.height * dpr));
    if (this.canvas.width !== width || this.canvas.height !== height) {
      this.canvas.width = width;
      this.canvas.height = height;
    }
    this.ensureHdrTarget(width, height);
    const dt = (millis - this.lastMillis) / 1000;
    this.lastMillis = millis;
    this.uniforms[0] = millis / 1000;
    this.uniforms[1] = dt;
    this.uniforms[2] = width;
    this.uniforms[3] = height;
    this.uniforms[5] = particleCount;
    this.uniforms[6] = 1;
    this.uniforms[7] = 0.18;
    this.device.queue.writeBuffer(this.uniformBuffer, 0, this.uniforms);
    this.canvas.dataset.stardustParticles = String(particleCount);

    const encoder = this.device.createCommandEncoder({ label: "stardust frame" });
    const computePass = encoder.beginComputePass();
    computePass.setPipeline(this.computePipeline);
    computePass.setBindGroup(0, this.computeBindGroup);
    computePass.dispatchWorkgroups(Math.ceil(particleCount / 128));
    computePass.end();

    const hdrPass = encoder.beginRenderPass({
      colorAttachments: [
        {
          clearValue: { r: 0, g: 0, b: 0, a: 0 },
          loadOp: "clear",
          storeOp: "store",
          view: this.hdrView,
        },
      ],
    });
    hdrPass.setPipeline(this.renderPipeline);
    hdrPass.setBindGroup(0, this.bindGroup);
    hdrPass.draw(6, particleCount);
    hdrPass.end();

    const view = this.context.getCurrentTexture().createView();
    const postPass = encoder.beginRenderPass({
      colorAttachments: [
        {
          clearValue: { r: 0, g: 0, b: 0, a: 0 },
          loadOp: "clear",
          storeOp: "store",
          view,
        },
      ],
    });
    postPass.setPipeline(this.postPipeline);
    postPass.setBindGroup(0, this.postBindGroup);
    postPass.draw(3);
    postPass.end();

    this.device.queue.submit([encoder.finish()]);
    this.raf = requestAnimationFrame(this.render);
  };

  private ensureHdrTarget(width: number, height: number) {
    if (this.hdrTexture && this.hdrSize.width === width && this.hdrSize.height === height) return;
    this.hdrTexture?.destroy?.();
    this.hdrSize = { width, height };
    this.hdrTexture = this.device.createTexture({
      label: "stardust hdr target",
      size: { width, height },
      format: "rgba16float",
      usage: (globalThis as any).GPUTextureUsage.RENDER_ATTACHMENT | (globalThis as any).GPUTextureUsage.TEXTURE_BINDING,
    });
    this.hdrView = this.hdrTexture.createView();
    this.postBindGroup = this.device.createBindGroup({
      label: "stardust post bindings",
      layout: this.postPipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: this.sampler },
        { binding: 1, resource: this.hdrView },
      ],
    });
  }
}

function additiveBlend() {
  return {
    alpha: {
      dstFactor: "one-minus-src-alpha",
      operation: "add",
      srcFactor: "one",
    },
    color: {
      dstFactor: "one",
      operation: "add",
      srcFactor: "src-alpha",
    },
  };
}

function alphaBlend() {
  return {
    alpha: {
      dstFactor: "one-minus-src-alpha",
      operation: "add",
      srcFactor: "one",
    },
    color: {
      dstFactor: "one-minus-src-alpha",
      operation: "add",
      srcFactor: "src-alpha",
    },
  };
}

function parseColor(value: string): [number, number, number] {
  const hex = value.trim().replace(/^#/, "");
  const normalized = hex.length === 3
    ? hex.split("").map((char) => `${char}${char}`).join("")
    : hex.padEnd(6, "0").slice(0, 6);
  const int = Number.parseInt(normalized, 16);
  if (!Number.isFinite(int)) return [0.55, 1, 0.83];
  return [
    ((int >> 16) & 255) / 255,
    ((int >> 8) & 255) / 255,
    (int & 255) / 255,
  ];
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
