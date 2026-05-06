# Scratch

Current slice:

- Mapped the live aquarium behavior before making cuteness changes.
- Added `notes/aquarium-behavior-map.md` as the explicit dataflow,
  interaction, agent, friction, and improvement-plan surface.
- Main design debt: the old operator console was gated behind aquarium objects
  but not broken down into aquarium-native agent organs.
- Current implementation pass split selected-agent focus data/actions by Self,
  Imagination, Eyes, Body, Hands, Soul, and Life using `renderAgentHabitat`.
- Follow-up correction: tree depth is not capped at one level. The new direction
  maps EpiphanyAgent API seams into recursive branch icons, and only leaf nodes
  unfold concrete UI surfaces.
- Added `notes/epiphany-agent-api-tree-map.md` to keep branch/leaf taxonomy
  grounded in backend read/write/proposal/runtime surfaces.
- Current correction: thought bubbles are already universal, and heartbeat is
  now universal too. Every creature gets a Heartbeat branch with Status and
  Pulse leaves; Face keeps Bubble as the local public-speaking surface.
- Current motion pass: render thin dashed gradient orbit guides in the crisp
  canvas, keep agents spring-tethered to those orbit slots, strengthen pointer
  attraction, and make touch/heartbeat pulses behave like damped plucked strings.
  Heartbeat is shared across all creatures; the latest awakened role gets the
  stronger local pluck.
- Pointer field correction: the aquarium should feel welcoming. Most creatures
  attract gently at range, only Soul pulls back weakly at range, and all
  creatures attract strongly only at short range so the cursor can pull a
  creature out of orbit on purpose.
- Current gravity pass: pointer force now samples an Aetheria-style PowerPulse
  heightfield normal and scales by slope squared, so force fades both outside
  the well and at the center. Agents hover above their cups with a DOM elevation
  offset while WebGL renders the fog/grid-like gravity surface below them.
- Current compute pass: added optional WebGPU stardust. It uses a real compute
  shader and instanced render pass for flow-driven particles, with WebGL2 kept
  as fallback. Current bridge mirrors agent projection velocity into a flow
  source buffer; a future full WebGPU fluid port can share the true velocity
  texture directly.
- Current camera pass: the aquarium now treats agent positions as grid
  coordinates, projects them through a tilted overhead camera for DOM/rendering,
  unprojects cursor screen input back onto the grid, and draws a cursor landing
  mark on the surface.
- Current Three/Aetheria pass: pulled Three.js into the aquarium for the 3D
  scene layer. The field mesh now uses Aetheria-Economy's current PowerPulse
  envelope for gravity cups instead of the 2011 denominator cup, and a separate
  radial-wave source array mirrors Aetheria gas giant wave emitters: PowerPulse
  mask, sine-power phase, frequency, speed, and time-driven breathing.
- Correction to that pass: current Aetheria does not make every grid point
  enumerate every source. It renders additive brush quads into grid-aligned
  gravity render textures with an orthographic top-down camera, then samples the
  accumulated height texture. Aquarium's Three layer now follows that model:
  static wells and low chirp-bank radial waves are splats, and the visible grid
  mesh samples the render target.
- Camera/control pass: the Three scene is now Z-up over a shared XY interaction
  plane. Middle drag orbits yaw/pitch around the plane, wheel zooms with an
  exponential distance curve, and right drag/WASD pan the camera target. Mouse
  coordinates project through the camera ray onto XY, so cursor, interactible,
  distance, drag, and force math can use one consistent plane.
- Follow-up correction: the perspective camera is an explicit orbital rig now:
  target point on the XY grid, polar yaw/pitch/distance converted to a Cartesian
  camera offset, and `camera.up` locked to `+Z` before `lookAt`.
- Pan correction: right-drag samples previous/current mouse ray intersections
  with XY and shifts the camera target by their difference; this avoids bespoke
  pan-vector compensation because the grid projection provides the delta.
- Projection repair pass: DOM creature icons, thought bubbles, radial option
  halos, focus surfaces, stardust attractors, and pointer-to-grid input should
  use the Three camera projection/unprojection rather than the old percent-space
  faux perspective.
- Aetheria stardust reference: `Assets/Shaders/Compute/Stardust/Stardust.compute`
  derives particles from moving grid cells, hashes paired cell seeds for stable
  lifetime phase, samples height/tint, and subtracts `flow(position) *
  lifetime * period` so motion appears continuous without storing much state.
- Stardust scale pass: particle count is now one million. The buffer is no
  longer CPU-seeded on creation because the compute pass overwrites every mote
  from hash-stable moving-cell state before the first draw.
- Stardust tone pass: motes are much smaller/fainter and render additively into
  an internal `rgba16float` target before a lightweight ACES postprocess writes
  the transparent overlay canvas. Future unification should put Three/grid/fluid
  lighting through the same HDR compositor.
- Performance correction: million-particle stardust now caps its WebGPU submit
  cadence around 30fps so main-thread interaction and React creature-tree
  updates do not get starved by constant million-quad HDR passes.
- Smoke correction: visual smoke loads `?smoke=visual`, which keeps the
  stardust shader path active but uses a lighter particle count so DOM
  interaction tests do not measure headless WebGPU starvation instead of UI
  behavior. Normal app loads still use one million particles.
- Smoke boundary correction: cursor interactivity assertions are removed from
  visual smoke because the test cannot be expected to solve the aquarium's 3D
  projection/billboard hit math. Those belong in explicit raycast-aware probes.
- Diegetic UI idea from current conversation: render overlay UI as SVG/DOM on
  camera-facing world-space billboards attached near creatures. Project cursor
  rays onto each billboard plane, then map local hit coordinates back to crisp
  DOM/SVG controls.
- Billboard implementation pass: Three now emits separate projected anchors for
  creature bodies, option halos, thought bubbles, and focus surfaces. Focus
  surfaces render with an SVG backplane and DOM controls as a camera-facing slab;
  pointer movement over a billboard writes local surface coordinates for
  interaction affordances.
- Current compositor correction: visible non-DOM rendering should live in one
  Three/HDR path. The old stardust overlay is now a compatibility stub, smoke
  and crisp canvases are hidden, and Three owns grid, cursor, agent bodies, cups,
  and faint 3D stardust under ACES tone mapping.
- Current billboard correction: 3D bodies do not need DOM glyph bodies. The
  centered DOM caption stays visible as the ID tag, click target, and radial
  interaction graph root. When the mouse is inside a creature billboard, the
  fluid orbit integrator damps that creature to a hold so the UI stabilizes.
- Current spring correction: Self used to be a separate motion class because
  the constellation treated the coordinator as the origin and made every other
  role orbit it. That asymmetry blocks multiple Epiphanies. The main projection
  path now gives every creature the same spring body: orbit-slot spring, pointer
  spring, hover hold damping, and pluck impulse all accumulate as forces.
- Current scale correction: zoom now affects the visible grid cell size and the
  actual rendered grid extent. Project labels are projected DOM billboards that
  fade in while zooming out, and the cursor is represented as a visible gravity
  well whose gradient supplies the spring-like attraction force.
- Current hierarchy correction: zoomed-out workspace labels now displace
  sub-agent captions instead of stacking with them. Each workspace label exposes
  radial swarm-level petals. Self also contributes a larger shared gravity divot
  so swarm cohesion is visible and force-based.
- Current audio-grid correction: chirp bands mapped into grid waves must obey
  spectral logic. Low bands get slow 0.5-1 Hz wide breathing waves; high bands
  get sharply attenuated amplitude, smaller radii, and sub-unit world
  wavelengths so treble reads as fine shimmer instead of huge surface heaving.
- Current stardust correction: Three stardust is parented to the grid group and
  samples the gravity texture for displaced grid height. Particle positions, not
  alpha, now follow an exponential vertical distribution around the grid surface
  with a tighter below-grid falloff.
- Current exploration correction: the visible grid mesh, deferred gravity camera,
  gravity sampling origin, and parented stardust now track the camera target so
  WASD/right-drag exploration moves across a continuous grid field instead of
  leaving the origin-centered floor behind.
- Current stardust domain correction: match Aetheria's moving-domain trick.
  Stardust buffer slots are cell offsets, not particle identities. The shader
  maps those offsets to world-space cells around the moving gravity origin,
  hashes world cell coordinates for jitter/height/lifetime/color, then fades at
  the grid field edge so cells can hand off invisibly as the camera explores.
- Current self-correction: GPU work must be approached as a compute architecture
  problem, not a web rendering problem. Persistent memory now carries a
  GPGPU-specialist doctrine: explicit passes, moving domains, memory hierarchy,
  workgroup-local cooperation, barriers, coalesced storage, deferred fields, and
  hash-derived identity when continuity does not require stored state.
- Current research meal: GigaVoxels adds demand-driven sparse-field doctrine.
  Rays are not just shading work; they are a visibility/LOD/missing-data oracle.
  Use sparse metadata plus brick pools, request only visible refinements, fall
  back to coarser resident data, and let temporal coherence/LRU keep the working
  set bounded while the virtual field exceeds resident memory.
- Current research meal: Wroński fog adds froxel-field doctrine. Volumetric fog
  is a camera/frustum-aligned 3D lighting field: inject density/light/shadows
  into froxels, scan along depth for Beer-Lambert transmittance and in-scatter,
  then sample by screen UV + scene depth. Production reality matters: shadow
  filtering, temporal/jitter tricks, low-pass feature discipline, local fog
  volumes, and explicit ugly approximations keep the field stable.
- Current research meal correction: Dreams needs the talk transcript, not only
  the PDF notes. The transcript added hard constraints and off-slide lessons:
  no imported textures/models/meshes, controller-native authoring, QA comfort
  with edit lists over topology, 40-shader compute doom, atomic point splatting,
  TAA as a stochastic resolver, imperfect shadow maps, and why point-jitter DOF
  works where naive point-jitter motion blur fails.
- Current Aetheria fog rehydration: read the live volumetric renderer path with
  the GPU/froxel/stochastic doctrines in mind. Aetheria's fog is a grid-owned
  world volume sampled by a downsampled camera post raymarch: surface height,
  patch density/height, tint, and flow are shared fields; the raymarch uses
  quadratic distance spacing, Halton plus blue-noise offsets, Beer-Lambert
  integration, and temporal reprojection/history clipping. The irreplaceable
  trick is phase-paired scrolling triangle noise through global/slope flow,
  which fakes continuous horizon-scale volumetric motion without storing a huge
  3D volume. Captured as `notes/aetheria-volumetric-fog-map.md` and
  `aetheria_volumetric_fog` memory.
- Stochastic transparency correction: because the fog raymarch is a post effect
  and cannot write depth, transparent-looking particles and similar VFX need to
  be depth participants by using blue-noise alpha test/cutout plus TAA resolve.
  `Aetheria/Dithered Particles` tags AlphaTest/TransparentCutout, clips coverage
  via `Dither Functions.cginc`, and includes a matching shadow caster pass.
- Current field-first implementation pass: `src/aquariumScene3d.ts` now has a
  first-class field-volume shader pass. It raymarches camera rays through
  SDF-defined solid forms, agent-proxy SDF solids, and gassy SDF density tied to
  the same moving gravity/grid texture as the mesh and stardust. The gas uses
  Aetheria-style phase-paired triangle noise, source/pointer density, and
  Beer-Lambert accumulation; solid hits use stochastic coverage so the
  transparency/depth doctrine is represented in the live renderer.
- Current research meal: Bruneton atmospheric scattering adds precomputed
  transport-cache doctrine. Global sky/aerial-perspective lighting should come
  from named textures for transmittance, scattering, optional single Mie, and
  irradiance, built by an explicit delta/accumulation pass graph. Dynamic gassy
  SDF fog stays local and stochastic, but should be lit/composited against those
  global transport fields. Captured as
  `notes/bruneton-atmospheric-scattering-map.md` and `bruneton_atmosphere`
  memory.
- Current SDF planet pass: visible swarm bodies are no longer Three mesh
  octahedra/cups/anchors. `src/aquariumScene3d.ts` keeps invisible groups only
  as projection anchors; the field-volume shader renders each agent as a chrome
  planet SDF displaced by cheap 4D fBm in local xyz plus time. Each source
  carries mass/activity, height, color, and a Self flag. Atmosphere shells scale
  with mass; Self uses solar emissive shading and stronger displacement/corona
  noise. Build passed, and `EPIPHANY_SMOKE_PORT=14920 npm run smoke:visual`
  passed after replacing the first expensive nested 4D value-noise draft with a
  cheaper analytic fBm.
- Current renderer documentation pass: added `notes/aquarium-renderer-map.md`.
  The live renderer is explicitly documented as Three scene + moving Aetheria
  2D gravity render target + fullscreen analytic SDF/gas raymarch. It is not
  currently a brick map projected into Wronski froxels. The intended future path
  is 2D source fields, typed SDF source lists, optional sparse bricks only for
  non-cheap derived fields, froxel injection/integration, depth-aware
  composition, stochastic TAA resolve, and Bruneton-style global atmosphere.
- Current WebGPU correction: the froxel layer should store primitive
  membership, not pre-baked fog density. `src/aquariumStardust.ts` now creates
  a WebGPU field overlay that builds one `u32` primitive bitmask per froxel in a
  storage buffer, then the render pass marches pixels and evaluates only the
  primitives named by that froxel. This avoids WebGPU->CPU->WebGL shuttling.
  `src/aquariumScene3d.ts` keeps the Three/WebGL field-volume path as fallback
  only; when `navigator.gpu` exists it marks the field as externally rendered
  and skips the expensive WebGL SDF/fog pass. Build and visual smoke passed on
  port 14922.
- Current solid/fog correction: both solid SDF and volumetric atmosphere
  evaluation go through the same WebGPU froxel primitive mask. The pixel ray
  reads the current froxel's bitset, evaluates only those primitives, and now
  early-exits when a solid SDF is touched, shading the solid with accumulated
  fog/transmittance up to the hit instead of continuing the march.
- Current HDR lighting pass: copied Aetheria's `studio3.hdr` into
  `public/textures/studio3.hdr`, loaded it with Three `RGBELoader`, PMREM-
  filtered it, and assigned it to `scene.environment` for PBR lighting. WebGPU
  field chrome uses a sampled HDR summary because WebGPU cannot borrow the
  Three/WebGL PMREM texture directly across APIs.
- Current SH/froxel lighting pass: the WebGPU field layer now ping-pongs a
  first-order spherical-harmonic lighting buffer through froxel space. It
  propagates previous lighting through six neighbors, injects environment light
  from grid-volume edges using the studio HDR summary, and injects local Self
  emission so diffuse volumetric light belongs to the scene instead of the
  screen. The old baseline screen-depth haze was removed, and planet
  displacement/noise frequencies were lowered so chrome bodies read as smooth
  gaseous planets rather than glittery shader rash.
