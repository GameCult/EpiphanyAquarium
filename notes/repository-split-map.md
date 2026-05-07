# Repository Split Map

The Aquarium repo has been split into four public GameCult repositories. The
goal is clean lineage: each branch of the experiment keeps the history that
actually explains it instead of dragging every pivot behind it forever.

## Repositories

- `GameCult/EpiphanyAquarium-Web`
  - URL: https://github.com/GameCult/EpiphanyAquarium-Web
  - Contains the React, Tauri, WebGL2, DOM-billboard, visual smoke, and original
    aquarium interaction prototype.
  - Preserves the web-era state and design memory.

- `GameCult/EpiphanyAquarium-Bevy`
  - URL: https://github.com/GameCult/EpiphanyAquarium-Bevy
  - Contains the frozen Bevy/Rust prototype: custom raymarch node, Grid-space
    fields, debug UI, CultCache/CultNet integration experiments, and in-tree
    synth copy needed by the prototype.
  - Keeps `notes/` and `state/` because the renderer doctrine and branch
    retrospective are part of why this specimen exists.

- `GameCult/AquariumSynth`
  - URL: https://github.com/GameCult/AquariumSynth
  - Extracted with `git subtree split --prefix=crates/aquarium_synth`.
  - Contains the standalone Rust synth crate, tests, lockfile, and split notes.

- `GameCult/Aquarium-Engine`
  - URL: https://github.com/GameCult/Aquarium-Engine
  - New-history C# engine seed. It does not pretend to have existed in old
    commits; it starts where the pivot starts.

## Original Repo

`GameCult/EpiphanyAquarium` remains the full archive/meta repo unless deliberately
retired later. It records the split and keeps the pre-surgery worktree intact.

Local working copies live directly under `E:\Projects`:

- `E:\Projects\EpiphanyAquarium-Web`
- `E:\Projects\EpiphanyAquarium-Bevy`
- `E:\Projects\AquariumSynth`
- `E:\Projects\Aquarium-Engine`

## Verification

- Created all four repositories as public GameCult repos.
- Pushed each split repository to `main`.
- Verified GitHub reports `PUBLIC` visibility and `main` as default branch.
- Ran `cargo test` in `AquariumSynth`: 41 tests passed.
