# Epiphany Aquarium

Fullscreen aquarium interface for Epiphany.

This is the cute, dynamic operator surface: React, Tauri, WebGL2, fluid
simulation, agent-local menus, and interaction audio. It is deliberately split
from `E:\Projects\EpiphanyAgent` so the UI can develop its own state, memory,
and taste without dragging harness architecture into every visual decision.

## Shape

- Idle state: fluid, agents, glow, motion, sound, vibes.
- Interaction: hover an aquarium object for local option petals.
- Selection: click an object to open its local focus surface.
- Face: the human-facing chat surface; other organs expose internals for
  inspection, not direct chat.
- Swarm: Epiphany instances do not poke each other's workspaces. Coordinators
  ask through visible messages and wait for callbacks.
- Contract: API affordances mirror user-story affordances, so forbidden stories
  are rejected by the backend instead of merely discouraged by the UI.
- Audio: agents sing; non-agent interface controls answer with short
  subtractive resonator hits.
- Backend: Tauri commands call the sibling EpiphanyAgent tools. Override with
  `EPIPHANY_AGENT_ROOT` if needed.

## Run

```powershell
npm install
npm run dev
```

The dev server runs at `http://127.0.0.1:1420/`.

Tauri:

```powershell
npm run tauri dev
```

## Verify

```powershell
npm run build
npm run smoke:visual
```

Smoke artifacts land under `.epiphany-aquarium/`.
If `1420` is already occupied, run smoke on another port:

```powershell
$env:EPIPHANY_SMOKE_PORT = "1422"
npm run smoke:visual
```

The smoke checks that:

- the default aquarium is quiet
- agent DOM projection follows the WebGL simulation
- agent-local option petals are real DOM controls
- selecting Eyes opens its local focus surface
- interface controls trigger the subtractive resonator path once
- fluid parameters persist

## Persistent State

This repo has its own pseudo-Epiphany state:

- `AGENTS.md`
- `state/map.yaml`
- `state/memory.json`
- `state/scratch.md`
- `state/evidence.jsonl`
- `notes/aquarium-interface-doctrine.md`

Treat these as the aquarium's taste and continuity surface. Keep them sharp.
