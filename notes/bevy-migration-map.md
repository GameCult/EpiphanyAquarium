# Bevy Migration Map

## Direction

Epiphany Aquarium is moving from a React/Tauri/WebGPU renderer experiment into a
Bevy-native client. The web app remains a staging shell until the Bevy host can
own the scene, agent communication, persistent state, and renderer pipeline.

## Runtime Split

- Bevy owns the living scene: camera, world-space bodies, springs, heightfield,
  froxels, SDF marching, volumetrics, and diegetic UI anchors.
- `cultnet-rs` owns wire communication with Epiphany agents and other Rust
  runtimes.
- `cultcache-rs` owns aquarium settings, local persistent state, and replicated
  document-shaped state.
- The existing Tauri backend remains a compatibility bridge while agent runtime
  communication moves to CultNet.

## Current Bevy Host

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
```

`bevy:dev` enables Bevy dynamic linking and asset watching. `bevy:watch` is a
local restart loop for code and shader changes. True system hotpatching is
available behind the crate's `hotpatch` feature, but it pulls in Bevy's heavier
Dioxus/subsecond stack; until that is worth the extra moving parts,
restart-on-save is the reliable default.

## Next Renderer Steps

1. Replace CPU mesh heightfield regeneration with a Bevy render-graph gravity
   texture pass.
2. Add WebGPU compute binning for solid and gaseous SDF primitives.
3. Store primitive membership in froxels, but sample fields continuously inside
   the raymarch so froxels skip empty space without sculpting the surface.
4. March solid and gaseous fields through one density accumulator; write depth
   once density saturates.
5. Add a froxel lighting cache using propagated spherical harmonics, with
   environment light injected from volume boundaries.
6. Move diegetic UI anchors into Bevy world entities, then decide whether the
   interactive surface is native egui, a webview overlay, or a smaller custom UI
   layer.

## Invariant

The renderer must be world-space first. No screen-space planet sizing, fake
background glow, or UI panel pretending to be a creature. The aquarium is not a
spreadsheet wearing a diving helmet.
