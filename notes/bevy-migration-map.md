# Bevy Prototype Map

## Direction

Epiphany Aquarium moved from a React/Tauri/WebGPU renderer experiment into a
Bevy-native client prototype, then stopped there. Bevy is now reference
material, not the target host.

The next target is a C# Aquarium-owned engine core, likely using Stride as
scaffolding and parts donor where that preserves momentum without handing over
the renderer's taste or frame ownership.

## Prototype Runtime Split

- Bevy owns the prototype scene: camera, world-space bodies, springs,
  heightfield source fields, SDF marching, HDR output, bloom/tonemapping, and
  a small native debug UI.
- `cultnet-rs` owns wire communication with Epiphany agents and other Rust
  runtimes.
- `cultcache-rs` owns aquarium settings, local persistent state, and replicated
  document-shaped state.
- The existing Tauri backend remains a compatibility bridge while agent runtime
  communication moves to CultNet.

## Current Bevy Prototype

Path: `bevy-aquarium`

Implemented:

- Bevy 0.18 binary crate inside this repository.
- World-space celestial bodies for a living Epiphany swarm and sleeping
  Epiphany projects.
- Shared spring integration for Self, agents, and sleeping project bodies.
- Orbit, cursor attraction, camera orbit, camera pan, and exponential zoom.
- A generated gravity heightfield mesh using the same PowerPulse-shaped well
  language the web prototype used.
- `cultcache-rs` settings persistence at
  `.epiphany-aquarium/bevy-client-settings.msgpack`.
- Typed CultCache document entries:
  - `epiphany.aquarium.client-settings`
  - `epiphany.aquarium.agent-presence`
- `cultnet-rs` hello-message construction advertising the Bevy client and its
  supported document types.
- A custom HDR raymarch path with Grid height/source-field compute, light brick
  occupancy, Grid-space irradiance propagation, solid body/terrain marching,
  debug modes, gentle Bloom, and ACES tonemapping.
- A native Bevy debug UI with a top-left `>_` affordance, tab rail, left-half
  panel, and registered-command terminal.
- `crates/aquarium_synth`, a Rust procedural synth/reference module that became
  stable enough to preserve as algorithmic salvage.

Run:

```powershell
npm run bevy
```

Check:

```powershell
npm run bevy:check
```

Fast iteration:

```powershell
npm run bevy:dev
npm run bevy:watch
npm run bevy:hot
```

`bevy:hot` is the actual Rust-code hotpatch path. It runs Bevy with the
`hotpatch` feature through the Dioxus CLI, using Subsecond under the hood.
`bevy:watch` is only a fallback restart loop for changes that cannot be patched.
Calling restart-on-save "hot reload" was nonsense with shoes on.

## Frozen Renderer Lessons

- Solid-only bring-up was the correct simplification.
- Purging Bevy deferred/prepass integration was correct.
- Stable body seeds are mandatory for translation-invariant SDF detail.
- Broad-phase bounds must stay conservative and separate from visible SDF
  shading.
- Renderer debug modes are not garnish; they are how black frames become facts.
- The engine host must not own the lighting story.

See `notes/rust-branch-retrospective.md` for the full salvage/rejection ledger.

## Invariant

The renderer must be world-space first. No screen-space planet sizing, fake
background glow, or UI panel pretending to be a creature. The aquarium is not a
spreadsheet wearing a diving helmet.
