# Aetheria Volumetric Fog Map

## Pipeline

Aetheria's nebula/fog path is a camera post effect over a grid-owned world
volume, not a precomputed 3D fog texture. The live frame flow is:

1. `VolumeSampling` publishes the moving grid domain:
   `_GridTransform`, `_NebulaSurfaceHeight`, `_NebulaPatchHeight`,
   `_NebulaPatch`, `_NebulaTint`, `_FluidVelocity`, and the environment knobs
   for density, noise, flow, lighting, extinction, and safety distance.
2. `VolumeCloudRenderer.OnRenderImage` allocates a downsampled HDR history pair
   and an undersample target, chooses 32/64/128/256 raymarch samples by quality,
   and forces ultra quality on the first frame to seed history.
3. `CloudShader` pass 0 raymarches from the camera through scene depth. Each
   pixel uses Halton plus blue-noise offset, quadratic distance spacing, and
   Beer-Lambert transmittance/in-scattering accumulation.
4. `CloudShader` pass 1 temporally reprojects the previous cloud buffer with
   previous view-projection, clips history to a local AABB from a 3x3 current
   neighborhood, and blends current fog into history.
5. `CloudShader` pass 2 composites scene color with accumulated fog:
   scene color is multiplied by remaining transmittance and fog intensity is
   added.

## Density Model

`Volumetric.cginc` is the real trick. Density is sampled from world position:

- Horizontal coordinates map into the moving grid texture domain through
  `_GridTransform`.
- `_NebulaSurfaceHeight` defines the displaced gravity/floor surface.
- `_NebulaPatch`, `_NebulaPatchHeight`, and `_NebulaTint` add localized cloud
  density and color over that surface.
- A baseline fill uses an inverse-powered smoothstep around the surface:
  extremely low fill density can still accumulate over horizon-length rays.
- Patch/floor density then adds thicker near-surface fog.
- Tint samples higher mip levels as density changes, so distant/low-frequency
  color can survive cheap sampling.

This makes the fog hug the same grid geography as gravity, planets, stardust,
and the minimap. It is not generic soup pasted over the camera.

## Flow And Noise

The flow is mostly procedural sampling, not stored 3D advection:

- `Tri3D` builds a cheap direction field from cross products of normalized
  triangle-noise vectors.
- `globalFlow` scrolls that field vertically by `_FlowScroll` and scales it by
  `_FlowScale` and `_FlowAmplitude`.
- Optional slope flow samples the gravity height gradient and produces along-
  slope and swirl components.
- The density function samples two low-frequency triangle-noise phases half a
  period apart, weights them with triangular phase windows, and displaces the
  raymarch sample height by their sum.
- Domain warping is part of the texture grammar: a low-frequency noise/flow
  field should offset the sampling coordinates of the next noise field, so the
  density detail bends through space instead of reading as layered wallpaper.
- It also samples two faster phases at 8x spatial frequency and half the period,
  subtracting them at half amplitude. The result is a cheap layered turbulence
  illusion with continuity across phase handoff.
- `NOISE_SLOPE` gates noise by surface normal/flatness so detail can concentrate
  near shaped gravity terrain instead of filling all space equally.

The important observation: this is a stochastic/exponential renderer and a
procedural flow-field renderer at the same time. It spends the raymarch budget
where the eye needs structure, then lets temporal accumulation hide the missing
samples.

## Tradeoffs

- Quadratic ray distance spacing reaches the horizon but undersamples near/far
  transitions if the density function has uncontrolled high frequencies.
- Blue-noise and Halton offsets trade stable banding for stochastic noise.
- Temporal reprojection makes low sample counts look expensive, but it depends
  on coherent depth, motion, and history clipping.
- The density function is not physically clean: height is displaced by noise
  before sampling patch/floor density. It works because the artistic target is
  flowing volumetric structure, not strict mass conservation.
- The two-phase flow hack is cheap because it derives continuity from domain
  sampling, not a persistent volume simulation.
- The grid/domain transform must remain the source of truth. If the fog domain,
  gravity surface, stardust, and camera-following grid drift apart, the illusion
  tears.
- The fog raymarch is a post effect with `ZWrite Off`, so it cannot contribute
  depth for later transparent sorting. Transparent-looking VFX therefore need to
  join the opaque/depth world by stochastic cutout: write/participate like
  opaque geometry, discard fragments by animated blue-noise coverage, then rely
  on temporal accumulation to reconcile the stipple into transparency.

## Stochastic Transparency

`Aetheria/Dithered Particles` is the particle-side answer to the fog/depth
problem:

- The shader tags itself as `Queue="AlphaTest"` and
  `RenderType="TransparentCutout"`, not normal transparent blending.
- Its fragment shader samples particle texture/color alpha, then calls
  `ditherClip(screenUV + particleOffset, alpha)`.
- `Dither Functions.cginc` compares alpha against `_DitheringTex` plus
  `_FrameNumber * 1.61803398875`, then uses `clip()` to discard uncovered
  fragments.
- `BlueNoiseProvider` sets the global noise texture, screen-relative noise
  scale, randomized offsets, and frame number each update.
- The shader includes a shadow caster pass that applies the same dithered alpha
  test, so these transparent-looking particles can also cast coverage-consistent
  shadows.

This is not just a particle style. It is a renderer contract: volumetric fog,
particles, shields, glow fades, slime, lightning, suns, and similar translucent
surfaces need compatible stochastic coverage if they must interact with depth,
fog, TAA, or shadows. Conventional alpha blending becomes the suspicious option.

## Rebuild Direction

For Aquarium/WebGPU, keep the effect goal and rebuild the machine:

1. Keep one moving world/grid domain shared by gravity, mesh displacement,
   stardust, fog, tint, and cursor wells.
2. Store deferred 2D fields for surface height, patch density/height, tint, and
   flow. Sparse sources splat into these fields; samples should not enumerate
   all sources.
3. Build a WebGPU fog path as explicit passes:
   density/source field update, optional low-res froxel injection, temporal
   raymarch/integration, history reprojection, ACES/HDR composition.
4. Preserve Aetheria's best hack: phase-paired procedural triangle noise advected
   through a flow field, with exponential/quadratic sample distribution and
   stochastic offsets.
5. Improve the old path by making ownership explicit: depth source, grid origin,
   sample distribution, history validity, flow field, and temporal weights should
   be named resources, not hidden shader globals.
6. Degrade by reducing fog resolution, sample count, history confidence, and
   noise octaves. Do not degrade by disconnecting fog from the shared grid
   domain.
7. Treat stochastic cutout transparency as part of the fog architecture. Any
   transparent-looking particle or billboard that must depth-sort with fog should
   use blue-noise coverage in the opaque/depth path, then let TAA/HDR resolve it.

## Source Anchors

- `E:\Projects\Aetheria-Economy\Assets\Scripts\Zone Display\VolumeSampling.cs`
  publishes the global fog/grid domain and environment parameters.
- `E:\Projects\Aetheria-Economy\Assets\Scripts\Zone Display\VolumeCloudRenderer.cs`
  owns downsample targets, history buffers, quality sample counts, Halton jitter,
  reprojection, and composition passes.
- `E:\Projects\Aetheria-Economy\Assets\Shaders\Raymarching\CloudShader.shader`
  implements the raymarch, quadratic sample spacing, Beer-Lambert accumulation,
  blue-noise/Halton offsets, history clipping, and final blend.
- `E:\Projects\Aetheria-Economy\Assets\Shaders\Volumetric.cginc`
  implements density, grid texture sampling, triangle noise, global/slope flow,
  phase-paired scrolling noise, and tint sampling.
- `E:\Projects\Aetheria-Economy\Assets\Shaders\StableFluids\Fluid.cs`
  exposes the moving 2D fluid velocity domain used by volume/stardust paths.
- `E:\Projects\Aetheria-Economy\Assets\Shaders\DitheredParticles.shader`,
  `E:\Projects\Aetheria-Economy\Assets\Shaders\Dither Functions.cginc`, and
  `E:\Projects\Aetheria-Economy\Assets\Scripts\Zone Display\BlueNoiseProvider.cs`
  implement the stochastic cutout transparency path used by particles and related
  depth-sensitive translucent effects.
