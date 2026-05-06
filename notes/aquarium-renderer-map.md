# Aquarium Renderer Map

## Short Answer

The current renderer is **not** a brick map projected into a Wronski-style
froxel grid.

Right now the live renderer is a hybrid:

- a Three.js scene renders the displaced gravity grid, cursor, and stardust;
- a moving 2D gravity render target stores Aetheria-style height/energy fields;
- a fullscreen field-volume shader reconstructs a camera ray per pixel;
- that shader raymarches analytic SDF solids and procedural gas directly;
- DOM labels and menus are projected from the same Three camera, but remain DOM.

So the current fog/planet path is a direct per-pixel raymarch over analytic
fields. It is not yet a froxel cache, sparse brick field, or compute-owned
volume.

## Pass Graph Today

### 1. Source Projection

React and the fluid/orbit layer produce `AquariumAgentProjection` records.
`src/aquariumScene3d.ts` maps those into:

- invisible Three groups used as world anchors for DOM projection;
- gravity splat quads for the deferred 2D gravity target;
- stardust source uniforms;
- field-volume source uniforms.

Field-volume source layout:

- `uFieldSources[i].xy`: world-space grid position;
- `uFieldSources[i].z`: mass/activity scalar;
- `uFieldSources[i].w`: height scalar;
- `uFieldColors[i].rgb`: body/atmosphere color;
- `uFieldColors[i].w`: Self flag.

This is deliberately small and crude. It is a source list, not a spatial
acceleration structure.

### 2. Deferred Gravity Field

The gravity field is a 256x256 half-float render target.

The renderer draws additive splat quads into it using an orthographic top-down
camera aligned to the current grid domain. Each splat writes:

- positive height in `r`;
- negative height in `g`;
- absolute energy in `b`;
- source coverage in `a`.

Splat shapes use the Aetheria `PowerPulse` envelope. Static wells, the large
Self/swarm divot, cursor attraction, and low-frequency chirp waves all enter
through this same texture. This is the most Aetheria-shaped part of the current
renderer: sparse source quads accumulate into a grid field once, and visible
surface/grid/stardust/fog sample the field.

### 3. Displaced Grid Mesh

The visible grid is a tessellated Three plane parented to the moving grid
domain. Its vertex shader samples the gravity texture and displaces `z` by the
height difference:

```glsl
float depth = field.r - field.g;
displaced.z = -depth;
```

Grid scale follows camera zoom. Grid origin follows the camera target. The mesh
fades near its moving-domain edges so the finite field window does not announce
itself like a cheap stage curtain.

### 4. Stardust

Stardust is currently Three `Points`, parented to the grid group and rendered
through the same Three/ACES path.

The particle buffer slot is not identity. Each slot is a cell offset around the
moving gravity origin. The shader hashes world-space cell coordinates to derive
jitter, lifetime, color, height, and local phase. That is the Aetheria moving
domain trick: when the camera moves, different buffer slots take over different
world cells without visible discontinuity.

Stardust samples the gravity texture for local grid height, spawns with an
exponential vertical distribution around the grid, and fades near the grid
domain edge.

### 5. Field-Volume Fullscreen Raymarch

After the Three scene render, the renderer draws a fullscreen plane with
`createFieldVolumeMaterial()`.

For each pixel:

1. Convert screen UV to NDC.
2. Use the inverse camera projection and camera world matrix to reconstruct a
   world-space ray.
3. March the ray with jittered quadratic spacing.
4. At each sample:
   - evaluate the nearest agent planet SDF;
   - if solid is hit, shade it and stop;
   - otherwise evaluate dynamic gas density;
   - integrate Beer-Lambert transmittance and in-scattering.

This is the important part: the shader marches the pixel ray directly. There is
no precomputed froxel volume between source fields and final composition.

### 6. Agent Planet SDFs

Agent bodies are analytic SDF planets:

```glsl
float planetSdf(vec3 p, vec4 source, float selfFlag) {
  vec3 center = vec3(source.xy, 0.54 + source.w * 0.55);
  float radius = sourceRadius(source);
  vec3 local = p - center;
  return length(local) - radius - sourceDisplacement(local, radius, source.z, selfFlag);
}
```

Displacement uses cheap 4D fBm:

- local planet xyz is the spatial domain;
- time is the fourth coordinate;
- Self gets higher frequency/amplitude and ridge-like loop emphasis.

Non-Self bodies shade as chrome planets: reflected view/sky color, Fresnel, and
agent tint. Self shades as a solar body with warm emission, stronger plasma
noise, and corona-weighted gas.

The visible old Three geometry for bodies has been removed. Invisible Three
groups remain because DOM projection needs stable world anchors.

### 7. Gas and Atmosphere

Gas density has several terms:

- a broad camera-target ellipsoid;
- grid-hugging surface fog based on gravity texture height;
- per-agent atmosphere shells based on planet SDF distance;
- Self-biased corona/plasma loop noise;
- pointer fog;
- phase-paired triangle-noise flow inspired by Aetheria.

Density is integrated per pixel:

```glsl
float stepTransmittance = exp(-extinction * stepSize);
scattering += transmittance
  * (luminance - luminance * stepTransmittance)
  / max(extinction, 0.0001);
transmittance *= stepTransmittance;
```

Solid hits use stochastic coverage:

```glsl
float noise = hash(gl_FragCoord.xy + floor(uTime * 60.0) * 1.61803398875);
if (noise > coverage) discard;
```

That preserves the Aetheria transparency contract in miniature: transparent-ish
field objects should not become naive alpha soup when they need to coexist with
fog and depth-sensitive composition. We do not yet have the proper TAA resolve,
so this is a placeholder with the right shape, not the finished law.

## What This Borrows From Each Doctrine

### Aetheria

Implemented now:

- moving grid domain;
- sparse source splats into a grid-aligned render target;
- `PowerPulse` wells and wave emitters;
- stardust using hash-stable world cells;
- phase-paired triangle-noise fog flow;
- stochastic transparency assumptions.

Not fully implemented:

- proper temporal reprojection/history clipping;
- blue-noise texture coverage and TAA reconciliation;
- fully unified HDR/depth-aware compositor.

### Wronski Froxel Fog

Implemented now:

- Beer-Lambert transmittance/in-scattering math;
- separation of density sources from integration in concept only.

Not implemented yet:

- camera/frustum-aligned froxel texture;
- density/light injection pass;
- depth-wise scattering/transmittance scan;
- composition by sampling accumulated froxel fog at scene depth.

The current raymarch is conceptually compatible with a Wronski path, but it is
not that path.

### Gigavoxels / Brick Maps

Implemented now:

- none of the brick pool machinery.

Influencing the next design:

- visible-demand refinement;
- sparse page/brick ownership;
- coarse fallback for missing detail;
- bounded resident GPU memory.

There is currently no sparse voxel tree, no brick atlas, no residency map, no
miss feedback, and no brick streaming. The renderer is source-list plus
procedural field math.

### Dreams

Implemented now:

- SDFs as source truth rather than traditional meshes;
- stochastic coverage assumptions;
- visible output allowed to be painterly/noisy instead of literal mesh purity.

Not implemented:

- edit-list SDF authoring;
- point/fleck surface extraction;
- clustered point-cloud LOD.

### Bruneton

Implemented now:

- no atmospheric precompute.

Intended role:

- global sky and aerial perspective should become cached transport fields;
- local dynamic fog should be lit/composited against those fields rather than
  solving horizon-scale multiple scattering every frame.

## Why Not Froxels Yet?

The current WebGL2 implementation is trying to prove the interaction grammar and
field ownership first:

- agents are fields, not DOM sprites;
- the grid, particles, fog, and bodies sample shared domains;
- DOM labels project from the same camera;
- Self and swarm cohesion have physical visual consequences.

A real Wronski path wants more infrastructure:

- a 3D froxel texture or packed 2D atlas;
- injection passes for density/light/source terms;
- temporal history and reprojection;
- depth-aware composition;
- debug views for slices and accumulated transmittance.

Doing that before the field vocabulary is stable would produce a very expensive
box full of uncertainty. Classic renderer mistake. Very dignified. Very dumb.

## Intended Next Architecture

The likely grown-up path is:

1. **2D source fields**
   - Keep Aetheria-style gravity, wave, and flow fields as moving 2D textures.
   - Continue splatting sparse sources instead of making every sample enumerate
     every source.

2. **SDF source list**
   - Keep analytic agent planets and gassy SDF volumes as typed sources.
   - Move the source model out of shader literals into a proper CPU/GPU schema.

3. **Optional sparse brick field**
   - Use brick maps only for fields that cannot be cheaply derived at sample
     time: persistent volumetric memory structures, complex fog authored over
     space, or high-detail cached lighting.
   - Do not brick-map simple planet SDFs just to feel serious.

4. **Froxel injection**
   - Project dynamic sources into a camera-aligned froxel grid each frame.
   - Inject density, emission, anisotropy, local light, and surface-coupled fog.
   - Sample 2D gravity/flow fields during injection.

5. **Froxel integration**
   - Run a depth-wise scan over froxels for transmittance and in-scattering.
   - Use jitter and temporal reprojection instead of brute-force per-pixel long
     marches.

6. **Composition**
   - Render solid SDF planets either by direct SDF hit pass or by a dedicated
     depth/coverage prepass.
   - Composite fog against scene depth.
   - Resolve stochastic coverage with TAA/blue-noise history.

7. **Global atmosphere**
   - Add Bruneton-style cached atmospheric transport for sky/aerial perspective.
   - Let local fog query those fields as lighting context.

## Current Cost Shape

Current cost is roughly:

```text
screen pixels * fog steps * maxFieldSources * SDF/noise cost
```

`?smoke=visual` lowers fog steps from 64 to 28.

This is why the first nested 4D value-noise planet draft was cut. It built, but
it was the wrong cost shape for a per-pixel raymarch. The live shader uses a
compact analytic 4D fBm instead.

## Current Renderer In One Sentence

Epiphany Aquarium currently renders a moving Aetheria-style gravity domain in
Three, then overlays a direct fullscreen analytic SDF/gas raymarch that turns
agents into chrome/solar field planets, while DOM billboards are projected from
the same camera for crisp interaction.
