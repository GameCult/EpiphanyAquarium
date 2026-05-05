# Epiphany Aquarium Instructions

## Purpose

Epiphany Aquarium is the interface organism for Epiphany: a fullscreen React,
Tauri, WebGL2, fluid-simulation control surface where agents, memories,
controls, and evidence appear as living objects in an aquarium.

This repo is not the Epiphany harness. The harness backend remains
`E:\Projects\EpiphanyAgent`; this repo owns the visual, tactile, sonic, and
interaction grammar.

## Operating Doctrine

- The default screen should be vibes, not paperwork. No always-on admin chrome
  unless the user explicitly asks for a utilitarian debug mode.
- Every interface surface must be gated behind an object in the aquarium:
  hover, touch, select, drag, dwell, orbit, bloom, unfold.
- If a panel is just sitting on screen, it is guilty until proven alive.
- Cute is not decoration. Cute is affordance, memory, emotional salience, and
  spatial recall wearing a better coat.
- Motion must communicate state. Calm objects drift. Busy objects pulse.
  Blocked objects tense. Panicked objects can shake, but panic is an event, not
  the default idle loop.
- Sound belongs to interaction, not noise spam. Agents sing; controls answer
  with subtractive resonator hits; mouse down gets one clear response.
- WebGL and DOM must agree. Do not paint invisible UI into fluid buffers unless
  the DOM or canvas layer visibly owns the same object.
- Keep the machine testable. Visual smoke should encode the interaction rule,
  not only check that pixels exist.
- API contracts must mirror user-story contracts. If the story is "ask another
  coordinator politely", the backend must expose that lane and reject
  cross-workspace rummaging. Pretty affordances without authority are stage
  dressing with a badge.
- Users may inspect Epiphany internals aggressively: state, artifacts, messages,
  role status, graphs, heartbeats, and evidence should surface in the aquarium.
  But humans talk to Face. Sub-agents can talk soul-to-soul through coordinator
  channels; they are not each a separate chat counter for the human to queue at.
- One Epiphany instance must not inspect or edit another instance's workspace.
  Cross-agent needs go through swarm coordinator messages and callbacks.

## Persistent State

- `state/map.yaml` is the canonical project map.
- `state/memory.json` is the persistent taste, doctrine, and implementation
  memory for the aquarium persona.
- `state/scratch.md` is disposable working context for the current slice.
- `state/evidence.jsonl` stores distilled lessons that should change future
  behavior.
- `notes/aquarium-interface-doctrine.md` is the live design doctrine.

Update these when the interface learns something durable. Delete stale guidance
instead of building a museum of old cravings.

## Backend Boundary

- Run GUI work from `E:\Projects\EpiphanyAquarium`.
- The Tauri backend calls Epiphany harness scripts in `E:\Projects\EpiphanyAgent`.
- Override the backend root with `EPIPHANY_AGENT_ROOT` when needed.
- Aquarium smoke artifacts live under `.epiphany-aquarium`.
- Harness action/runtime/rider artifacts still live under the backend repo's
  `.epiphany-gui` directories until the backend contract is renamed.

## Verification

Use focused checks:

```powershell
npm run build
npm run smoke:visual
```

The visual smoke must keep proving:

- the initial aquarium is quiet
- agent projections are synchronized
- agent-local options are real DOM controls
- selecting an agent opens the local focus surface
- non-agent interface controls trigger the subtractive resonator once
- fluid parameters persist
