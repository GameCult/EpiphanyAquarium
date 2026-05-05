import type { AquariumAgentProjection } from "./aquariumFluid";

type StardustProjection = AquariumAgentProjection & {
  color?: string;
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

export interface AquariumStardustOverlay {
  dispose(): void;
  setProjections(projections: StardustProjection[]): void;
}

export async function createAquariumStardustOverlay(canvas: HTMLCanvasElement): Promise<AquariumStardustOverlay | null> {
  const gpu = (navigator as Navigator & { gpu?: GpuApi }).gpu;
  if (!gpu) return null;
  const adapter = await gpu.requestAdapter();
  if (!adapter) return null;
  const device = await adapter.requestDevice();
  const context = canvas.getContext("webgpu") as any;
  if (!context) return null;
  return new WebGpuStardustOverlay(canvas, device, context);
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

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}
