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

export interface AquariumScene3d {
  dispose(): void;
  setPointer(pointer: PointerState): void;
  setProjections(projections: SceneProjection[]): void;
}

const worldWidth = 10;
const worldDepth = 7.2;

export function createAquariumScene3d(canvas: HTMLCanvasElement): AquariumScene3d {
  return new ThreeAquariumScene(canvas);
}

class ThreeAquariumScene implements AquariumScene3d {
  private agentGroups = new Map<string, THREE.Group>();
  private camera = new THREE.PerspectiveCamera(42, 1, 0.1, 80);
  private cursor = new THREE.Group();
  private disposed = false;
  private pointer: PointerState = { active: false, xPercent: 50, yPercent: 50 };
  private raf = 0;
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();

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
    this.camera.position.set(0, 6.2, 7.8);
    this.camera.lookAt(0, 0, 0.2);
    this.scene.add(new THREE.AmbientLight(0xbfffe8, 0.74));
    const key = new THREE.DirectionalLight(0xd8fff0, 1.2);
    key.position.set(-4.5, 8, 5.2);
    this.scene.add(key);
    this.scene.add(this.createGrid());
    this.scene.add(this.createCursor());
    this.raf = requestAnimationFrame(this.render);
  }

  dispose() {
    this.disposed = true;
    cancelAnimationFrame(this.raf);
    this.scene.traverse((object) => {
      if (object instanceof THREE.Mesh || object instanceof THREE.LineSegments || object instanceof THREE.Line) {
        object.geometry?.dispose();
        const materials = Array.isArray(object.material) ? object.material : [object.material];
        materials.forEach((material) => material.dispose());
      }
    });
    this.renderer.dispose();
  }

  setPointer(pointer: PointerState) {
    this.pointer = pointer;
  }

  setProjections(projections: SceneProjection[]) {
    const live = new Set<string>();
    for (const projection of projections) {
      live.add(projection.id);
      const group = this.agentGroups.get(projection.id) ?? this.createAgent(projection);
      const target = gridToWorld(projection.gridXPercent, projection.gridYPercent);
      const height = 0.44 + projection.z * 0.95;
      group.position.lerp(new THREE.Vector3(target.x, height, target.z), 0.22);
      group.scale.setScalar(0.9 + projection.z * 0.22 + projection.hover * 0.08);
      group.rotation.y += 0.006 + projection.expression * 0.004;
      const cup = group.userData.cup as THREE.Mesh | undefined;
      if (cup?.material instanceof THREE.MeshBasicMaterial) {
        cup.material.opacity = 0.46 + projection.hover * 0.22 + projection.acknowledgement * 0.18;
      }
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
    const plane = new THREE.Mesh(
      new THREE.PlaneGeometry(worldWidth, worldDepth, 48, 32),
      new THREE.MeshBasicMaterial({
        color: 0x16876e,
        opacity: 0.24,
        transparent: true,
        depthWrite: false,
        side: THREE.DoubleSide,
      }),
    );
    plane.rotation.x = -Math.PI / 2;
    group.add(plane);

    const grid = new THREE.GridHelper(10.4, 32, 0x67ffd5, 0x2c8975);
    grid.position.y = 0.012;
    if (grid.material instanceof THREE.Material) {
      grid.material.transparent = true;
      grid.material.opacity = 0.48;
      grid.material.depthWrite = false;
    }
    group.add(grid);
    return group;
  }

  private createCursor() {
    const ring = new THREE.Mesh(
      new THREE.TorusGeometry(0.22, 0.012, 8, 48),
      new THREE.MeshBasicMaterial({ color: 0xe9ffb0, opacity: 0.62, transparent: true, depthWrite: false }),
    );
    ring.rotation.x = Math.PI / 2;
    const beam = new THREE.Mesh(
      new THREE.CylinderGeometry(0.012, 0.028, 1.2, 10, 1, true),
      new THREE.MeshBasicMaterial({ color: 0x80ffd5, opacity: 0.18, transparent: true, depthWrite: false }),
    );
    beam.position.y = 0.6;
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
    cup.rotation.x = Math.PI / 2;
    cup.position.y = -0.43;
    const anchor = new THREE.Mesh(
      new THREE.CylinderGeometry(0.015, 0.015, 0.78, 8),
      new THREE.MeshBasicMaterial({ color: glow, opacity: 0.36, transparent: true, depthWrite: false }),
    );
    anchor.position.y = -0.04;
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
      this.cursor.position.set(world.x, 0.04, world.z);
      this.cursor.rotation.y = millis * 0.0014;
    } else {
      this.cursor.visible = false;
    }
    this.renderer.render(this.scene, this.camera);
    this.canvas.dataset.threeReady = "true";
    this.canvas.dataset.threeAgents = String(this.agentGroups.size);
    this.raf = requestAnimationFrame(this.render);
  };
}

function gridToWorld(xPercent: number, yPercent: number) {
  return {
    x: (xPercent / 100 - 0.5) * worldWidth,
    z: (yPercent / 100 - 0.5) * worldDepth,
  };
}
