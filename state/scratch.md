# Scratch

Current renderer truth:

- The live Bevy aquarium renderer has no G-buffer.
- The live Bevy aquarium renderer does not write prepass textures, deferred
  payloads, lighting-pass ids, motion vectors, material payloads, or Bevy light
  data for raymarched surfaces.
- Bevy hosts the window, ECS state, render graph scheduling, HDR view target,
  Bloom, ACES tonemapping, audio, and input.
- Aquarium owns its pixels. `AquariumRaymarchNode` dispatches Grid-height,
  brick-occupancy, and Grid-space irradiance compute, then `fs_main` renders
  directly into the HDR `ViewTarget` before Bloom/Tonemapping.
- The WGSL `SurfaceSample` carries only hit kind, point, normal, final HDR
  color, and ray distance. No `emissive`, `unlit`, `roughness`, `metallic`,
  clip matrices, or motion-vector compatibility fields remain.
- Debug modes are now final, hit coverage, depth, normals, brick occupancy, and
  irradiance luminance. The old motion-vector debug mode was part of the
  G-buffer era and is gone.
- Lighting is diegetic. The current readable pass shades Grid/body pixels from
  the aquarium-owned Grid-space irradiance volume. Self is the only emitter;
  probe cells trace cheap SDF visibility to Self, encode directional radiance
  into first-order SH, propagate/scatter through neighbors, and surfaces sample
  that field by normal. There is no global ambient or manual Self-lighting term
  in `shade_diegetic`.
- Body shading now uses normals from the same displaced body SDF used for hit
  refinement. The broad sphere is only a cheap cull; the visible hit is refined
  against `body_sdf`, and `body_normal` finite-differences that SDF so surface
  detail affects lighting instead of only silhouette.
- Body displacement now uses a fast Quilez-style 3D value-noise derivative
  basis. `noised3` returns value plus analytical gradient, which supplies the
  planet-local domain warp without six extra finite-difference samples.
  `body_bound_radius` is derived from `body_displacement_amplitude` plus margin,
  so broad phase remains a conservative contract rather than a shape shortcut.

Current next cut:

- Runtime-validate WGSL after any irradiance edit; `cargo check` proves Rust
  layout only, not shader entrypoint correctness.
- Inspect the live image and tune irradiance strength/visibility if planets go
  too black or the Grid starts emitting square ghosts again.
