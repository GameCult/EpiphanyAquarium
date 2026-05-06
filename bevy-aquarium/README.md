# Epiphany Aquarium Bevy Host

This is the new native renderer host for Epiphany Aquarium. The React/Tauri app
still exists while the migration is staged, but the renderer target is Bevy:
world-space bodies, springs, heightfields, froxels, compute, and diegetic UI
anchors belong here.

Run it from the repo root:

```powershell
npm run bevy
```

Or directly:

```powershell
cargo run --manifest-path bevy-aquarium/Cargo.toml
```

Controls:

- Middle mouse drag: orbit camera.
- Right mouse drag: pan across the grid.
- Mouse wheel: exponential zoom.
- WASD: pan along the camera-projected grid basis.

Current pass:

- Epiphanies are world-space celestial bodies, not screen-space UI tokens.
- Sleeping Epiphanies render as cold white dwarfs.
- A living Epiphany expands into a small agent swarm.
- All bodies use the same spring integration path.
- The grid is a real mesh heightfield generated from gravity wells.

Next renderer pass:

- Replace mesh displacement with a Bevy render graph path that writes a gravity
  field texture.
- Add WebGPU compute froxel bins for solid and gaseous SDF primitives.
- March the shared density accumulator and write depth when density saturates.
- Add volumetric lighting caches before making the fog pretty. Pretty fog that
  lies is just expensive weather.
