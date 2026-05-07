# Scratch

Current pivot truth:

- The repo split is complete. Public repos:
  - `https://github.com/GameCult/EpiphanyAquarium-Web`
  - `https://github.com/GameCult/EpiphanyAquarium-Bevy`
  - `https://github.com/GameCult/AquariumSynth`
  - `https://github.com/GameCult/Aquarium-Engine`
- Local project roots now live directly under `E:\Projects`:
  - `E:\Projects\EpiphanyAquarium-Web`
  - `E:\Projects\EpiphanyAquarium-Bevy`
  - `E:\Projects\AquariumSynth`
  - `E:\Projects\Aquarium-Engine`
- Split map lives at `notes/repository-split-map.md`.
- The Bevy/Rust branch is frozen as prototype/reference, not the future host.
- The next target is C# land: use Stride as scaffolding and parts donor where
  useful, but build an Aquarium-owned engine core that owns the frame, render
  graph, field pipeline, debug UI, CultCache/CultNet integration, and taste.
- The branch retrospective lives at `notes/rust-branch-retrospective.md`.
- `crates/aquarium_synth` is salvageable real work. Keep it as a Rust oracle
  and port the patch model/analyzer/performance lessons to C# when audio returns.
- Graph/layout work remains valuable as data-first tooling: serializable graph
  layouts, evidence/code-ref projections, and icon-branch/leaf-surface UI
  doctrine should carry forward.

Frozen Bevy renderer truth:

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
- Non-Self planet displacement is intentionally exaggerated: low-frequency
  domain-warped noise plus three cheap directional macro lobes, with declared
  displacement amplitude raised to 18% of radius. Self keeps its existing lower
  solar/blob frequency mix for now.
- Smooth-potato correction: ordinary planets now add high-frequency ridged
  detail in the same warped local domain. The fine layer has much lower
  amplitude than the macro lobes so normals get tooth without wrecking the
  readable silhouette.
- Body surface domains must be translation-invariant. Cursor pull made the bug
  visible because it moved planets faster than idle orbit, but the real fault
  was identity/domain seed coupling to body position and shader slot. The CPU
  now writes a stable per-body seed derived from `body_id`; WGSL samples that
  seed while using `(point - body.xyz) / radius` as the local domain.
- Windows link failures mentioning unresolved `anon.*.llvm.*` symbols from
  `libaquarium_synth` have reproduced as stale incremental package artifacts.
  `scripts/repair-bevy-link.ps1` performs the narrow recovery: `cargo clean -p
  aquarium_synth`, then rebuilds the Bevy hotpatch target.
- Debug UI exists as a native Bevy UI escape hatch, not the main aquarium
  grammar: a tiny top-left `>_` button opens square tab buttons and a left-half
  terminal panel. The first tab is a focused registered-command terminal; camera
  input yields while the terminal owns keyboard focus.

Current next cut:

- Start the C# host/scaffold map in `GameCult/Aquarium-Engine` before
  writing renderer code.
- Keep Bevy changes limited to documentation, salvage extraction, or
  build-preservation unless explicitly reactivated.
- If a shell terminal is added later in the new host, make it a separate loudly
  labeled tab with confirmation/logging. The default debug terminal is an
  in-process command DSL.
