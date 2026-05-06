# Aquarium Renderer Map

## Short Answer

The current renderer is **not** a brick map projected into a Wronski-style
froxel grid.

Right now the live renderer is a hybrid:

- a Three.js scene renders the displaced gravity grid, cursor, and stardust;
- a moving 2D gravity render target stores Aetheria-style height/energy fields;
- `public/textures/studio3.hdr` is loaded through Three `RGBELoader`, filtered
  through PMREM, and assigned to `scene.environment` for PBR lighting;
- a WebGPU field layer builds a froxel primitive mask in a storage buffer;
- a second WebGPU compute pass ping-pongs low-order spherical-harmonic lighting
  coefficients through froxel space;
- the WebGPU field renderer marches screen/frustum samples and evaluates only
  the primitives and SH lighting named by the current froxel;
- a WebGL analytic field-volume path remains as fallback when WebGPU is absent;
- DOM labels and menus are projected from the same Three camera, but remain DOM.

So the current preferred fog/planet path is now real compute: froxels store
which primitives intersect each cell. It is not yet a sparse brick map or full
Wronski lighting/integration cache.

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

### 5. WebGPU Froxel Primitive Map

`src/aquariumStardust.ts` now owns the preferred field renderer when WebGPU is
available.

It uses:

- one storage buffer of `u32` masks, one entry per froxel;
- two ping-ponged storage buffers of SH lighting, four `vec4` coefficients per
  froxel;
- one storage buffer of projected primitive data;
- one storage buffer of primitive colors / Self flags;
- one storage buffer containing a tiny sampled summary of `studio3.hdr`;
- one uniform buffer for screen size, time, primitive count, froxel size, and
  the near/far depth interval fitted to the projected grid volume.

The compute pass dispatches one invocation per froxel:

```wgsl
primitiveMasks[index] = mask;
```

Each bit in the mask names one possible primitive. With the current eight-agent
limit, one `u32` is enough. The important contract is:

```text
froxel cell -> primitive membership bitset
```

The froxel does **not** store pre-baked fog color/density as the primary truth.
It stores which fields are worth sampling in that region. The pixel pass then
evaluates those fields while marching through the froxel. This is the machine
we actually wanted; the earlier cached-density variant was the wrong little
office job.

The froxel `x/y` dimensions remain canvas-local and screen-shaped. They are not
scissored to the visible grid rectangle because the screen projection is the
right sampling domain for stable full-frame fog. The fitted part is the `z`
interval: Three projects the moving grid-volume bounds through the camera, and
WebGPU maps froxel depth slices into that near/far range. Agent primitives carry
their projected body depth, so primitive masks, SH propagation, and the field
march stop spending depth samples outside the grid volume.

### 6. WebGPU Froxel SH Lighting

After primitive membership is built, a second compute pass updates a first-order
lighting field in froxel space.

Each froxel stores four RGB coefficients:

```text
L0, Lx, Ly, Lz
```

This is deliberately low order. It is for soft directional volumetric lighting,
not mirror-perfect reflection. The pass:

1. reads the previous frame's SH coefficients;
2. blends in six-neighbor propagation;
3. injects environment lighting from grid-volume edges using a small sampled
   summary of `studio3.hdr`;
4. injects local emissive light from intersecting primitives, with Self acting
   as the warm solar emitter;
5. writes the next SH buffer.

This is propagation, not yet full camera-motion reprojection with history
validity. It still changes the ownership model in the important way: diffuse
volumetric light now belongs to froxel space. The previous baseline
screen-depth haze was removed because it did not move with the camera or scene.

### 7. WebGPU Field March

The WebGPU render pass draws a fullscreen triangle over the stardust/field
canvas. For each pixel:

1. March a small fixed number of frustum-depth samples.
2. Map the sample to a froxel cell.
3. Read that cell's primitive bitmask from the storage buffer.
4. Evaluate only the named planet/atmosphere primitives for both solid SDF
   hits and volumetric atmosphere density.
5. Sample the froxel's SH lighting coefficients for local diffuse gas and
   solid wrap lighting.
6. If the ray touches a solid SDF, shade that solid, preserve the accumulated
   in-scattering/transmittance up to the hit, and terminate the march.
7. Otherwise accumulate Beer-Lambert transmittance and in-scattering for the
   current volumetric sample.

The WebGL field-volume shader still exists as fallback, but Three marks it as
`webgpu-external-field` and skips that expensive path when WebGPU is available.
No CPU readback or WebGPU-to-WebGL texture shuttle is used.

The current WebGPU chrome shader does not yet sample the HDR texture directly;
it uses a sampled HDR summary for SH/environment response and a procedural
warm-key/cool-fill lobe for reflections. Direct HDR sampling in WebGPU needs a
parsed Radiance HDR texture or a prefiltered GPU-side environment resource owned
by the WebGPU graph, not a WebGL PMREM texture borrowed across APIs.

The solid and volumetric paths intentionally share the same primitive map. A
froxel says "these primitives may matter here"; the pixel ray then asks those
primitives whether they are solid, atmospheric, or both at the current sample.

### 8. Agent Planet SDFs

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
- Self gets higher amplitude and ridge-like loop emphasis.

The displacement frequencies are intentionally low. The planets should read as
chrome/gas bodies with slow organic surface motion, not sandpaper covered in
shader enthusiasm.

Non-Self bodies shade as chrome planets: reflected view/sky color, Fresnel, and
agent tint. Self shades as a solar body with warm emission, stronger plasma
noise, and corona-weighted gas.

The visible old Three geometry for bodies has been removed. Invisible Three
groups remain because DOM projection needs stable world anchors.

### 9. Gas and Atmosphere

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
- WebGPU compute pass that builds a camera/frustum froxel primitive mask;
- WebGPU compute pass that propagates first-order SH lighting in froxel space;
- edge environment injection from `studio3.hdr` summary;
- local Self emission injection;
- pixel march samples the primitive mask and SH lighting while evaluating only
  relevant fields.

Not implemented yet:

- density injection cache;
- depth-wise scattering/transmittance scan;
- camera-motion reprojection and history clipping for the SH field;
- composition by sampling accumulated froxel fog at scene depth.

The current WebGPU path is now an acceleration grid plus a small propagated
lighting cache. It is still not a full Wronski fog integration cache yet.

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

## Why Not Full Wronski Yet?

The current WebGL2 implementation is trying to prove the interaction grammar and
field ownership first:

- agents are fields, not DOM sprites;
- the grid, particles, fog, and bodies sample shared domains;
- DOM labels project from the same camera;
- Self and swarm cohesion have physical visual consequences.

A full Wronski path wants more infrastructure:

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

Preferred WebGPU cost is now closer to:

```text
froxel cells * cheap primitive overlap
froxel cells * SH propagation/injection
+ screen pixels * fog steps * primitives named by current froxel
```

The old WebGL fallback cost is still roughly:

```text
screen pixels * fog steps * maxFieldSources * SDF/noise cost
```

`?smoke=visual` lowers fog steps from 64 to 28.

This is why WebGPU matters here. The primitive map lets empty froxels skip most
agent SDF/atmosphere work instead of making every pixel sample every object like
it lost a bet.

## Current Renderer In One Sentence

Epiphany Aquarium currently renders a moving Aetheria-style gravity domain in
Three, then overlays a WebGPU compute-built froxel primitive map, propagated SH
lighting field, and SDF/atmosphere march that turns agents into chrome/solar
field planets, while DOM billboards are projected from the same camera for crisp
interaction.
