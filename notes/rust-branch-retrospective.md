# Rust Branch Retrospective

Date: 2026-05-07

This branch moved the aquarium from the web prototype into a Rust/Bevy host,
then proved that Bevy/Rust is not the right primary substrate for the next
Aquarium engine. That is not failure. It is the branch doing its job: reveal
the shape of the machine before the wrong host becomes a religion.

## Decision

Freeze the Bevy host as a prototype/reference branch. Do not deepen it as the
primary renderer.

The next target is C# land: a Stride-adjacent or Stride-scaffolded environment
where Aquarium owns the frame, render graph, field pipeline, debug UI, state
model, and taste. Stride can provide momentum where it helps: windowing, device
setup, input, assets, shader compilation, editor affordances, and other pieces
that do not get to decide what the aquarium is.

The strategic direction is not "port to Stride and obey Stride." It is:

```text
Use Stride as scaffolding and parts donor.
Build the Aquarium engine core in C#.
Own the frame.
```

## Why Rust/Bevy Stopped Fitting

- Bevy gave us a working host, but the code did not feel like an environment
  the user could keep shaped to their standards.
- Deep renderer work spent too much energy negotiating with Bevy's render graph,
  prepass, UI, and hotpatch assumptions.
- Rust was good for correctness pressure, but not for this project's desired
  authorial feel. The user wants to read and reshape the machine directly.
- The Bevy renderer repeatedly became an engine-integration argument before it
  became a visual instrument.
- Raw Bevy UI is too primitive for the desired CultUI-style ergonomics without
  building a substantial layer first.
- Windows Rust linking/hotpatching added enough operational grit to slow the
  loop at exactly the wrong layer.

The blunt lesson: a renderer this opinionated needs a host that feels like home.

## What Survived

### Renderer Doctrine

These should carry forward unchanged:

- Aquarium owns its pixels. The engine may host the device and swapchain; it
  must not own the lighting story.
- No default global/ambient/fullbright light. Illumination is diegetic.
- Self is the initial emitter, but surfaces should sample a field, not a manual
  special-case light term.
- The Grid center is the camera target projected to the XY plane.
- Grid radius follows zoom, not camera angle.
- Orbit anchors remain world-space.
- Planet surface domains must be translation-invariant:
  `(point - body_center) / radius` plus stable identity seed.
- Broad phase is only culling. Visible shading normals come from the same SDF
  that produced the hit.
- Field/render resources should be explicit: source fields, lighting fields,
  history buffers, stardust worker slots, and final HDR compose.
- Bloom spreads HDR energy before ACES tonemapping.
- Debug views are part of the renderer, not optional garnish.

### Bevy Renderer Lessons

The useful pieces:

- Custom render graph node owning compute and fullscreen raymarch ordering.
- Ping-pong storage buffers for Grid-space irradiance/SH propagation.
- Grid-source buffer storing height/slope/edge coverage.
- Solid-only raymarch bring-up was the right simplification.
- Debug modes for final, hit coverage, depth, normals, brick occupancy, and
  irradiance luminance were valuable.
- Purging Bevy deferred was correct. The G-buffer path fought the renderer.
- Stable body seeds fixed surface-detail swimming.
- Fast analytical-gradient value noise was a good body-displacement basis.

The parts to leave behind:

- Bevy deferred/prepass integration.
- Trying to make Bevy UI the long-term compositional UI layer directly.
- Treating Bevy hotpatch as the main iteration story on Windows.
- Generic engine lighting as a compatibility goal.

### UI Lessons

The debug UI should survive conceptually, not mechanically:

- A tiny debug affordance can open a larger left-half panel.
- The terminal must be a registered-command DSL by default, not a shell.
- Focus ownership matters. Terminal input must not also pan the camera.
- A CultUI-style code-first API is worth building, but the Bevy implementation
  should become reference material only.

The C# version should preserve the ergonomic shape of `PropertiesPanel` and
CultUI:

```csharp
panel.AddField("Shutdown Threshold", read, write, 0, 1);
ui.Slider("Bloom", binding, 0, 1);
ui.Enum("Renderer Mode", binding);
ui.Command("Reload Shaders", DebugCommand.ReloadShaders);
```

But it should not rely on Unity-style prefabs. In the new engine, widgets are
code-first primitives with stable IDs, typed bindings, command routing, and
theme tokens.

### CultNet / CultCache

The Rust branch clarified the boundary:

- CultNet is the communication grammar.
- CultCache is the persistence grammar.
- Client settings, renderer modes, body state, active members, and debug knobs
  should be serializable state, not process vapor.

The implementation language can change. The contract remains.

## Salvage: `aquarium_synth`

`crates/aquarium_synth` is real work and should not be thrown away.

It currently contains:

- Serializably described modular patch data.
- Oscillators, envelopes, pitch ramps, filters, phaser/repeat/arpeggio style
  primitives.
- Sfxr compatibility mapping and seeded mutation.
- A line-oriented patch script with terse primitive aliases.
- Defaults/templates/borrowing in the script language:
  `d`, `def`, `u=`.
- Patch-level modulation lanes and oscillator-to-target routing.
- FM bell/chime/gong primitives.
- 808-style mini-kit scripts.
- Wobble/formant/FM bass scripts.
- Audio analysis:
  duration, RMS, peak, zero-crossing, spectral centroid/rolloff, envelope,
  log-mel spectrogram, comparison score.
- Reference tests against the original `sfxr` crate.
- A release voice-capacity probe.

Observed local release performance:

- Roughly 1900-2200 simple pluck voices at realtime parity.
- Roughly 450-520 colored/formant voices.
- Roughly 320-360 wobble/FM/formant voices.
- Roughly 180-215 deliberately maximal per-voice graphs.
- Roughly 240-265 maximal shared-bus graphs.

What to do with it:

- Treat it as a stable algorithm/reference implementation.
- Port the patch model and analyzer to C# when the audio layer becomes active.
- Keep the Rust crate around as an oracle for generated audio and performance
  comparisons until the C# version earns trust.
- Preserve script syntax unless there is a deliberate reason to change it.

Do not bury it because the host changed. That would be waste with a mustache.

## Salvage: Graph Layout Work

The graph/layout work was also valuable, even if it is not the renderer target.

Carry forward:

- Architecture graphs are first-class debug/data surfaces.
- Graph layouts should be generated into explicit data, not hand-positioned UI.
- Layout output should become inspectable, serializable state.
- Aquarium UI should be able to show graph summaries, code refs, evidence refs,
  and object relationships without dumping users into generic panels.
- The local interaction graph doctrine remains: branch nodes are icons, leaves
  are concrete surfaces.

Likely C# direction:

- Keep graph model and layout as data-only modules.
- Use ELK-like or force/layout algorithms as offline or background jobs.
- Render graph projections through the Aquarium UI layer, not through a web DOM
  dependency.
- Persist useful graph layout state through CultCache.

## What To Document As Rejected

Rejected for primary future:

- React/Tauri/WebGL as the main host.
- Bevy/Rust as the main host.
- Engine-owned deferred lighting as the main renderer.
- Default shell-backed debug terminal.
- Global ambient/fullbright readability hacks.

Not rejected:

- Rust modules as algorithm references.
- Bevy prototype as renderer evidence.
- WGPU/WebGPU/GPU-compute doctrine.
- CultNet and CultCache.
- Aetheria-inspired Grid field rendering.
- CultUI/PropertiesPanel ergonomic lessons.

## Migration Shape

The next engine should start small:

```text
AquariumHost
  platform/window/device/input from Stride or a narrow C# host
AquariumRuntime
  CultCache/CultNet/state/debug commands
AquariumRenderGraph
  explicit passes and resource ownership
AquariumUi
  code-first widgets, bindings, commands, debug panels
AquariumAudio
  ported synth model, queued procedural voices
```

First target:

1. C# app window.
2. Camera rig and Grid invariant.
3. Sun and planets.
4. ACES/bloom.
5. Debug UI with registered command DSL.
6. CultCache-backed settings.
7. Then renderer source fields and raymarching.

Keep the machine small until it visibly deserves more organs.
