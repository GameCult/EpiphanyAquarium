# Scratch

Current renderer truth:

- The live Bevy aquarium renderer has no G-buffer.
- The live Bevy aquarium renderer does not write prepass textures, deferred
  payloads, lighting-pass ids, motion vectors, material payloads, or Bevy light
  data for raymarched surfaces.
- Bevy hosts the window, ECS state, render graph scheduling, HDR view target,
  Bloom, ACES tonemapping, audio, and input.
- Aquarium owns its pixels. `AquariumRaymarchNode` dispatches Grid-height,
  brick-occupancy, and SH compute, then `fs_main` renders directly into the HDR
  `ViewTarget` before Bloom/Tonemapping.
- The WGSL `SurfaceSample` carries only hit kind, point, normal, final HDR
  color, and ray distance. No `emissive`, `unlit`, `roughness`, `metallic`,
  clip matrices, or motion-vector compatibility fields remain.
- Debug modes are now final, hit coverage, depth, normals, brick occupancy, and
  SH luminance. The old motion-vector debug mode was part of the G-buffer era
  and is gone.
- Lighting is diegetic. The current readable pass shades Grid/body pixels from
  Self's world position. Future indirect light should come from aquarium-owned
  volumetric/SH fields, not Bevy deferred reconstruction.

Current next cut:

- Inspect the live image after the purge and tune the HDR raymarch composition
  if alpha/color looks too flat after removing the old protocol baggage.
- Keep the renderer data model lean: add a buffer only when a pass consumes it
  directly, and remove every field that exists only to satisfy an abandoned
  integration path.
