import * as THREE from "three";
import type { AquariumAgentProjection } from "./aquariumFluid";

type SceneProjection = AquariumAgentProjection & {
  color?: string;
  glow?: string;
};

type PointerState = {
  active: boolean;
  xPercent: number;
  yPercent: number;
};

type CameraDragMode = "orbit" | "pan";

export type ProjectLabelProjection = {
  id: string;
  label: string;
  subLabel?: string;
  opacity: number;
  scale: number;
  xPercent: number;
  yPercent: number;
};

export interface AquariumScene3d {
  dispose(): void;
  projectProjectLabels(labels: Array<{ id: string; label: string; subLabel?: string }>): ProjectLabelProjection[];
  projectPointerToGrid(pointer: PointerState): { xPercent: number; yPercent: number } | null;
  projectProjections(projections: SceneProjection[]): SceneProjection[];
  pointerDown(pointer: PointerState, button: number): void;
  pointerMove(pointer: PointerState): void;
  pointerUp(): void;
  setPointer(pointer: PointerState): void;
  setProjections(projections: SceneProjection[]): void;
  wheel(deltaY: number): void;
}

const worldWidth = 10;
const worldDepth = 7.2;
const mathTau = Math.PI * 2;
const stardustParticleCount = new URLSearchParams(globalThis.location?.search ?? "").has("smoke") ? 48_000 : 1_000_000;
const maxStardustSources = 8;

export function createAquariumScene3d(canvas: HTMLCanvasElement): AquariumScene3d {
  return new ThreeAquariumScene(canvas);
}

class ThreeAquariumScene implements AquariumScene3d {
  private agentGroups = new Map<string, THREE.Group>();
  private camera = new THREE.PerspectiveCamera(42, 1, 0.1, 80);
  private cameraDistance = 10;
  private cameraPitch = 0.68;
  private cameraTarget = new THREE.Vector3(0, 0, 0);
  private cameraYaw = 0;
  private cursor = new THREE.Group();
  private cursorSplat: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>;
  private dragMode: CameraDragMode | null = null;
  private dragPointer: PointerState | null = null;
  private disposed = false;
  private gravityCamera = new THREE.OrthographicCamera(-worldWidth / 2, worldWidth / 2, worldDepth / 2, -worldDepth / 2, 0.1, 10);
  private gravityRenderTarget = new THREE.WebGLRenderTarget(256, 256, {
    depthBuffer: false,
    format: THREE.RGBAFormat,
    magFilter: THREE.LinearFilter,
    minFilter: THREE.LinearFilter,
    stencilBuffer: false,
    type: THREE.HalfFloatType,
  });
  private gravityScene = new THREE.Scene();
  private gravityUniforms = {
    uCellSize: { value: 0.34 },
    uFieldHalfSize: { value: new THREE.Vector2(worldWidth / 2, worldDepth / 2) },
    uGridColor: { value: new THREE.Color(0x69ffd8) },
    uGravitySize: { value: new THREE.Vector2(worldWidth, worldDepth) },
    uGravityTex: { value: null as THREE.Texture | null },
    uOpacity: { value: 0.42 },
    uTime: { value: 0 },
  };
  private gridGroup!: THREE.Group;
  private pointer: PointerState = { active: false, xPercent: 50, yPercent: 50 };
  private pointerWorld = new THREE.Vector3(0, 0, 0);
  private raf = 0;
  private raycaster = new THREE.Raycaster();
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private splatMeshes: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>[] = [];
  private stardustMaterial!: THREE.ShaderMaterial;
  private worldPlane = new THREE.Plane(new THREE.Vector3(0, 0, 1), 0);
  private readonly handleKeyDown = (event: KeyboardEvent) => this.keyPan(event);

  constructor(private canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({
      alpha: true,
      antialias: true,
      canvas,
      premultipliedAlpha: true,
      preserveDrawingBuffer: true,
    });
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.18;
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.6));
    this.gravityUniforms.uGravityTex.value = this.gravityRenderTarget.texture;
    this.camera.up.set(0, 0, 1);
    this.gravityCamera.position.set(0, 0, 5);
    this.gravityCamera.lookAt(0, 0, 0);
    this.createSplatPool(48);
    this.cursorSplat = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), this.createSplatMaterial());
    this.cursorSplat.frustumCulled = false;
    this.cursorSplat.visible = false;
    this.gravityScene.add(this.cursorSplat);
    this.updateCamera();
    this.scene.add(new THREE.AmbientLight(0xbfffe8, 0.74));
    const key = new THREE.DirectionalLight(0xd8fff0, 1.2);
    key.position.set(-4.5, 8, 5.2);
    this.scene.add(key);
    this.gridGroup = this.createGrid();
    this.scene.add(this.gridGroup);
    this.stardustMaterial = this.createStardustMaterial();
    this.scene.add(this.createStardust());
    this.scene.add(this.createCursor());
    window.addEventListener("keydown", this.handleKeyDown);
    this.raf = requestAnimationFrame(this.render);
  }

  dispose() {
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    window.removeEventListener("keydown", this.handleKeyDown);
    this.scene.traverse((object) => {
      if (object instanceof THREE.Mesh || object instanceof THREE.LineSegments || object instanceof THREE.Line) {
        object.geometry?.dispose();
        const materials = Array.isArray(object.material) ? object.material : [object.material];
        materials.forEach((material) => material.dispose());
      }
    });
    this.gravityScene.traverse((object) => {
      if (object instanceof THREE.Mesh) {
        object.geometry?.dispose();
        object.material?.dispose();
      }
    });
    this.gravityRenderTarget.dispose();
    this.renderer.dispose();
  }

  setPointer(pointer: PointerState) {
    this.pointer = pointer;
  }

  pointerDown(pointer: PointerState, button: number) {
    this.pointer = pointer;
    this.dragPointer = pointer;
    this.dragMode = button === 1 ? "orbit" : button === 2 ? "pan" : null;
  }

  pointerMove(pointer: PointerState) {
    if (this.dragMode && this.dragPointer) {
      const dx = pointer.xPercent - this.dragPointer.xPercent;
      const dy = pointer.yPercent - this.dragPointer.yPercent;
      if (this.dragMode === "orbit") {
        this.cameraYaw -= dx * 0.012;
        this.cameraPitch = clamp(this.cameraPitch + dy * 0.008, 0.18, 1.38);
      } else {
        this.panCamera(this.dragPointer, pointer);
      }
      this.updateCamera();
      this.dragPointer = pointer;
    }
    this.pointer = pointer;
  }

  pointerUp() {
    this.dragMode = null;
    this.dragPointer = null;
  }

  projectPointerToGrid(pointer: PointerState) {
    const projected = this.projectPointerToPlane(pointer);
    if (!projected) return null;
    return worldToGridPercent(projected.x, projected.y);
  }

  projectProjectLabels(labels: Array<{ id: string; label: string; subLabel?: string }>) {
    const zoomOpacity = smoothstep(8.5, 15.5, this.cameraDistance);
    const centroid = this.agentCentroid();
    return labels.map((label) => {
      const point = centroid.clone();
      const screen = this.projectWorldToScreen(point);
      return {
        ...label,
        opacity: zoomOpacity,
        scale: screen.scale * (0.82 + zoomOpacity * 0.16),
        xPercent: screen.xPercent,
        yPercent: screen.yPercent,
      };
    });
  }

  private agentCentroid() {
    const centroid = new THREE.Vector3();
    let count = 0;
    for (const group of this.agentGroups.values()) {
      centroid.add(group.position);
      count += 1;
    }
    if (count === 0) return new THREE.Vector3(0, 0, 1.25);
    centroid.multiplyScalar(1 / count);
    return centroid;
  }

  projectProjections(projections: SceneProjection[]) {
    return projections.map((projection) => {
      const target = gridToWorld(projection.gridXPercent, projection.gridYPercent);
      const height = this.agentHeight(projection);
      const body = this.agentGroups.get(projection.id)?.position.clone() ?? new THREE.Vector3(target.x, target.y, height);
      const screen = this.projectWorldToScreen(body);
      const right = this.cameraRight();
      const up = this.cameraUp();
      const side = screen.xPercent > 62 ? -1 : screen.xPercent < 38 ? 1 : screen.xPercent > 50 ? -1 : 1;
      const focus = this.projectWorldToScreen(
        body.clone()
          .addScaledVector(right, side * 2.35)
          .addScaledVector(up, 1.04),
      );
      const thought = this.projectWorldToScreen(
        body.clone()
          .addScaledVector(right, side * 0.44)
          .addScaledVector(up, 1.02),
      );
      const halo = this.projectWorldToScreen(
        body.clone(),
      );
      return {
        ...projection,
        billboardXPercent: halo.xPercent,
        billboardYPercent: halo.yPercent,
        billboardScale: halo.scale,
        focusXPercent: focus.xPercent,
        focusYPercent: focus.yPercent,
        focusScale: focus.scale,
        thoughtXPercent: thought.xPercent,
        thoughtYPercent: thought.yPercent,
        thoughtScale: thought.scale,
        xPercent: screen.xPercent,
        yPercent: screen.yPercent,
        screenScale: screen.scale,
      };
    });
  }

  wheel(deltaY: number) {
    this.cameraDistance = clamp(this.cameraDistance * Math.exp(deltaY * 0.001), 4.8, 18);
    this.updateCamera();
  }

  setProjections(projections: SceneProjection[]) {
    const live = new Set<string>();
    let splatIndex = 0;
    let sourceIndex = 0;
    const selfProjection = projections.find((projection) => projection.id === "coordinator") ?? projections[0];
    if (selfProjection && splatIndex < this.splatMeshes.length) {
      const selfTarget = gridToWorld(selfProjection.gridXPercent, selfProjection.gridYPercent);
      this.configureSplat(this.splatMeshes[splatIndex], selfTarget.x, selfTarget.y, 4.25, 0.7, 2.35, 0, 0, 1.25);
      splatIndex += 1;
    }
    for (const projection of projections) {
      live.add(projection.id);
      const group = this.agentGroups.get(projection.id) ?? this.createAgent(projection);
      const target = gridToWorld(projection.gridXPercent, projection.gridYPercent);
      const height = this.agentHeight(projection);
      group.position.set(target.x, target.y, height);
      group.scale.setScalar(0.9 + projection.z * 0.22 + projection.hover * 0.08);
      group.rotation.set(0.18 + projection.expression * 0.04, 0, projection.tilt * 0.01);
      const cup = group.userData.cup as THREE.Mesh | undefined;
      if (cup?.material instanceof THREE.MeshBasicMaterial) {
        cup.material.opacity = 0.46 + projection.hover * 0.22 + projection.acknowledgement * 0.18;
      }
      if (sourceIndex < maxStardustSources) {
        this.updateStardustSource(sourceIndex, target.x, target.y, projection);
        sourceIndex += 1;
      }
      if (splatIndex < this.splatMeshes.length) {
        const strength = 0.46 + projection.z * 0.74 + projection.expression * 0.16;
        const radius = 1.65 + projection.acknowledgement * 0.34 + projection.hover * 0.26;
        this.configureSplat(this.splatMeshes[splatIndex], target.x, target.y, radius, strength, 2.18, 0, 0, 1.25);
        splatIndex += 1;
      }
      if (splatIndex < this.splatMeshes.length) {
        const bands = projection.chirpBank.length > 0
          ? projection.chirpBank
          : [[0.28, 0.035 + projection.glowPulse * 0.01, stablePhase(projection.id), 0.01] as const];
        for (let bandIndex = 0; bandIndex < bands.length && splatIndex < this.splatMeshes.length; bandIndex += 1) {
          const [temporalFrequency, amplitude, phase, chirp] = bands[bandIndex];
          const wave = spectralGridWave(
            temporalFrequency,
            amplitude,
            chirp,
            bandIndex,
            bands.length,
            projection.z,
            projection.hover,
            projection.acknowledgement,
          );
          this.configureSplat(this.splatMeshes[splatIndex], target.x, target.y, wave.radius, wave.depth, 8, wave.spatialFrequency, phase + bandIndex * 0.7, wave.sinePower, wave.speed);
          splatIndex += 1;
        }
      }
    }
    for (let index = splatIndex; index < this.splatMeshes.length; index += 1) {
      this.splatMeshes[index].visible = false;
    }
    for (let index = sourceIndex; index < maxStardustSources; index += 1) {
      this.updateStardustSource(index, 999, 999, { expression: 0, acknowledgement: 0, hover: 0 } as SceneProjection);
    }
    for (const [id, group] of this.agentGroups) {
      if (!live.has(id)) {
        this.scene.remove(group);
        this.agentGroups.delete(id);
      }
    }
  }

  private createGrid() {
    const group = new THREE.Group();
    const geometry = new THREE.PlaneGeometry(worldWidth, worldDepth, 96, 72);
    const field = new THREE.Mesh(geometry, this.createGravityGridMaterial(0.12, false));
    const wire = new THREE.Mesh(geometry.clone(), this.createGravityGridMaterial(0.46, true));
    wire.position.z = 0.018;
    group.add(field, wire);
    return group;
  }

  private createGravityGridMaterial(opacity: number, wireframe: boolean) {
    return new THREE.ShaderMaterial({
      uniforms: {
        ...this.gravityUniforms,
        uOpacity: { value: opacity },
      },
      vertexShader: `
        uniform sampler2D uGravityTex;
        uniform vec2 uFieldHalfSize;
        uniform vec2 uGravitySize;
        varying vec2 vGridPosition;
        varying float vDepth;
        varying float vFade;

        void main() {
          vec3 displaced = position;
          vec2 worldPosition = (modelMatrix * vec4(position, 1.0)).xy;
          vGridPosition = worldPosition;
          vec2 uv = worldPosition / uGravitySize + 0.5;
          float gravityMask = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
          vec4 field = texture2D(uGravityTex, clamp(uv, vec2(0.0), vec2(1.0))) * gravityMask;
          float depth = field.r - field.g;
          displaced.z = -depth;
          float edge = max(abs(worldPosition.x) / max(uFieldHalfSize.x, 0.001), abs(worldPosition.y) / max(uFieldHalfSize.y, 0.001));
          vDepth = depth;
          vFade = (1.0 - smoothstep(0.72, 1.0, edge)) * (1.0 - smoothstep(0.65, 1.8, depth));
          gl_Position = projectionMatrix * modelViewMatrix * vec4(displaced, 1.0);
        }
      `,
      fragmentShader: `
        uniform vec3 uGridColor;
        uniform float uCellSize;
        uniform float uOpacity;
        varying vec2 vGridPosition;
        varying float vDepth;
        varying float vFade;

        void main() {
          float cup = smoothstep(0.02, 0.28, vDepth);
          vec2 cell = abs(fract(vGridPosition / max(uCellSize, 0.001) + 0.5) - 0.5);
          vec2 lineWidth = fwidth(vGridPosition / max(uCellSize, 0.001)) * 1.35;
          float line = 1.0 - min(smoothstep(0.0, lineWidth.x, cell.x), smoothstep(0.0, lineWidth.y, cell.y));
          vec3 color = mix(uGridColor * 0.38, vec3(0.82, 1.0, 0.9), cup);
          gl_FragColor = vec4(color, uOpacity * vFade * mix(0.45, 1.0, line));
        }
      `,
      transparent: true,
      depthWrite: false,
      side: THREE.DoubleSide,
      wireframe,
    });
  }

  private configureSplat(
    mesh: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>,
    x: number,
    y: number,
    radius: number,
    depth: number,
    power: number,
    frequency: number,
    phase: number,
    sinePower: number,
    speed = 0,
  ) {
    mesh.visible = true;
    mesh.position.set(x, y, 0);
    mesh.scale.set(radius * 2, radius * 2, 1);
    mesh.material.uniforms.uDepth.value = depth;
    mesh.material.uniforms.uFrequency.value = frequency;
    mesh.material.uniforms.uPhase.value = phase;
    mesh.material.uniforms.uPower.value = power;
    mesh.material.uniforms.uSinePower.value = sinePower;
    mesh.material.uniforms.uSpeed.value = speed;
  }

  private createSplatPool(count: number) {
    for (let index = 0; index < count; index += 1) {
      const mesh = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), this.createSplatMaterial());
      mesh.frustumCulled = false;
      mesh.visible = false;
      this.gravityScene.add(mesh);
      this.splatMeshes.push(mesh);
    }
  }

  private createSplatMaterial() {
    return new THREE.ShaderMaterial({
      uniforms: {
        uDepth: { value: 0 },
        uFrequency: { value: 0 },
        uPhase: { value: 0 },
        uPower: { value: 2 },
        uSinePower: { value: 1.25 },
        uSpeed: { value: 0 },
        uTime: this.gravityUniforms.uTime,
      },
      vertexShader: `
        varying vec2 vUv;
        void main() {
          vUv = uv;
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }
      `,
      fragmentShader: `
        uniform float uDepth;
        uniform float uFrequency;
        uniform float uPhase;
        uniform float uPower;
        uniform float uSinePower;
        uniform float uSpeed;
        uniform float uTime;
        varying vec2 vUv;

        float powerPulse(float x, float exponent) {
          x = clamp(abs(x), 0.0, 1.0);
          return pow((x + 1.0) * (1.0 - x), exponent);
        }

        void main() {
          float dist = length(vUv - vec2(0.5)) * 2.0;
          float envelope = powerPulse(dist, uPower) * smoothstep(1.0, 0.95, dist);
          float wave = uFrequency > 0.0 ? cos(pow(dist, uSinePower) * uFrequency + uPhase + uTime * uSpeed) : 1.0;
          float height = envelope * wave * uDepth;
          gl_FragColor = vec4(max(height, 0.0), max(-height, 0.0), abs(height), 1.0);
        }
      `,
      blending: THREE.AdditiveBlending,
      depthTest: false,
      depthWrite: false,
      transparent: true,
    });
  }

  private createCursor() {
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.22, 0.012, 8, 48),
      new THREE.MeshBasicMaterial({ color: 0xe9ffb0, opacity: 0.62, transparent: true, depthWrite: false }),
    );
    const beam = new THREE.Mesh(
      new THREE.CylinderGeometry(0.012, 0.028, 1.2, 10, 1, true),
      new THREE.MeshBasicMaterial({ color: 0x80ffd5, opacity: 0.18, transparent: true, depthWrite: false }),
    );
    beam.rotation.x = Math.PI / 2;
    beam.position.z = 0.6;
    this.cursor.add(ring, beam);
    this.cursor.visible = false;
    return this.cursor;
  }

  private createStardust() {
    const geometry = new THREE.BufferGeometry();
    const positions = new Float32Array(stardustParticleCount * 3);
    const seeds = new Float32Array(stardustParticleCount);
    for (let index = 0; index < stardustParticleCount; index += 1) {
      const offset = index * 3;
      positions[offset] = (hashNumber(index * 17 + 3) - 0.5) * worldWidth * 1.8;
      positions[offset + 1] = (hashNumber(index * 31 + 5) - 0.5) * worldDepth * 1.8;
      positions[offset + 2] = hashNumber(index * 43 + 7) * 2.8 + 0.12;
      seeds[index] = hashNumber(index * 97 + 11);
    }
    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute("aSeed", new THREE.BufferAttribute(seeds, 1));
    const points = new THREE.Points(geometry, this.stardustMaterial);
    points.frustumCulled = false;
    points.renderOrder = 4;
    return points;
  }

  private createStardustMaterial() {
    const sourceData = Array.from({ length: maxStardustSources }, () => new THREE.Vector4(999, 999, 0, 0));
    return new THREE.ShaderMaterial({
      uniforms: {
        uSources: { value: sourceData },
        uTime: this.gravityUniforms.uTime,
        uWorld: { value: new THREE.Vector2(worldWidth, worldDepth) },
      },
      vertexShader: `
        attribute float aSeed;
        uniform float uTime;
        uniform vec2 uWorld;
        uniform vec4 uSources[${maxStardustSources}];
        varying float vAlpha;
        varying vec3 vColor;

        float hash(float value) {
          return fract(sin(value * 12.9898) * 43758.5453);
        }

        vec2 wrapWorld(vec2 value) {
          vec2 size = uWorld * 1.8;
          return mod(value + size * 0.5, size) - size * 0.5;
        }

        void main() {
          vec3 p = position;
          float pair = floor(aSeed * 2048.0);
          float lifetime = fract(uTime * 0.11 + hash(pair));
          vec2 flow = vec2(
            sin(p.y * 1.7 + uTime * 0.23 + aSeed * 6.28318),
            cos(p.x * 1.4 - uTime * 0.19 + aSeed * 5.31)
          ) * 0.022;
          for (int i = 0; i < ${maxStardustSources}; i += 1) {
            vec4 source = uSources[i];
            vec2 delta = source.xy - p.xy;
            float influence = exp(-dot(delta, delta) * 0.42) * source.z;
            vec2 tangent = normalize(vec2(-delta.y, delta.x) + vec2(0.001, 0.0));
            flow += tangent * influence * 0.038 + source.w * normalize(delta + vec2(0.001, 0.0)) * influence * 0.015;
          }
          p.xy = wrapWorld(p.xy - flow * lifetime * 24.0 + vec2(uTime * 0.018, -uTime * 0.011));
          p.z += sin(uTime * 0.31 + aSeed * 19.0) * 0.08;
          vec4 mv = modelViewMatrix * vec4(p, 1.0);
          gl_Position = projectionMatrix * mv;
          gl_PointSize = clamp((0.72 + hash(aSeed * 41.0) * 0.9) * (260.0 / max(-mv.z, 0.1)), 0.45, 2.4);
          float life = 1.0 - abs(lifetime - 0.5) * 1.6;
          vAlpha = clamp(life, 0.08, 0.42) * 0.11;
          vColor = mix(vec3(0.50, 0.98, 0.78), vec3(1.15, 1.08, 0.72), hash(aSeed * 13.0));
        }
      `,
      fragmentShader: `
        varying float vAlpha;
        varying vec3 vColor;
        void main() {
          vec2 local = gl_PointCoord - vec2(0.5);
          float falloff = exp(-dot(local, local) * 9.0);
          gl_FragColor = vec4(vColor * falloff * 1.6, vAlpha * falloff);
        }
      `,
      blending: THREE.AdditiveBlending,
      depthTest: true,
      depthWrite: false,
      transparent: true,
    });
  }

  private updateStardustSource(index: number, x: number, y: number, projection: SceneProjection) {
    const sources = this.stardustMaterial.uniforms.uSources.value as THREE.Vector4[];
    if (!sources[index]) return;
    sources[index].set(x, y, 0.3 + projection.expression * 0.26 + projection.acknowledgement * 0.7, projection.hover);
  }

  private createAgent(projection: SceneProjection) {
    const color = new THREE.Color(projection.color ?? "#8fffd3");
    const glow = new THREE.Color(projection.glow ?? projection.color ?? "#8fffd3");
    const group = new THREE.Group();
    const cup = new THREE.Mesh(
      new THREE.TorusGeometry(0.42, 0.035, 12, 64),
      new THREE.MeshBasicMaterial({ color: glow, opacity: 0.38, transparent: true, depthWrite: false }),
    );
    cup.position.z = -0.43;
    const anchor = new THREE.Mesh(
      new THREE.CylinderGeometry(0.015, 0.015, 0.78, 8),
      new THREE.MeshBasicMaterial({ color: glow, opacity: 0.28, transparent: true, depthWrite: false }),
    );
    anchor.rotation.x = Math.PI / 2;
    anchor.position.z = -0.04;
    const body = new THREE.Mesh(
      new THREE.OctahedronGeometry(0.24, 0),
      new THREE.MeshStandardMaterial({
        color,
        emissive: glow,
        emissiveIntensity: 0.64,
        metalness: 0.18,
        roughness: 0.32,
        transparent: false,
        opacity: 1,
      }),
    );
    group.add(cup, anchor, body);
    group.userData.cup = cup;
    this.agentGroups.set(projection.id, group);
    this.scene.add(group);
    return group;
  }

  private keyPan(event: KeyboardEvent) {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const target = event.target;
    if (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement || target instanceof HTMLButtonElement) return;
    const key = event.key.toLowerCase();
    if (!["w", "a", "s", "d"].includes(key)) return;
    const step = this.cameraDistance * 0.035;
    const forward = this.cameraForwardOnPlane();
    const right = new THREE.Vector3().crossVectors(forward, new THREE.Vector3(0, 0, 1)).normalize();
    if (key === "w") this.cameraTarget.addScaledVector(forward, step);
    if (key === "s") this.cameraTarget.addScaledVector(forward, -step);
    if (key === "d") this.cameraTarget.addScaledVector(right, step);
    if (key === "a") this.cameraTarget.addScaledVector(right, -step);
    this.clampCameraTarget();
    this.updateCamera();
    event.preventDefault();
  }

  private panCamera(previousPointer: PointerState, nextPointer: PointerState) {
    const previous = this.projectPointerToPlane(previousPointer);
    const next = this.projectPointerToPlane(nextPointer);
    if (!previous || !next) return;
    this.cameraTarget.add(previous.sub(next));
    this.clampCameraTarget();
  }

  private cameraForwardOnPlane() {
    return new THREE.Vector3(-Math.sin(this.cameraYaw), Math.cos(this.cameraYaw), 0).normalize();
  }

  private clampCameraTarget() {
    const marginX = worldWidth * 0.55;
    const marginY = worldDepth * 0.55;
    this.cameraTarget.x = clamp(this.cameraTarget.x, -marginX, marginX);
    this.cameraTarget.y = clamp(this.cameraTarget.y, -marginY, marginY);
    this.cameraTarget.z = 0;
  }

  private projectPointerToPlane(pointer: PointerState) {
    const ndc = new THREE.Vector2(pointer.xPercent / 50 - 1, 1 - pointer.yPercent / 50);
    const hit = new THREE.Vector3();
    this.raycaster.setFromCamera(ndc, this.camera);
    return this.raycaster.ray.intersectPlane(this.worldPlane, hit);
  }

  private projectWorldToScreen(point: THREE.Vector3) {
    const projected = point.clone().project(this.camera);
    const distance = this.camera.position.distanceTo(point);
    return {
      scale: clamp((this.cameraDistance / Math.max(distance, 0.001)) * 0.96, 0.62, 1.72),
      xPercent: (projected.x * 0.5 + 0.5) * 100,
      yPercent: (0.5 - projected.y * 0.5) * 100,
    };
  }

  private cameraRight() {
    const direction = new THREE.Vector3();
    this.camera.getWorldDirection(direction);
    return new THREE.Vector3().crossVectors(direction, this.camera.up).normalize();
  }

  private cameraUp() {
    const direction = new THREE.Vector3();
    this.camera.getWorldDirection(direction);
    return new THREE.Vector3().crossVectors(this.cameraRight(), direction).normalize();
  }

  private agentHeight(projection: Pick<SceneProjection, "z">) {
    return 0.44 + projection.z * 0.95;
  }

  private updateCamera() {
    this.clampCameraTarget();
    const horizontal = Math.cos(this.cameraPitch) * this.cameraDistance;
    const z = Math.sin(this.cameraPitch) * this.cameraDistance;
    const offset = new THREE.Vector3(
      Math.sin(this.cameraYaw) * horizontal,
      -Math.cos(this.cameraYaw) * horizontal,
      z,
    );
    this.camera.position.copy(this.cameraTarget).add(offset);
    this.camera.lookAt(this.cameraTarget);
  }

  private render = (millis: number) => {
    if (this.disposed) return;
    const rect = this.canvas.getBoundingClientRect();
    const width = Math.max(1, Math.floor(rect.width));
    const height = Math.max(1, Math.floor(rect.height));
    this.renderer.setSize(width, height, false);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.gravityUniforms.uCellSize.value = this.gridCellSize();
    this.updateGridExtent();
    if (this.pointer.active) {
      const world = gridToWorld(this.pointer.xPercent, this.pointer.yPercent);
      this.cursor.visible = true;
      const projected = this.projectPointerToPlane(this.pointer);
      this.pointerWorld.copy(projected ?? new THREE.Vector3(world.x, world.y, 0));
      this.cursor.position.set(this.pointerWorld.x, this.pointerWorld.y, 0.04);
      this.cursor.rotation.z = millis * 0.0014;
      this.configureSplat(this.cursorSplat, this.pointerWorld.x, this.pointerWorld.y, 0.86, 0.34, 2.7, 0, 0, 1.25);
    } else {
      this.cursor.visible = false;
      this.cursorSplat.visible = false;
    }
    this.gravityUniforms.uTime.value = millis * 0.001;
    const previousTarget = this.renderer.getRenderTarget();
    this.renderer.setRenderTarget(this.gravityRenderTarget);
    this.renderer.setClearColor(0x000000, 1);
    this.renderer.clear(true, false, false);
    this.renderer.render(this.gravityScene, this.gravityCamera);
    this.renderer.setRenderTarget(previousTarget);
    this.renderer.setClearColor(0x000000, 0);
    this.renderer.render(this.scene, this.camera);
    this.canvas.dataset.threeReady = "true";
    this.canvas.dataset.threeAgents = String(this.agentGroups.size);
    this.canvas.dataset.threeCamera = [
      this.cameraDistance.toFixed(3),
      this.cameraYaw.toFixed(3),
      this.cameraPitch.toFixed(3),
      this.cameraTarget.x.toFixed(3),
      this.cameraTarget.y.toFixed(3),
    ].join(",");
    this.canvas.dataset.threeCursor = [this.pointerWorld.x.toFixed(3), this.pointerWorld.y.toFixed(3)].join(",");
    this.canvas.dataset.threeGridCell = this.gravityUniforms.uCellSize.value.toFixed(3);
    this.canvas.dataset.threeGridSize = [
      (this.gravityUniforms.uFieldHalfSize.value.x * 2).toFixed(3),
      (this.gravityUniforms.uFieldHalfSize.value.y * 2).toFixed(3),
    ].join(",");
    this.canvas.dataset.threeStardust = String(stardustParticleCount);
    this.raf = requestAnimationFrame(this.render);
  };

  private updateGridExtent() {
    const scale = clamp(this.cameraDistance / 9.2, 1, 3.1);
    const width = worldWidth * scale;
    const depth = worldDepth * scale;
    this.gridGroup.scale.set(width / worldWidth, depth / worldDepth, 1);
    this.gravityUniforms.uFieldHalfSize.value.set(width / 2, depth / 2);
  }

  private gridCellSize() {
    const exponent = Math.floor(Math.log2(Math.max(this.cameraDistance, 1) / 5.5));
    return clamp(0.28 * 2 ** exponent, 0.14, 1.12);
  }
}

function gridToWorld(xPercent: number, yPercent: number) {
  return {
    x: (xPercent / 100 - 0.5) * worldWidth,
    y: (0.5 - yPercent / 100) * worldDepth,
  };
}

function worldToGridPercent(x: number, y: number) {
  return {
    xPercent: clamp((x / worldWidth + 0.5) * 100, 0, 100),
    yPercent: clamp((0.5 - y / worldDepth) * 100, 0, 100),
  };
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function smoothstep(edge0: number, edge1: number, value: number) {
  const t = clamp((value - edge0) / Math.max(edge1 - edge0, 0.0001), 0, 1);
  return t * t * (3 - 2 * t);
}

function stablePhase(id: string) {
  let hash = 0;
  for (let index = 0; index < id.length; index += 1) {
    hash = (hash * 31 + id.charCodeAt(index)) >>> 0;
  }
  return (hash / 0xffffffff) * Math.PI * 2;
}

function spectralGridWave(
  temporalFrequency: number,
  amplitude: number,
  chirp: number,
  bandIndex: number,
  bandCount: number,
  elevation: number,
  hover: number,
  acknowledgement: number,
) {
  const octave = Math.log2(Math.max(temporalFrequency, 0.05) / 0.24);
  const rank = clamp(bandIndex / Math.max(bandCount - 1, 1), 0, 1);
  const octaveRank = clamp((octave + 0.35) / 4.6, 0, 1);
  const highness = Math.max(rank, octaveRank);
  const lowness = 1 - highness;
  const depthRollOff = 1 / (1 + highness * highness * 8.5);
  const excitation = 1 + acknowledgement * (0.42 - highness * 0.24) + hover * 0.12;
  const radius = 0.62 + lowness * 1.42 + elevation * 0.32 + hover * 0.16;
  const wavelength = 0.92 - highness * 0.68;
  const spatialFrequency = mathTau * radius / clamp(wavelength, 0.16, 0.92);
  const breathingHertz = clamp(0.5 + temporalFrequency * 0.44 + acknowledgement * 0.12, 0.5, 1);
  const shimmerHertz = clamp(1.1 + temporalFrequency * 0.82 + Math.abs(chirp) * 2.4, 1.1, 4.8);
  return {
    depth: amplitude * depthRollOff * excitation * (0.5 + lowness * 0.36),
    radius,
    sinePower: 0.92 + highness * 0.72 + Math.min(0.34, Math.abs(chirp) * 10),
    spatialFrequency,
    speed: mathTau * (breathingHertz * lowness + shimmerHertz * highness),
  };
}

function hashNumber(value: number) {
  return ((Math.sin(value * 12.9898) * 43758.5453) % 1 + 1) % 1;
}
