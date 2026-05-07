# Rust VFX Research Map

## Current Bias

Epiphany Aquarium should behave like a lean Rust-native renderer now. Bevy owns
the app and ECS source state; WGPU/WGSL owns field computation; CultNet carries
agent messages; CultCache carries settings, live state, and hot-reload recovery.
The old web stack is useful only as migration archaeology and interaction
prototype evidence.

## Bevy 0.18 Signals

Bevy 0.18 matters because it moves closer to the shape Aquarium already needs:

- generalized atmospheric scattering media;
- atmosphere occlusion affecting scene lighting;
- PBR material fixes;
- Solari realtime raytracing improvements;
- high-level fullscreen materials for simple post effects;
- scenario-oriented Cargo feature collections;
- first-party camera controllers useful as tooling references.

Aquarium should use the high-level features where they fit, but the core field
renderer stays custom. Fullscreen materials are good for small post effects;
the aquarium raymarch, deferred prepass writes, SH propagation, brick occupancy,
and future density scans need explicit render graph ownership.

The important prepass contract is already in Bevy: `DepthPrepass`,
`NormalPrepass`, `MotionVectorPrepass`, and `DeferredPrepass` add
`ViewPrepassTextures` to the view. Those textures carry depth, normals, motion,
deferred material data, and deferred lighting pass id. Aquarium's raymarcher is
correct to write there because later deferred lighting, TAA, and screen-space
effects can consume the same surfaces as mesh materials.

Bevy 0.18's `ScatteringMedium` should be treated as global atmosphere
infrastructure. It can teach parameter shape and maybe supply sky/aerial
context, but Aetheria-style local nebulae are not generic atmospheres. They are
Grid-domain weather fields with authored height/patch/tint/flow source maps.

## WGPU Discipline

Useful WGPU mental model:

- source/state lives in storage buffers, storage textures, and sampled textures;
- render graph nodes own pass ordering and bind group contracts;
- CPU readback is not part of the frame path;
- WGSL validation happens at runtime specialization, not at Rust `cargo check`;
- debug labels and `VERBOSE_SHADER_ERROR=1` are operational tools, not luxuries.
- use `Device::push_error_scope` around risky pipeline/bind-group/resource
  creation so validation failures become renderer data instead of surprise
  panics;
- label resources and wrap compute/render passes with debug groups/markers so
  RenderDoc, WGPU logs, and panic messages point at the real pass;
- use timestamp queries when available to profile GPU time per pass; a CPU
  frame timer cannot tell whether SH propagation, brick update, raymarch, or
  composition ate the frame;
- choose texture usages deliberately. A texture sampled after a compute write
  needs storage and texture binding usage; transient render attachments are only
  for data that truly dies inside the pass.
- portable storage-texture design should assume write-only storage outputs and
  separate sampled inputs unless a native-only read/write feature is explicitly
  chosen. Ping-pong textures/buffers are not boilerplate; they are the contract
  that keeps advection, reprojection, and SH propagation legible.

For Aquarium, every field pass should name its domain:

- Grid-space: gravity, terrain height, nebula density/tint, SH lighting, brick
  occupancy;
- view/froxel-space: depth-bounded integration, temporal reprojection, final
  composition against scene depth;
- world-space: agent anchors, orbit/spring state, cursor projection, CultCache
  body state.

## Particles

Bevy Hanabi is the Rust ecosystem reference for GPU-first particles: simulation
is compute-driven, scalable, and minimizes CPU intervention. Aquarium should
study its buffer layouts and effect authoring, but Aetheria-style stardust has a
different identity contract: particles are deterministic readings of a moving
world domain, so buffer slots can change jobs without visible discontinuity.

The target particle model is therefore:

- world-domain hashing for spawn position, color, height, lifetime, and phase;
- GPU compute/update where persistent state is truly needed;
- field sampling from Grid flow/lighting;
- edge fades and overlapping windows so moving domains never reveal seams.

Aetheria's `Stardust.compute` is the model to steal, not merely admire. It
indexes a flat particle buffer as a moving Grid-space lattice, hashes world cell
coordinates for stable randomness, samples nebula height/tint/flow, distributes
height exponentially through available headroom, then moves particles backward
through the flow by lifetime. The buffer stores workers; world-domain hashes own
identity. That is the right answer for a million faint particles.

## Aetheria Nebula Lesson

Aetheria's nebula is wild because it is disciplined. The shader couples every
visual flourish to a field:

- `_NebulaSurfaceHeight` gives terrain-relative density and slope;
- `_NebulaPatch` and `_NebulaPatchHeight` localize floor/patch density;
- `_NebulaTint` provides material color and density-driven LOD;
- global and slope flow vectors advect procedural triangle noise;
- phase-paired noise crossfades continuity through time;
- quadratic steps, blue-noise offsets, temporal reprojection, and AABB clipping
  make cheap sampling look expensive.

The Bevy rebuild should preserve those contracts while changing the machinery:
explicit Grid source textures, WGPU compute/injection passes, view-depth fitting,
history buffers, and deferred composition.

More specific source map:

- `VolumeSampling.cs` publishes all shared nebula parameters and source
  textures as globals: surface height, patch height, patch density, tint,
  density controls, tint LOD, flow controls, noise controls, dynamic lighting
  LODs, and safety distance.
- `VolumeCloudRenderer.cs` owns the temporal frame machine: downsampled
  current/history buffers, first-frame ultra sampling, Halton ray offsets,
  previous view matrix, and a three-pass pipeline of current raymarch, history
  reprojection, and final composition.
- `CloudShader.shader` performs quadratic distance stepping, combines Halton
  and blue-noise offsets, integrates Beer-Lambert extinction/in-scattering,
  terminates when transmittance saturates, reprojects history with previous VP,
  clips history against a 3x3 current-neighborhood AABB, and composites as
  `scene * transmittance + fog`.
- `BlueNoiseProvider.cs` supplies a screen-scaled blue-noise texture and
  golden-ratio frame offset for stochastic coverage. The same idea appears in
  dithered particles, which makes temporal reconstruction a system-wide
  contract rather than a one-off trick.

The deeper principle: the nebula is cheap because it is temporally honest. It
does not pretend a low sample count is enough in one frame. It jitters, stores,
clips, and converges.

## Shader Organization

WGSL should not become one giant file just because the old Unity shader was one
include-heavy organism. The likely path:

- keep current Bevy `#import`/naga_oil style while close to Bevy internals;
- split reusable math into Grid, noise, temporal, SH, and packing modules;
- consider WESL when Bevy's WESL path is stable enough for our custom renderer;
- use `encase`/`ShaderType` for host-shareable structs instead of manual padding
  theater when data shapes become larger than a few uniforms.

Manual layout mistakes in a field renderer are especially stupid because they
look like art bugs. Let the tooling do the boring alignment work.

## Diagnostics Doctrine

Each renderer stage needs an explicit debug output:

- hit coverage: show terrain/body hit ids before lighting;
- brick occupancy: show the 8x8x4 flags projected into the view;
- SH lighting: slice or luminance debug of propagated coefficients;
- density: show injected density before integration;
- temporal history: show rejection/out-of-bounds masks;
- composition: show transmittance and in-scatter separately.

Black-screen bugs should collapse into one of those categories. If they do not,
the diagnostic layer is lying by omission.

Bevy and WGPU already provide some teeth:

- `FrameTimeDiagnosticsPlugin` gives CPU-side frame/fps history;
- `RenderDiagnosticsPlugin` can collect render diagnostics including CPU/GPU
  elapsed time per pass and pipeline statistics. Use this first because it is
  already Bevy-native; reach for `wgpu-profiler` when nested custom GPU scopes
  need more detail than Bevy's diagnostics surface exposes;
- `wgpu-profiler` wraps timestamp queries into named GPU scopes and can export
  traces; it manages query sets/buffers, supports nested scopes around encoders
  or passes, resolves queries into buffers, and processes finished frames using
  the queue timestamp period;
- `renderdoc`/RenderDoc integration is the frame-forensics path for native
  graphics debugging;
- WGPU error scopes should wrap pipeline and bind group creation during renderer
  bring-up.

The medicine: no more "it looks black" as a diagnostic category. The renderer
needs named pass timings, validation scopes, and one-button debug views. Anything
less is just expensive guessing in a nice coat.

## Current Bevy Renderer Reality

The live `bevy-aquarium` renderer has already crossed the important line:
raymarched surfaces are not traditional Bevy meshes pretending to be planets.
The app inserts `DefaultOpaqueRendererMethod::deferred()`, adds a custom
`AquariumDeferredPrepassNode` after `Node3d::LateDeferredPrepass`, runs compute
for brick occupancy and SH grid lighting, then writes normal, motion-vector,
deferred material, lighting-pass-id, and depth outputs through
`fs_deferred_prepass`.

That is the right ownership seam, but the renderer is still under-instrumented:

- `aquarium_light_compute` dispatches brick occupancy and SH propagation in one
  compute pass with no visible debug mode and no per-stage timing.
- `aquarium_raymarch_deferred_prepass` writes the G-buffer, but there is no
  user-facing view for hit ids, terrain crossings, normal quality, motion
  vectors, deferred payloads, or depth coverage.
- The WGSL uses analytic `grid_height` in both terrain and lighting paths.
  That is acceptable as a bootstrap, but the Aetheria-shaped target is explicit
  Grid source textures so height, density, tint, stardust, and fog all sample
  the same authored fields.
- Froxel masks currently live inside the uniform payload as packed `UVec4`
  words produced on the CPU. That is fine for eight bodies; it is not the
  destination. GPU brick/froxel occupancy should become storage data owned by
  compute once moving primitives and Grid-domain terrain get larger.

The next implementation step is not more beauty. It is a renderer truth panel:
debug mode persisted through CultCache, pass labels/timings, and draw modes that
prove each stage before final lighting hides the evidence.

## Temporal Resolve Contract

Bevy's TAA path is compatible with the direction, but it is not forgiving:
`TemporalAntiAliasing` is for perspective 3D cameras, requires depth and motion
vectors, disables MSAA, and expects every visible surface to write correct
motion. That makes Aquarium's deferred-prepass integration the right seam for
noisy raymarched surfaces, stochastic terrain detail, and future blue-noise
coverage.

Implications:

- Raymarched terrain and planets must output stable depth and motion vectors, or
  TAA will smear the field and make debugging worse.
- Transparent-looking field effects that must participate in TAA/depth should
  prefer stochastic coverage or volumetric accumulation over ordinary alpha
  blending.
- Camera cuts, Grid scale jumps, and hot reload rehydration need a way to reset
  temporal history. That reset bit should be driven by CultCache/state changes,
  not left as a hidden renderer panic button.
- Any future particle system that renders before TAA must either write motion
  vectors or use the Aetheria-style stochastic/depth contract. Rendering after
  TAA is valid only for effects that are intentionally not part of the fog/depth
  world.

## Built-In Bevy Fog Boundary

Bevy has a `VolumetricFog` camera component and 0.18's broader atmosphere work,
including `ScatteringMedium`, so the engine already contains useful production
language for fog quality, light shafts, atmosphere occlusion, and scattering
media. Aquarium should not ignore that. It also should not confuse it with the
Aetheria nebula contract.

Use Bevy fog/atmosphere as:

- a reference for parameter naming, quality knobs, and integration with Bevy's
  camera/light/PBR systems;
- a possible global sky or simple atmospheric baseline;
- an interoperability target for how Aquarium samples scene depth and lighting.

Do not use it as the local Grid weather engine unless it can consume the same
Grid source textures: surface height, patch density, patch height, tint, flow,
blue-noise temporal history, and CultCache-authored environment settings. The
local nebula is authored field state. A camera fog component is not allowed to
erase that distinction just because it compiles.

## Next Renderer Questions

1. Add renderer diagnostics before adding more beauty.
   - Current code has labels on buffers/pipelines/passes, but no user-facing
     debug views for coverage, bricks, SH, density, history, or composition.
   - Add a small `RendererDebugMode` persisted in CultCache.
   - Add pass timing through Bevy render diagnostics or `wgpu-profiler` once
     the render graph shape stops wobbling.
   - First commit should be deliberately boring: `RendererDebugMode` in
     CultCache, a key/control path to cycle it, and WGSL branches that output
     hit coverage, depth, normals, motion magnitude, brick occupancy, and SH
     luminance instead of final material payloads.

2. Rebuild Aetheria density as Grid source textures.
   - Start with CPU/CultCache-authored settings matching `NebulaSettings`,
     `FlowSettings`, `NoiseSettings`, and `AmbientLightingSettings`.
   - Add GPU source textures for surface height, patch density, patch height,
     and tint.
   - Keep world-space Grid transform as the only domain conversion authority.
   - Second commit should remove duplicated analytic `grid_height` as the
     renderer's long-term truth. It may keep the analytic function as the source
     generator, but the raymarch and lighting passes should sample the produced
     Grid texture.

3. Restore fog in the smallest honest temporal form.
   - First version can be stochastic per-pixel integration using Aetheria's
     quadratic steps, blue-noise/Halton offsets, transmittance, and history
     clipping.
   - Wronski-style froxel scan comes after debug views and history are real, not
     before. Otherwise it is just a larger black box.
   - Third commit should add current/history low-res fog targets and history
     validity visualization before adding brick sparsity, octrees, or gassy SDF
     flourish.

4. Move stardust to the Aetheria worker-slot model.
   - Use Grid-space cell hashing for identity.
   - Sample height/tint/flow fields.
   - Let buffer slots move with the Grid domain without visible discontinuity.

5. Decide Bevy atmosphere boundaries.
   - Use `ScatteringMedium` for global sky and aerial perspective if it can
     remain outside the local Grid fog machinery.
   - Do not force Aquarium's nebula into Bevy's global atmosphere if it loses
     terrain/patch/tint/flow authorship.
