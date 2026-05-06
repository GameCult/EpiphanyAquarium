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

## WGPU Discipline

Useful WGPU mental model:

- source/state lives in storage buffers, storage textures, and sampled textures;
- render graph nodes own pass ordering and bind group contracts;
- CPU readback is not part of the frame path;
- WGSL validation happens at runtime specialization, not at Rust `cargo check`;
- debug labels and `VERBOSE_SHADER_ERROR=1` are operational tools, not luxuries.

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

## Next Renderer Questions

- Should the density field be integrated per-pixel with stochastic steps first,
  or should it move directly into a Wronski-style froxel scan?
- Which Grid source textures should live as CultCache-persisted authored state
  versus transient GPU fields?
- How much of Bevy's built-in atmosphere stack can supply global sky/aerial
  context without fighting the local Grid nebula?
- Where should diagnostic modes live so black-frame bugs expose missing hit
  coverage, missing lighting, or graph ordering separately?
