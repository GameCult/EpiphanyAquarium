# Epiphany Aquarium Bevy Host

This is the new native renderer host for Epiphany Aquarium. The React/Tauri app
still exists while the migration is staged, but the renderer target is Bevy:
world-space bodies, springs, heightfields, froxels, compute, and diegetic UI
anchors belong here.

Run it from the repo root:

```powershell
npm run bevy
```

Fast iteration:

```powershell
.\run-bevy-hot.ps1
npm run bevy:dev
npm run bevy:watch
npm run bevy:hot
```

From the repo root, `.\run-bevy-hot.ps1` installs the local prebuilt Dioxus CLI
if needed and starts the Bevy hotpatch loop.

`bevy:hot` is the real Rust-code hotpatch lane. It uses Bevy's
`hotpatching` feature through the Dioxus CLI:

```powershell
npm run bevy:hot:install
npm run bevy:hot
```

`bevy:dev` enables Bevy dynamic linking and forces asset watching on.
`bevy:watch` is only the fallback restart-on-save loop for changes that cannot
be patched into the running process. It is useful, but it is not hot reload.

Or directly:

```powershell
cargo run --manifest-path bevy-aquarium/Cargo.toml
```

Controls:

- Middle mouse drag: orbit camera.
- Right mouse drag: pan across the grid.
- Mouse wheel: exponential zoom.
- WASD: pan along the camera-projected grid basis.
- F3: cycle the CultCache-backed renderer debug mode.
- F5: rehydrate the Bevy domain from CultCache.

Current pass:

- Epiphanies are world-space celestial bodies, not screen-space UI tokens.
- Sleeping Epiphanies render as cold white dwarfs.
- A living Epiphany expands into a small agent swarm.
- All bodies use the same spring integration path.
- The visible grid and bodies are raymarched through the Bevy/WGPU renderer;
  ECS entities carry simulation, cache, label, audio, and renderer-source state.
- Grid height is now an explicit GPU source field generated before brick
  occupancy, SH lighting, and the deferred raymarch sample it.
- The density-history experiment is removed; the current renderer is back to
  making the sun and planets read before field complexity returns.
- The camera renders through an HDR intermediate with `Tonemapping::AcesFitted`
  and thresholded bloom, so solar emissive values can glow without flattening
  the whole scene.

Next renderer pass:

- Move Grid height from a storage-buffer field to a packed source texture once
  density, tint, and flow join it.
- Improve raymarched body material response, solar surface motion, and planet
  readability before adding more field buffers.
- Tune bloom exposure/material values against the actual running view, not just
  compile-time righteousness.
