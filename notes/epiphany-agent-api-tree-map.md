# Epiphany Agent API Tree Map

Last mapped: 2026-05-05

This map grounds the aquarium interaction tree in `E:\Projects\EpiphanyAgent`
instead of in the old operator-console layout. Branches are cute icons. Leaves
are specific seams.

## Surface Families

### Read Lenses

These are reflection surfaces. They do not mutate durable Epiphany state.

- `thread/read`: hydrated thread, including `Thread.epiphanyState`.
- `thread/epiphany/scene`: compact scene, state revision, checkpoint summary,
  available actions.
- `thread/epiphany/planning`: captures, backlog, roadmap streams, Objective
  Drafts, active-objective separation.
- `thread/epiphany/context`: bounded state shard for graph nodes/edges,
  frontier, checkpoint, observations, and evidence.
- `thread/epiphany/graphQuery`: bounded graph traversal and code-ref matching.
- `thread/epiphany/jobs`: launcher-bound jobs plus live backend overlay.
- `thread/epiphany/freshness`: retrieval, graph, watcher, and churn freshness.
- `thread/epiphany/pressure`: context-pressure gauge.
- `thread/epiphany/reorient`: read-only resume/regather verdict.
- `thread/epiphany/crrc`: read-only coordinator recommendation.

### Proposal Lenses

These draft or evaluate possible state changes but should not directly become
operator controls without a clear review step.

- `thread/epiphany/retrieve`
- `thread/epiphany/distill`
- `thread/epiphany/propose`
- `thread/epiphany/promote`

### Runtime Loops

These are explicit launch/read/accept loops. The aquarium should expose each
step as a separate leaf, not as one generic action pile.

- Role workers:
  - `thread/epiphany/roleLaunch`
  - `thread/epiphany/roleResult`
  - `thread/epiphany/roleAccept`
- Reorientation worker:
  - `thread/epiphany/reorientLaunch`
  - `thread/epiphany/reorientResult`
  - `thread/epiphany/reorientAccept`
- Launcher jobs:
  - `thread/epiphany/jobLaunch`
  - `thread/epiphany/jobInterrupt`
  - `thread/epiphany/jobsUpdated`

### State Writes

These are red-pen surfaces.

- `thread/epiphany/update`
- accepted `thread/epiphany/promote`
- `thread/epiphany/jobLaunch`
- `thread/epiphany/jobInterrupt`
- `thread/epiphany/reorientLaunch`
- `thread/epiphany/reorientAccept`
- normal rollout persistence of `EpiphanyThreadState`

### GUI Wrapper Actions

`tools/epiphany_gui_action.py` currently wraps the backend seams into aquarium
actions:

- `prepareCheckpoint`
- `launchImagination`, `readImaginationResult`, `acceptImagination`
- `launchModeling`, `readModelingResult`, `acceptModeling`
- `launchVerification`, `readVerificationResult`, `acceptVerification`
- `launchReorient`, `readReorientResult`, `acceptReorient`
- `adoptObjectiveDraft`
- `continueImplementation`

## Aquarium Tree Assignment

### Shared Heartbeat

Every creature gets a Heartbeat branch:

- Status -> shared `thread/epiphany` heartbeat status read, scoped in the UI to
  the selected creature and the latest awakened role.
- Pulse -> shared `runHeartbeat` / `heartbeatStatus` action pair.

This is shared aquarium life, not eight fake backend clocks. Face remains the
public mouth for bubble artifacts, but heartbeat is no longer Face-private.

### Self

- Read
  - CRRC -> `thread/epiphany/crrc`
  - Thread -> `thread/read`, `thread/epiphany/scene`
- Write
  - Checkpoint -> `prepareCheckpoint` / `thread/epiphany/update`
  - Run -> status/coordinator wrapper actions
- Sound
  - Harmony -> local MIDI harmony source, not backend state

### Imagination

- Heartbeat
  - Status -> shared heartbeat read
  - Pulse -> shared heartbeat action pair
- Planning
  - Drafts -> `thread/epiphany/planning` Objective Drafts and
    `adoptObjectiveDraft`
  - Backlog -> `thread/epiphany/planning` backlog items
  - Captures -> `thread/epiphany/planning` captures
- Worker loop
  - Launch -> `launchImagination`
  - Read -> `readImaginationResult`
  - Accept -> `acceptImagination`

### Eyes

- Heartbeat
  - Status -> shared heartbeat read
  - Pulse -> shared heartbeat action pair
- Evidence
  - Graph Query -> `thread/epiphany/graphQuery`, current rendered graph summary
  - Artifacts -> GUI action artifact manifests
- Future useful leaves:
  - Context Shard -> `thread/epiphany/context`
  - Freshness -> `thread/epiphany/freshness`

### Body

- Heartbeat
  - Status -> shared heartbeat read
  - Pulse -> shared heartbeat action pair
- Structure
  - Graph -> typed architecture/dataflow graph projection
  - Modeling Result -> `readModelingResult`
- Worker loop
  - Launch -> `launchModeling`
  - Accept -> `acceptModeling`

### Hands

- Heartbeat
  - Status -> shared heartbeat read
  - Pulse -> shared heartbeat action pair
- Workspace
  - Diff -> Rider/git workspace projection
  - Artifact -> implementation audit artifact
- Continue
  - Run -> `continueImplementation`
  - Source -> `inspectRider`

### Soul

- Heartbeat
  - Status -> shared heartbeat read
  - Pulse -> shared heartbeat action pair
- Risk
  - Findings -> verification result projection
  - Runtime -> Unity/Rider bridge projections
  - Review -> launch/read/accept verification loop

### Life

- Heartbeat
  - Status -> shared heartbeat read
  - Pulse -> shared heartbeat action pair
- Continuity
  - Pressure -> `thread/epiphany/pressure`
  - Verdict -> `thread/epiphany/reorient`
  - Worker -> launch/read/accept reorientation loop

## Design Rule

Branch nodes must remain small, iconic, and mostly wordless. A branch exists to
answer "which seam family?" Leaf nodes are where text, tables, forms, and action
buttons are allowed to unfold.
