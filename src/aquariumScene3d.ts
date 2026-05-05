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

export interface AquariumScene3d {
  dispose(): void;
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
    uGridColor: { value: new THREE.Color(0x69ffd8) },
    uGravityTex: { value: null as THREE.Texture | null },
    uOpacity: { value: 0.42 },
    uTime: { value: 0 },
  };
  private pointer: PointerState = { active: false, xPercent: 50, yPercent: 50 };
  private pointerWorld = new THREE.Vector3(0, 0, 0);
  private raf = 0;
  private raycaster = new THREE.Raycaster();
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private splatMeshes: THREE.Mesh<THREE.PlaneGeometry, THREE.ShaderMaterial>[] = [];
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
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.6));
    this.gravityUniforms.uGravityTex.value = this.gravityRenderTarget.texture;
    this.camera.up.set(0, 0, 1);
    this.gravityCamera.position.set(0, 0, 5);
    this.gravityCamera.lookAt(0, 0, 0);
    this.createSplatPool(48);
    this.updateCamera();
    this.scene.add(new THREE.AmbientLight(0xbfffe8, 0.74));
    const key = new THREE.DirectionalLight(0xd8fff0, 1.2);
    key.position.set(-4.5, 8, 5.2);
    this.scene.add(key);
    this.scene.add(this.createGrid());
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

  projectProjections(projections: SceneProjection[]) {
    return projections.map((projection) => {
      const target = gridToWorld(projection.gridXPercent, projection.gridYPercent);
      const height = this.agentHeight(projection);
      const screen = this.projectWorldToScreen(new THREE.Vector3(target.x, target.y, height));
      return {
        ...projection,
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
    for (const projection of projections) {
      live.add(projection.id);
      const group = this.agentGroups.get(projection.id) ?? this.createAgent(projection);
      const target = gridToWorld(projection.gridXPercent, projection.gridYPercent);
      const height = this.agentHeight(projection);
      group.position.lerp(new THREE.Vector3(target.x, target.y, height), 0.22);
      group.scale.setScalar(0.9 + projection.z * 0.22 + projection.hover * 0.08);
      group.rotation.y += 0.006 + projection.expression * 0.004;
      const cup = group.userData.cup as THREE.Mesh | undefined;
      if (cup?.material instanceof THREE.MeshBasicMaterial) {
        cup.material.opacity = 0.46 + projection.hover * 0.22 + projection.acknowledgement * 0.18;
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
          const waveDepth = amplitude * (0.36 + projection.acknowledgement * 0.5 + projection.hover * 0.18);
          const waveRadius = 1.85 + bandIndex * 0.28 + projection.z * 0.42 + projection.hover * 0.22;
          const spatialFrequency = 3.8 + temporalFrequency * 5.6 + bandIndex * 0.85;
          const speed = 0.22 + temporalFrequency * 1.7 + Math.abs(chirp) * 4.5 + projection.acknowledgement * 0.9;
          const sinePower = 1.05 + bandIndex * 0.08 + Math.min(0.5, Math.abs(chirp) * 18);
          this.configureSplat(this.splatMeshes[splatIndex], target.x, target.y, waveRadius, waveDepth, 8, spatialFrequency, phase + bandIndex * 0.7, sinePower, speed);
          splatIndex += 1;
        }
      }
    }
    for (let index = splatIndex; index < this.splatMeshes.length; index += 1) {
      this.splatMeshes[index].visible = false;
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
        varying float vDepth;
        varying float vFade;

        void main() {
          vec3 displaced = position;
          vec2 uv = displaced.xy / vec2(${worldWidth.toFixed(3)}, ${worldDepth.toFixed(3)}) + 0.5;
          vec4 field = texture2D(uGravityTex, uv);
          float depth = field.r - field.g;
          displaced.z = -depth;
          vec2 edgeBasis = vec2(${(worldWidth / 2).toFixed(3)}, ${(worldDepth / 2).toFixed(3)});
          float edge = max(abs(displaced.x) / edgeBasis.x, abs(displaced.y) / edgeBasis.y);
          vDepth = depth;
          vFade = (1.0 - smoothstep(0.24, 1.0, edge)) * (1.0 - smoothstep(0.65, 1.8, depth));
          gl_Position = projectionMatrix * modelViewMatrix * vec4(displaced, 1.0);
        }
      `,
      fragmentShader: `
        uniform vec3 uGridColor;
        uniform float uOpacity;
        varying float vDepth;
        varying float vFade;

        void main() {
          float cup = smoothstep(0.02, 0.28, vDepth);
          vec3 color = mix(uGridColor * 0.38, vec3(0.82, 1.0, 0.9), cup);
          gl_FragColor = vec4(color, uOpacity * vFade);
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

  private createAgent(projection: SceneProjection) {
    const color = new THREE.Color(projection.color ?? "#8fffd3");
    const glow = new THREE.Color(projection.glow ?? projection.color ?? "#8fffd3");
    const group = new THREE.Group();
    const cup = new THREE.Mesh(
      new THREE.TorusGeometry(0.42, 0.035, 12, 64),
      new THREE.MeshBasicMaterial({ color: glow, opacity: 0.46, transparent: true, depthWrite: false }),
    );
    cup.position.z = -0.43;
    const anchor = new THREE.Mesh(
      new THREE.CylinderGeometry(0.015, 0.015, 0.78, 8),
      new THREE.MeshBasicMaterial({ color: glow, opacity: 0.36, transparent: true, depthWrite: false }),
    );
    anchor.rotation.x = Math.PI / 2;
    anchor.position.z = -0.04;
    const body = new THREE.Mesh(
      new THREE.OctahedronGeometry(0.24, 0),
      new THREE.MeshStandardMaterial({
        color,
        emissive: glow,
        emissiveIntensity: 0.42,
        metalness: 0.08,
        roughness: 0.38,
        transparent: true,
        opacity: 0.86,
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
    if (this.pointer.active) {
      const world = gridToWorld(this.pointer.xPercent, this.pointer.yPercent);
      this.cursor.visible = true;
      const projected = this.projectPointerToPlane(this.pointer);
      this.pointerWorld.copy(projected ?? new THREE.Vector3(world.x, world.y, 0));
      this.cursor.position.set(this.pointerWorld.x, this.pointerWorld.y, 0.04);
      this.cursor.rotation.z = millis * 0.0014;
    } else {
      this.cursor.visible = false;
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
    this.raf = requestAnimationFrame(this.render);
  };
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

function stablePhase(id: string) {
  let hash = 0;
  for (let index = 0; index < id.length; index += 1) {
    hash = (hash * 31 + id.charCodeAt(index)) >>> 0;
  }
  return (hash / 0xffffffff) * Math.PI * 2;
}
