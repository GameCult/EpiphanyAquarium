import * as THREE from "three";
import { RGBELoader } from "three/examples/jsm/loaders/RGBELoader.js";
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
  projectGridDepthBounds(): { far: number; near: number };
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
const stardustSpan = Math.ceil(Math.sqrt(stardustParticleCount));
const stardustSpacing = (worldWidth * 3.4) / stardustSpan;
const maxStardustSources = 8;
const maxFieldSources = 8;
const froxelWidth = 96;
const froxelHeight = 54;
const froxelDepth = 24;
const froxelAtlasColumns = 6;
const froxelAtlasRows = 4;
const froxelMaxDistance = 28;
const studioHdrUrl = "/textures/studio3.hdr";

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
    uGravityOrigin: { value: new THREE.Vector2(0, 0) },
    uGridColor: { value: new THREE.Color(0x69ffd8) },
    uGridScale: { value: new THREE.Vector2(1, 1) },
    uGravitySize: { value: new THREE.Vector2(worldWidth, worldDepth) },
    uGravityTex: { value: null as THREE.Texture | null },
    uOpacity: { value: 0.42 },
    uTime: { value: 0 },
  };
  private gridGroup!: THREE.Group;
  private fieldColorData = Array.from({ length: maxFieldSources }, () => new THREE.Vector4(0.52, 1.0, 0.78, 0));
  private fieldSourceData = Array.from({ length: maxFieldSources }, () => new THREE.Vector4(999, 999, 0, 0));
  private fieldVolumeMaterial!: THREE.ShaderMaterial;
  private fieldVolumeScene = new THREE.Scene();
  private fieldVolumeCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  private froxelCamera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);
  private froxelInjectionMaterial!: THREE.ShaderMaterial;
  private froxelPrimitiveTargetA = new THREE.WebGLRenderTarget(froxelWidth * froxelAtlasColumns, froxelHeight * froxelAtlasRows, {
    depthBuffer: false,
    format: THREE.RGBAFormat,
    magFilter: THREE.NearestFilter,
    minFilter: THREE.NearestFilter,
    stencilBuffer: false,
    type: THREE.HalfFloatType,
  });
  private froxelPrimitiveTargetB = new THREE.WebGLRenderTarget(froxelWidth * froxelAtlasColumns, froxelHeight * froxelAtlasRows, {
    depthBuffer: false,
    format: THREE.RGBAFormat,
    magFilter: THREE.NearestFilter,
    minFilter: THREE.NearestFilter,
    stencilBuffer: false,
    type: THREE.HalfFloatType,
  });
  private froxelScene = new THREE.Scene();
  private pointer: PointerState = { active: false, xPercent: 50, yPercent: 50 };
  private pointerWorld = new THREE.Vector3(0, 0, 0);
  private raf = 0;
  private raycaster = new THREE.Raycaster();
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private splatMeshes: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>[] = [];
  private stardustMaterial!: THREE.ShaderMaterial;
  private useExternalFieldRenderer = Boolean((navigator as unknown as { gpu?: unknown }).gpu);
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
    this.gridGroup.add(this.createStardust());
    this.froxelInjectionMaterial = this.createFroxelInjectionMaterial();
    this.froxelScene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), this.froxelInjectionMaterial));
    this.fieldVolumeMaterial = this.createFieldVolumeMaterial();
    this.fieldVolumeScene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), this.fieldVolumeMaterial));
    this.scene.add(this.createCursor());
    this.loadStudioEnvironment();
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
    this.froxelPrimitiveTargetA.dispose();
    this.froxelPrimitiveTargetB.dispose();
    this.scene.environment?.dispose();
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
        screenDepth: screen.depth,
      };
    });
  }

  projectGridDepthBounds() {
    const halfSize = this.gravityUniforms.uFieldHalfSize.value as THREE.Vector2;
    const origin = this.gravityUniforms.uGravityOrigin.value as THREE.Vector2;
    const margin = 0.045;
    const corners = [
      new THREE.Vector3(origin.x - halfSize.x, origin.y - halfSize.y, -0.1),
      new THREE.Vector3(origin.x + halfSize.x, origin.y - halfSize.y, -0.1),
      new THREE.Vector3(origin.x + halfSize.x, origin.y + halfSize.y, -0.1),
      new THREE.Vector3(origin.x - halfSize.x, origin.y + halfSize.y, -0.1),
      new THREE.Vector3(origin.x - halfSize.x, origin.y - halfSize.y, 1.7),
      new THREE.Vector3(origin.x + halfSize.x, origin.y - halfSize.y, 1.7),
      new THREE.Vector3(origin.x + halfSize.x, origin.y + halfSize.y, 1.7),
      new THREE.Vector3(origin.x - halfSize.x, origin.y + halfSize.y, 1.7),
    ].map((point) => point.project(this.camera).z * 0.5 + 0.5);
    const near = clamp(Math.min(...corners) - margin, 0, 0.98);
    const far = clamp(Math.max(...corners) + margin, near + 0.02, 1);
    return {
      far,
      near,
    };
  }

  wheel(deltaY: number) {
    this.cameraDistance = clamp(this.cameraDistance * Math.exp(deltaY * 0.001), 4.8, 18);
    this.updateCamera();
  }

  setProjections(projections: SceneProjection[]) {
    const live = new Set<string>();
    let splatIndex = 0;
    let sourceIndex = 0;
    let fieldSourceIndex = 0;
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
      group.rotation.set(0, 0, 0);
      if (sourceIndex < maxStardustSources) {
        this.updateStardustSource(sourceIndex, target.x, target.y, projection);
        sourceIndex += 1;
      }
      if (fieldSourceIndex < maxFieldSources) {
        this.updateFieldSource(fieldSourceIndex, target.x, target.y, projection);
        fieldSourceIndex += 1;
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
    for (let index = fieldSourceIndex; index < maxFieldSources; index += 1) {
      this.updateFieldSource(index, 999, 999, { expression: 0, acknowledgement: 0, hover: 0, z: 0 } as SceneProjection);
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

  private loadStudioEnvironment() {
    const pmrem = new THREE.PMREMGenerator(this.renderer);
    pmrem.compileEquirectangularShader();
    new RGBELoader().load(
      studioHdrUrl,
      (texture) => {
        if (this.disposed) {
          texture.dispose();
          pmrem.dispose();
          return;
        }
        texture.mapping = THREE.EquirectangularReflectionMapping;
        const environment = pmrem.fromEquirectangular(texture).texture;
        this.scene.environment = environment;
        this.canvas.dataset.threeEnvironment = "studio3.hdr";
        texture.dispose();
        pmrem.dispose();
      },
      undefined,
      () => {
        this.canvas.dataset.threeEnvironment = "missing";
        pmrem.dispose();
      },
    );
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
        uniform vec2 uGravityOrigin;
        uniform vec2 uGravitySize;
        varying vec2 vGridPosition;
        varying float vDepth;
        varying float vFade;

        void main() {
          vec3 displaced = position;
          vec2 worldPosition = (modelMatrix * vec4(position, 1.0)).xy;
          vGridPosition = worldPosition;
          vec2 fieldPosition = worldPosition - uGravityOrigin;
          vec2 uv = fieldPosition / uGravitySize + 0.5;
          float gravityMask = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
          vec4 field = texture2D(uGravityTex, clamp(uv, vec2(0.0), vec2(1.0))) * gravityMask;
          float depth = field.r - field.g;
          displaced.z = -depth;
          float edge = max(abs(fieldPosition.x) / max(uFieldHalfSize.x, 0.001), abs(fieldPosition.y) / max(uFieldHalfSize.y, 0.001));
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
    for (let index = 0; index < stardustParticleCount; index += 1) {
      const offset = index * 3;
      positions[offset] = (index % stardustSpan) - stardustSpan / 2;
      positions[offset + 1] = Math.floor(index / stardustSpan) - stardustSpan / 2;
      positions[offset + 2] = 0;
    }
    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    const points = new THREE.Points(geometry, this.stardustMaterial);
    points.frustumCulled = false;
    points.renderOrder = 4;
    return points;
  }

  private createStardustMaterial() {
    const sourceData = Array.from({ length: maxStardustSources }, () => new THREE.Vector4(999, 999, 0, 0));
    return new THREE.ShaderMaterial({
      uniforms: {
        uGravityOrigin: this.gravityUniforms.uGravityOrigin,
        uGridScale: this.gravityUniforms.uGridScale,
        uGravityTex: this.gravityUniforms.uGravityTex,
        uGravitySize: this.gravityUniforms.uGravitySize,
        uDustSpacing: { value: stardustSpacing },
        uSources: { value: sourceData },
        uTime: this.gravityUniforms.uTime,
      },
      vertexShader: `
        uniform vec2 uGravityOrigin;
        uniform vec2 uGridScale;
        uniform sampler2D uGravityTex;
        uniform vec2 uGravitySize;
        uniform float uDustSpacing;
        uniform float uTime;
        uniform vec4 uSources[${maxStardustSources}];
        varying float vAlpha;
        varying vec3 vColor;

        float hash(float value) {
          return fract(sin(value * 12.9898) * 43758.5453);
        }

        float hashCell(vec2 cell, float salt) {
          return hash(dot(cell, vec2(127.1, 311.7)) + salt);
        }

        void main() {
          vec2 cellOffset = position.xy;
          vec2 originCell = floor(uGravityOrigin / uDustSpacing);
          vec2 worldCell = cellOffset + originCell;
          float cellSeed = hashCell(worldCell, 11.0);
          float lifetime = fract(uTime * 0.11 + cellSeed);
          float heightSeed = hashCell(worldCell, 173.0);
          float sideSeed = hashCell(worldCell, 257.0);
          float belowSeed = hashCell(worldCell, 331.0);
          vec2 jitter = vec2(hashCell(worldCell, 19.0), hashCell(worldCell, 41.0)) - 0.5;
          vec2 worldXY = (worldCell + jitter * 0.82) * uDustSpacing;
          vec2 flow = vec2(
            sin(worldXY.y * 1.7 + uTime * 0.23 + cellSeed * 6.28318),
            cos(worldXY.x * 1.4 - uTime * 0.19 + cellSeed * 5.31)
          ) * 0.022;
          for (int i = 0; i < ${maxStardustSources}; i += 1) {
            vec4 source = uSources[i];
            vec2 delta = source.xy - worldXY;
            float influence = exp(-dot(delta, delta) * 0.42) * source.z;
            vec2 tangent = normalize(vec2(-delta.y, delta.x) + vec2(0.001, 0.0));
            flow += tangent * influence * 0.038 + source.w * normalize(delta + vec2(0.001, 0.0)) * influence * 0.015;
          }
          worldXY -= flow * lifetime * 24.0;
          vec2 fieldPosition = worldXY - uGravityOrigin;
          vec2 gridUv = fieldPosition / uGravitySize + 0.5;
          float gravityMask = step(0.0, gridUv.x) * step(gridUv.x, 1.0) * step(0.0, gridUv.y) * step(gridUv.y, 1.0);
          vec4 field = texture2D(uGravityTex, clamp(gridUv, vec2(0.0), vec2(1.0))) * gravityMask;
          float gridHeight = -(field.r - field.g);
          float aboveHeight = -log(max(1.0 - heightSeed, 0.001)) * 0.18;
          float belowHeight = log(max(1.0 - belowSeed, 0.001)) * 0.045;
          float gridDistance = sideSeed < 0.12 ? belowHeight : aboveHeight;
          gridDistance += sin(uTime * 0.31 + cellSeed * 19.0) * 0.018;
          vec3 p = vec3(fieldPosition / max(uGridScale, vec2(0.001)), gridHeight + gridDistance);
          vec4 mv = modelViewMatrix * vec4(p, 1.0);
          gl_Position = projectionMatrix * mv;
          gl_PointSize = clamp((0.72 + hashCell(worldCell, 71.0) * 0.9) * (260.0 / max(-mv.z, 0.1)), 0.45, 2.4);
          float life = 1.0 - abs(lifetime - 0.5) * 1.6;
          float edge = max(abs(fieldPosition.x) / max(uGravitySize.x * 0.5, 0.001), abs(fieldPosition.y) / max(uGravitySize.y * 0.5, 0.001));
          float edgeFade = (1.0 - smoothstep(0.74, 1.0, edge)) * gravityMask;
          vAlpha = clamp(life, 0.08, 0.42) * 0.11 * edgeFade;
          vColor = mix(vec3(0.50, 0.98, 0.78), vec3(1.15, 1.08, 0.72), hashCell(worldCell, 13.0));
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

  private createFroxelInjectionMaterial() {
    return new THREE.ShaderMaterial({
      uniforms: {
        uCameraMatrixWorld: { value: new THREE.Matrix4() },
        uCameraPosition: { value: new THREE.Vector3() },
        uCameraTarget: { value: this.cameraTarget },
        uFieldColors: { value: this.fieldColorData },
        uFieldSources: { value: this.fieldSourceData },
        uFroxelAtlas: { value: new THREE.Vector4(froxelWidth, froxelHeight, froxelAtlasColumns, froxelAtlasRows) },
        uFroxelDepth: { value: froxelDepth },
        uFroxelMaxDistance: { value: froxelMaxDistance },
        uGravityOrigin: this.gravityUniforms.uGravityOrigin,
        uGravitySize: this.gravityUniforms.uGravitySize,
        uGravityTex: this.gravityUniforms.uGravityTex,
        uInvProjectionMatrix: { value: new THREE.Matrix4() },
        uPrimitiveOffset: { value: 0 },
        uPointer: { value: new THREE.Vector4(999, 999, 0, 0) },
        uTime: this.gravityUniforms.uTime,
      },
      vertexShader: `
        varying vec2 vUv;

        void main() {
          vUv = uv;
          gl_Position = vec4(position.xy, 0.0, 1.0);
        }
      `,
      fragmentShader: `
        precision highp float;

        uniform mat4 uCameraMatrixWorld;
        uniform vec3 uCameraPosition;
        uniform vec3 uCameraTarget;
        uniform vec4 uFieldColors[${maxFieldSources}];
        uniform vec4 uFieldSources[${maxFieldSources}];
        uniform vec4 uFroxelAtlas;
        uniform int uFroxelDepth;
        uniform float uFroxelMaxDistance;
        uniform vec2 uGravityOrigin;
        uniform vec2 uGravitySize;
        uniform sampler2D uGravityTex;
        uniform mat4 uInvProjectionMatrix;
        uniform int uPrimitiveOffset;
        uniform vec4 uPointer;
        uniform float uTime;
        varying vec2 vUv;

        float sourceRadius(vec4 source) {
          return 0.24;
        }

        float primitiveIntersectsFroxel(int index, vec3 p, float t) {
          vec4 source = uFieldSources[index];
          vec3 center = vec3(source.xy, 0.54 + source.w * 0.55);
          float radius = sourceRadius(source);
          float atmosphereRadius = radius * mix(1.45, 2.35, uFieldColors[index].w) * (0.95 + source.z * 0.42);
          float cellRadius = 0.22 + t * 0.018;
          return 1.0 - step(atmosphereRadius + cellRadius, length(p - center));
        }

        void main() {
          vec2 atlasPixel = floor(vUv * uFroxelAtlas.xy * uFroxelAtlas.zw);
          float tileX = mod(atlasPixel.x, uFroxelAtlas.x);
          float tileY = mod(atlasPixel.y, uFroxelAtlas.y);
          float column = floor(atlasPixel.x / uFroxelAtlas.x);
          float row = floor(atlasPixel.y / uFroxelAtlas.y);
          float slice = row * uFroxelAtlas.z + column;
          if (slice >= float(uFroxelDepth)) discard;
          vec2 sliceUv = (vec2(tileX, tileY) + 0.5) / uFroxelAtlas.xy;
          vec2 ndc = sliceUv * 2.0 - 1.0;
          vec4 farView = uInvProjectionMatrix * vec4(ndc, 1.0, 1.0);
          farView /= farView.w;
          vec3 rayOrigin = uCameraPosition;
          vec3 rayFar = (uCameraMatrixWorld * farView).xyz;
          vec3 rayDir = normalize(rayFar - rayOrigin);
          float progress = (slice + 0.5) / max(float(uFroxelDepth), 1.0);
          float t = mix(0.12, uFroxelMaxDistance, progress * progress);
          vec3 p = rayOrigin + rayDir * t;
          vec4 mask = vec4(0.0);
          for (int channel = 0; channel < 4; channel += 1) {
            int primitiveIndex = uPrimitiveOffset + channel;
            float hit = 0.0;
            if (primitiveIndex == 0) hit = primitiveIntersectsFroxel(0, p, t);
            if (primitiveIndex == 1) hit = primitiveIntersectsFroxel(1, p, t);
            if (primitiveIndex == 2) hit = primitiveIntersectsFroxel(2, p, t);
            if (primitiveIndex == 3) hit = primitiveIntersectsFroxel(3, p, t);
            if (primitiveIndex == 4) hit = primitiveIntersectsFroxel(4, p, t);
            if (primitiveIndex == 5) hit = primitiveIntersectsFroxel(5, p, t);
            if (primitiveIndex == 6) hit = primitiveIntersectsFroxel(6, p, t);
            if (primitiveIndex == 7) hit = primitiveIntersectsFroxel(7, p, t);
            if (channel == 0) mask.x = hit;
            if (channel == 1) mask.y = hit;
            if (channel == 2) mask.z = hit;
            if (channel == 3) mask.w = hit;
          }
          gl_FragColor = mask;
        }
      `,
      depthTest: false,
      depthWrite: false,
    });
  }

  private createFieldVolumeMaterial() {
    return new THREE.ShaderMaterial({
      uniforms: {
        uCameraMatrixWorld: { value: new THREE.Matrix4() },
        uCameraPosition: { value: new THREE.Vector3() },
        uCameraTarget: { value: this.cameraTarget },
        uFieldColors: { value: this.fieldColorData },
        uFieldSources: { value: this.fieldSourceData },
        uFogSteps: { value: new URLSearchParams(globalThis.location?.search ?? "").has("smoke") ? 18 : 32 },
        uFroxelAtlas: { value: new THREE.Vector4(froxelWidth, froxelHeight, froxelAtlasColumns, froxelAtlasRows) },
        uFroxelDepth: { value: froxelDepth },
        uFroxelMaxDistance: { value: froxelMaxDistance },
        uFroxelPrimitiveTexA: { value: this.froxelPrimitiveTargetA.texture },
        uFroxelPrimitiveTexB: { value: this.froxelPrimitiveTargetB.texture },
        uGravityOrigin: this.gravityUniforms.uGravityOrigin,
        uGravitySize: this.gravityUniforms.uGravitySize,
        uGravityTex: this.gravityUniforms.uGravityTex,
        uInvProjectionMatrix: { value: new THREE.Matrix4() },
        uPointer: { value: new THREE.Vector4(999, 999, 0, 0) },
        uResolution: { value: new THREE.Vector2(1, 1) },
        uTime: this.gravityUniforms.uTime,
      },
      vertexShader: `
        varying vec2 vUv;

        void main() {
          vUv = uv;
          gl_Position = vec4(position.xy, 0.0, 1.0);
        }
      `,
      fragmentShader: `
        precision highp float;

        uniform mat4 uCameraMatrixWorld;
        uniform vec3 uCameraPosition;
        uniform vec3 uCameraTarget;
        uniform vec4 uFieldColors[${maxFieldSources}];
        uniform vec4 uFieldSources[${maxFieldSources}];
        uniform int uFogSteps;
        uniform vec4 uFroxelAtlas;
        uniform int uFroxelDepth;
        uniform float uFroxelMaxDistance;
        uniform sampler2D uFroxelPrimitiveTexA;
        uniform sampler2D uFroxelPrimitiveTexB;
        uniform vec2 uGravityOrigin;
        uniform vec2 uGravitySize;
        uniform sampler2D uGravityTex;
        uniform mat4 uInvProjectionMatrix;
        uniform vec4 uPointer;
        uniform vec2 uResolution;
        uniform float uTime;
        varying vec2 vUv;

        float hash(vec2 p) {
          return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
        }

        float hash13(vec3 p) {
          return fract(sin(dot(p, vec3(17.17, 59.4, 15.13))) * 43758.5453123);
        }

        float tri(float x) {
          return abs(fract(x) - 0.5);
        }

        vec3 tri3(vec3 p) {
          return vec3(
            tri(p.z + tri(p.y)),
            tri(p.z + tri(p.x)),
            tri(p.y + tri(p.x))
          );
        }

        float cheapNoise4(vec4 p) {
          return sin(dot(p, vec4(1.71, 2.43, 3.17, 1.19)))
            * cos(dot(p, vec4(2.13, -1.37, 1.91, 2.61)))
            + 0.45 * sin(dot(p, vec4(-1.11, 3.03, 2.07, 1.73)) + sin(p.w + p.x));
        }

        float fbm4(vec4 p) {
          float value = 0.0;
          float amplitude = 0.5;
          mat3 rot = mat3(
            0.00, 0.80, 0.60,
           -0.80, 0.36,-0.48,
           -0.60,-0.48, 0.64
          );
          for (int i = 0; i < 4; i += 1) {
            value += cheapNoise4(p) * amplitude;
            p.xyz = rot * p.xyz * 2.03 + vec3(3.7, 1.9, 5.1);
            p.w = p.w * 1.37 + 2.11;
            amplitude *= 0.52;
          }
          return clamp(value, -1.0, 1.0);
        }

        float triNoise3d(vec3 p) {
          float z = 1.4;
          float rz = 0.001;
          vec3 bp = p;
          for (int i = 0; i < 3; i += 1) {
            vec3 dg = tri3(bp * 2.0);
            p += dg + uTime * 0.018;
            bp *= 1.8;
            z *= 1.5;
            p *= 1.2;
            rz += tri(p.z + tri(p.x + tri(p.y))) / z;
            bp += 0.14;
          }
          return rz;
        }

        vec3 triFlow(vec3 p) {
          vec3 t1 = normalize(tri3(p * 0.52) - 0.5);
          vec3 t2 = normalize(tri3(p * 0.841 + 11.7) - 0.5);
          return normalize(cross(t1, t2) + vec3(0.001, 0.0, 0.0));
        }

        float sdSphere(vec3 p, float radius) {
          return length(p) - radius;
        }

        float sdRoundBox(vec3 p, vec3 b, float r) {
          vec3 q = abs(p) - b;
          return length(max(q, 0.0)) + min(max(q.x, max(q.y, q.z)), 0.0) - r;
        }

        float sourceRadius(vec4 source) {
          return 0.24;
        }

        float sourceDisplacement(vec3 local, float radius, float mass, float selfFlag) {
          vec3 domain = local / max(radius, 0.001);
          float grain = fbm4(vec4(domain * mix(1.85, 3.35, selfFlag), uTime * mix(0.12, 0.34, selfFlag)));
          float ridges = 1.0 - abs(fbm4(vec4(domain * 4.8 + grain, uTime * 0.58)) * 1.35);
          float loops = pow(clamp(ridges, 0.0, 1.0), 4.2) * selfFlag;
          return grain * radius * mix(0.08, 0.24, selfFlag) * (0.65 + mass * 0.8) + loops * radius * 0.34;
        }

        float planetSdf(vec3 p, vec4 source, float selfFlag) {
          vec3 center = vec3(source.xy, 0.54 + source.w * 0.55);
          float radius = sourceRadius(source);
          vec3 local = p - center;
          return length(local) - radius - sourceDisplacement(local, radius, source.z, selfFlag);
        }

        vec4 sampleGravity(vec2 xy) {
          vec2 uv = (xy - uGravityOrigin) / uGravitySize + 0.5;
          float mask = step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
          return texture2D(uGravityTex, clamp(uv, vec2(0.0), vec2(1.0))) * mask;
        }

        float gravityHeight(vec2 xy) {
          vec4 field = sampleGravity(xy);
          return -(field.r - field.g);
        }

        float primitiveMask(int index, vec4 maskA, vec4 maskB) {
          if (index == 0) return maskA.x;
          if (index == 1) return maskA.y;
          if (index == 2) return maskA.z;
          if (index == 3) return maskA.w;
          if (index == 4) return maskB.x;
          if (index == 5) return maskB.y;
          if (index == 6) return maskB.z;
          if (index == 7) return maskB.w;
          return 0.0;
        }

        float solidSdfMasked(vec3 p, vec4 maskA, vec4 maskB, out vec3 color) {
          float agentSolid = 999.0;
          vec3 agentColor = vec3(0.55, 1.0, 0.78);
          for (int i = 0; i < ${maxFieldSources}; i += 1) {
            if (primitiveMask(i, maskA, maskB) > 0.01) {
              vec4 source = uFieldSources[i];
              vec4 sourceColor = uFieldColors[i];
              float d = planetSdf(p, source, sourceColor.w);
              if (d < agentSolid) {
                agentSolid = d;
                agentColor = sourceColor.rgb;
              }
            }
          }
          color = agentColor;
          return agentSolid;
        }

        float nearestPlanetMasked(vec3 p, vec4 maskA, vec4 maskB, out vec4 source, out vec4 sourceColor, out float sdfValue) {
          float nearest = 999.0;
          source = vec4(999.0, 999.0, 0.0, 0.0);
          sourceColor = vec4(0.5, 1.0, 0.76, 0.0);
          for (int i = 0; i < ${maxFieldSources}; i += 1) {
            if (primitiveMask(i, maskA, maskB) > 0.01) {
              vec4 candidate = uFieldSources[i];
              vec4 candidateColor = uFieldColors[i];
              float d = planetSdf(p, candidate, candidateColor.w);
              if (d < nearest) {
                nearest = d;
                source = candidate;
                sourceColor = candidateColor;
              }
            }
          }
          sdfValue = nearest;
          return nearest;
        }

        float gasDensityMasked(vec3 p, vec3 rayDir, vec4 maskA, vec4 maskB, out vec3 tint) {
          vec3 anchor = uCameraTarget;
          vec3 gasP = p - (anchor + vec3(0.0, 0.0, 0.24));
          vec3 flow = triFlow(gasP * 0.42 - vec3(0.0, uTime * 0.035, 0.0));
          float phaseA = fract(uTime / 9.0);
          float phaseB = fract(uTime / 9.0 + 0.5);
          float windowA = 1.0 - 2.0 * abs(phaseA - 0.5);
          float windowB = 1.0 - 2.0 * abs(phaseB - 0.5);
          float low = (
            pow(max(triNoise3d((gasP + flow * (phaseA - 0.5) * 9.0) / 2.6), 0.0001), 0.78) * windowA +
            pow(max(triNoise3d((gasP + flow * (phaseB - 0.5) * 9.0) / 2.6), 0.0001), 0.78) * windowB
          );
          float high = pow(max(triNoise3d((gasP + flow * phaseA * 2.6) * 3.1), 0.0001), 1.7) * 0.35;
          vec3 ellipsoid = gasP / vec3(4.8, 3.1, 1.16);
          float gasShape = exp(-dot(ellipsoid, ellipsoid) * 1.35);
          float h = gravityHeight(p.xy);
          float surfaceFog = exp(-abs(p.z - h - 0.16) * 1.9) * 0.28;
          float sourceFog = 0.0;
          vec3 sourceTint = vec3(0.0);
          for (int i = 0; i < ${maxFieldSources}; i += 1) {
            if (primitiveMask(i, maskA, maskB) > 0.01) {
              vec4 source = uFieldSources[i];
              vec4 sourceColor = uFieldColors[i];
              vec3 center = vec3(source.xy, 0.54 + source.w * 0.55);
              float radius = sourceRadius(source);
              float selfFlag = sourceColor.w;
              float d = planetSdf(p, source, selfFlag);
              float shell = exp(-max(d, 0.0) / max(radius * mix(0.28, 0.82, selfFlag) * (0.78 + source.z * 0.55), 0.001));
              float outside = smoothstep(-0.025, 0.08, d);
              float loopNoise = pow(max(0.0, fbm4(vec4((p - center) / max(radius, 0.001) * 3.5, uTime * 0.42)) * 0.5 + 0.5), mix(2.6, 7.0, selfFlag));
              float localFog = shell * outside * (0.035 + source.z * 0.12 + selfFlag * 0.36) * (0.55 + loopNoise * mix(0.7, 2.4, selfFlag));
              sourceFog += localFog;
              sourceTint += localFog * mix(sourceColor.rgb, vec3(2.8, 1.65, 0.46), selfFlag);
            }
          }
          vec2 pointerDelta = p.xy - uPointer.xy;
          float pointerFog = exp(-dot(pointerDelta, pointerDelta) * 1.1 - abs(p.z - 0.28) * 2.2) * uPointer.z * 0.18;
          float horizonBias = smoothstep(0.1, 0.9, dot(rayDir, normalize(vec3(rayDir.xy, -0.18))) * 0.5 + 0.5);
          float density = max(0.0, gasShape * (0.018 + low * 0.04 - high * 0.018) + surfaceFog + sourceFog + pointerFog) * (0.7 + horizonBias * 0.55);
          tint = mix(fogTint(p, density), sourceTint / max(sourceFog, 0.0001), clamp(sourceFog * 9.0, 0.0, 1.0));
          return density;
        }

        vec3 fogTint(vec3 p, float density) {
          vec4 field = sampleGravity(p.xy);
          vec3 base = mix(vec3(0.18, 0.92, 0.72), vec3(1.0, 0.72, 0.42), clamp(field.b * 2.5, 0.0, 1.0));
          vec3 cool = vec3(0.22, 0.46, 0.72);
          return mix(cool, base, clamp(density * 8.0, 0.0, 1.0));
        }

        vec3 atmosphereTint(vec3 p, vec4 maskA, vec4 maskB) {
          vec4 nearestSource;
          vec4 nearestColor;
          float d;
          nearestPlanetMasked(p, maskA, maskB, nearestSource, nearestColor, d);
          float selfFlag = nearestColor.w;
          vec3 solar = vec3(2.8, 1.65, 0.46);
          return mix(nearestColor.rgb * 0.95 + vec3(0.08, 0.22, 0.18), solar, selfFlag);
        }

        void sampleFroxelMasks(vec2 screenUv, float progress, out vec4 maskA, out vec4 maskB) {
          float z = clamp(progress, 0.0, 0.9999) * float(uFroxelDepth);
          float slice = floor(z);
          vec2 tileUv = clamp(screenUv, vec2(0.001), vec2(0.999));
          float column = mod(slice, uFroxelAtlas.z);
          float row = floor(slice / uFroxelAtlas.z);
          vec2 atlas = (vec2(column, row) + tileUv) / uFroxelAtlas.zw;
          maskA = texture2D(uFroxelPrimitiveTexA, atlas);
          maskB = texture2D(uFroxelPrimitiveTexB, atlas);
        }

        vec3 estimateNormal(vec3 p, vec4 maskA, vec4 maskB) {
          vec3 c;
          vec2 e = vec2(0.015, 0.0);
          float dx = solidSdfMasked(p + e.xyy, maskA, maskB, c) - solidSdfMasked(p - e.xyy, maskA, maskB, c);
          float dy = solidSdfMasked(p + e.yxy, maskA, maskB, c) - solidSdfMasked(p - e.yxy, maskA, maskB, c);
          float dz = solidSdfMasked(p + e.yyx, maskA, maskB, c) - solidSdfMasked(p - e.yyx, maskA, maskB, c);
          return normalize(vec3(dx, dy, dz));
        }

        void main() {
          vec2 ndc = vUv * 2.0 - 1.0;
          vec4 nearView = uInvProjectionMatrix * vec4(ndc, -1.0, 1.0);
          nearView /= nearView.w;
          vec4 farView = uInvProjectionMatrix * vec4(ndc, 1.0, 1.0);
          farView /= farView.w;
          vec3 rayOrigin = uCameraPosition;
          vec3 rayFar = (uCameraMatrixWorld * farView).xyz;
          vec3 rayDir = normalize(rayFar - rayOrigin);
          float jitter = hash(gl_FragCoord.xy + uTime * 23.17);
          float maxT = uFroxelMaxDistance;
          float transmittance = 1.0;
          vec3 scattering = vec3(0.0);
          vec3 solidColor = vec3(0.0);
          float solidHit = 0.0;
          float solidT = maxT;
          float t = 0.16 + jitter * 0.08;
          for (int i = 0; i < 72; i += 1) {
            if (i >= uFogSteps) break;
            float progress = (float(i) + jitter) / max(float(uFogSteps), 1.0);
            t = mix(0.12, maxT, progress * progress);
            vec3 p = rayOrigin + rayDir * t;
            vec4 maskA;
            vec4 maskB;
            sampleFroxelMasks(vUv, progress, maskA, maskB);
            vec3 localSolidColor;
            float sd = solidSdfMasked(p, maskA, maskB, localSolidColor);
            if (sd < 0.012 && solidHit < 0.5) {
              solidHit = 1.0;
              solidT = t;
              vec3 normal = estimateNormal(p, maskA, maskB);
              vec4 hitSource;
              vec4 hitColor;
              float hitSdf;
              nearestPlanetMasked(p, maskA, maskB, hitSource, hitColor, hitSdf);
              float selfFlag = hitColor.w;
              vec3 viewDir = normalize(rayOrigin - p);
              vec3 reflected = reflect(-viewDir, normal);
              vec3 sky = mix(vec3(0.04, 0.12, 0.18), vec3(0.62, 0.98, 0.86), clamp(reflected.z * 0.5 + 0.5, 0.0, 1.0));
              float fresnel = pow(1.0 - clamp(dot(normal, viewDir), 0.0, 1.0), 4.0);
              float light = clamp(dot(normal, normalize(vec3(-0.32, 0.44, 0.84))) * 0.5 + 0.5, 0.0, 1.0);
              float plasma = pow(clamp(fbm4(vec4((p - vec3(hitSource.xy, 0.54 + hitSource.w * 0.55)) / max(sourceRadius(hitSource), 0.001) * 5.4, uTime * 0.62)) * 0.5 + 0.5, 0.0, 1.0), 5.0);
              vec3 chrome = mix(localSolidColor * (0.18 + light * 0.28), sky * 1.55, 0.72 + fresnel * 0.22);
              vec3 solar = vec3(4.2, 2.15, 0.56) * (0.74 + plasma * 1.7) + vec3(1.2, 0.32, 0.08) * fresnel;
              solidColor = mix(chrome + localSolidColor * fresnel * 0.7, solar, selfFlag);
              break;
            }
            float stepSize = maxT / max(float(uFogSteps), 1.0) * (0.45 + progress * 1.45);
            vec3 tint;
            float d = gasDensityMasked(p, rayDir, maskA, maskB, tint);
            float extinction = d * 0.82;
            float stepTransmittance = exp(-extinction * stepSize);
            vec3 luminance = mix(tint, atmosphereTint(p, maskA, maskB), clamp(d * 4.0, 0.0, 1.0)) * d * 1.35;
            scattering += transmittance * (luminance - luminance * stepTransmittance) / max(extinction, 0.0001);
            transmittance *= stepTransmittance;
            if (transmittance < 0.015) break;
          }
          float fogAlpha = clamp(1.0 - transmittance, 0.0, 0.82);
          vec3 color = scattering;
          float alpha = fogAlpha;
          if (solidHit > 0.5) {
            color = solidColor * transmittance + scattering;
            alpha = 0.96;
            float coverage = 0.92;
            float noise = hash(gl_FragCoord.xy + floor(uTime * 60.0) * 1.61803398875);
            if (noise > coverage) discard;
          }
          gl_FragColor = vec4(color, alpha);
        }
      `,
      blending: THREE.NormalBlending,
      depthTest: false,
      depthWrite: false,
      transparent: true,
    });
  }

  private updateStardustSource(index: number, x: number, y: number, projection: SceneProjection) {
    const sources = this.stardustMaterial.uniforms.uSources.value as THREE.Vector4[];
    if (!sources[index]) return;
    sources[index].set(x, y, 0.3 + projection.expression * 0.26 + projection.acknowledgement * 0.7, projection.hover);
  }

  private updateFieldSource(index: number, x: number, y: number, projection: SceneProjection) {
    if (index < 0 || index >= maxFieldSources) return;
    const sources = this.fieldVolumeMaterial.uniforms.uFieldSources.value as THREE.Vector4[];
    const colors = this.fieldVolumeMaterial.uniforms.uFieldColors.value as THREE.Vector4[];
    if (!sources[index]) return;
    const mass = projection.id === "coordinator"
      ? clamp(1.18 + projection.expression * 0.26 + projection.acknowledgement * 0.42, 0, 1.65)
      : clamp(0.22 + projection.expression * 0.34 + projection.acknowledgement * 0.9, 0, 1.4);
    sources[index].set(x, y, mass, projection.z ?? 0);
    if (colors[index]) {
      const color = new THREE.Color(projection.color ?? "#8fffd3");
      const glow = new THREE.Color(projection.glow ?? projection.color ?? "#8fffd3");
      const body = projection.id === "coordinator" ? glow.lerp(new THREE.Color("#ffd15e"), 0.46) : color.lerp(glow, 0.42);
      colors[index].set(body.r, body.g, body.b, projection.id === "coordinator" ? 1 : 0);
    }
  }

  private createAgent(projection: SceneProjection) {
    const group = new THREE.Group();
    group.userData.agentId = projection.id;
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
      depth: clamp(projected.z * 0.5 + 0.5, 0, 1),
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
    if (!this.useExternalFieldRenderer) {
      this.froxelInjectionMaterial.uniforms.uCameraMatrixWorld.value.copy(this.camera.matrixWorld);
      this.froxelInjectionMaterial.uniforms.uCameraPosition.value.copy(this.camera.position);
      this.froxelInjectionMaterial.uniforms.uCameraTarget.value.copy(this.cameraTarget);
      this.froxelInjectionMaterial.uniforms.uInvProjectionMatrix.value.copy(this.camera.projectionMatrixInverse);
      this.froxelInjectionMaterial.uniforms.uPointer.value.set(this.pointerWorld.x, this.pointerWorld.y, this.pointer.active ? 1 : 0, 0);
      this.renderer.setRenderTarget(this.froxelPrimitiveTargetA);
      this.renderer.setClearColor(0x000000, 1);
      this.renderer.clear(true, false, false);
      this.froxelInjectionMaterial.uniforms.uPrimitiveOffset.value = 0;
      this.renderer.render(this.froxelScene, this.froxelCamera);
      this.renderer.setRenderTarget(this.froxelPrimitiveTargetB);
      this.renderer.clear(true, false, false);
      this.froxelInjectionMaterial.uniforms.uPrimitiveOffset.value = 4;
      this.renderer.render(this.froxelScene, this.froxelCamera);
      this.renderer.setRenderTarget(previousTarget);
      this.renderer.setClearColor(0x000000, 0);
      this.fieldVolumeMaterial.uniforms.uCameraMatrixWorld.value.copy(this.camera.matrixWorld);
      this.fieldVolumeMaterial.uniforms.uCameraPosition.value.copy(this.camera.position);
      this.fieldVolumeMaterial.uniforms.uCameraTarget.value.copy(this.cameraTarget);
      this.fieldVolumeMaterial.uniforms.uInvProjectionMatrix.value.copy(this.camera.projectionMatrixInverse);
      this.fieldVolumeMaterial.uniforms.uPointer.value.set(this.pointerWorld.x, this.pointerWorld.y, this.pointer.active ? 1 : 0, 0);
      this.fieldVolumeMaterial.uniforms.uResolution.value.set(width, height);
      this.renderer.autoClear = false;
      this.renderer.render(this.fieldVolumeScene, this.fieldVolumeCamera);
      this.renderer.autoClear = true;
    }
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
    this.canvas.dataset.threeGridOrigin = [
      this.gravityUniforms.uGravityOrigin.value.x.toFixed(3),
      this.gravityUniforms.uGravityOrigin.value.y.toFixed(3),
    ].join(",");
    this.canvas.dataset.threeStardust = String(stardustParticleCount);
    this.canvas.dataset.threeFieldVolume = this.useExternalFieldRenderer ? "webgpu-external-field" : "webgl-froxel-primitive-mask+sdf-gas";
    this.raf = requestAnimationFrame(this.render);
  };

  private updateGridExtent() {
    const scale = clamp(this.cameraDistance / 9.2, 1, 3.1);
    const width = worldWidth * scale;
    const depth = worldDepth * scale;
    const scaleX = width / worldWidth;
    const scaleY = depth / worldDepth;
    this.gridGroup.position.set(this.cameraTarget.x, this.cameraTarget.y, 0);
    this.gridGroup.scale.set(scaleX, scaleY, 1);
    this.gravityUniforms.uFieldHalfSize.value.set(width / 2, depth / 2);
    this.gravityUniforms.uGravityOrigin.value.set(this.cameraTarget.x, this.cameraTarget.y);
    this.gravityUniforms.uGravitySize.value.set(width, depth);
    this.gravityUniforms.uGridScale.value.set(scaleX, scaleY);
    this.gravityCamera.left = -width / 2;
    this.gravityCamera.right = width / 2;
    this.gravityCamera.top = depth / 2;
    this.gravityCamera.bottom = -depth / 2;
    this.gravityCamera.position.set(this.cameraTarget.x, this.cameraTarget.y, 5);
    this.gravityCamera.lookAt(this.cameraTarget.x, this.cameraTarget.y, 0);
    this.gravityCamera.updateProjectionMatrix();
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
  const octaveStep = Math.max(0, octave + rank * 0.65);
  const amplitudeLacunarity = 0.42;
  const depthRollOff = amplitudeLacunarity ** octaveStep / (1 + highness * highness * 5.5);
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
