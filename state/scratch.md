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
