# Aquarium Interface Doctrine

The aquarium is not a dashboard with an animated background. It is the primary
interface world.

## First Principles

- **Vibes first.** The idle state should be fluid, agents, glow, motion, sound,
  and implication. The user should not be greeted by an office supply catalog.
- **Objects own surfaces.** Data appears because a living object offers it.
  Controls appear because the object can act.
- **DOM is still real.** Cute controls must remain accessible buttons, inputs,
  details, lists, and regions where that matters. Whimsy does not get to fake
  hit targets in a canvas and call the crime poetry.
- **WebGL is atmosphere and embodiment.** Fluid, bloom, wakes, dye, and
  particle-like residue should communicate state, not hide the interface.
- **Sound is acknowledgement.** Agent voices may form a soft choir. Interface
  elements should answer with brief resonant, subtractive hits.

## Distilled Interaction Reading

This is a high-level synthesis of common lessons from interaction design,
animation, CSS architecture, game feel, WebGL, GPU simulation, and audio UI
practice:

- Users build spatial memory faster when controls live near the thing they
  affect.
- Progressive disclosure works best when the reveal motion explains where the
  surface came from and how to dismiss it.
- Animation should preserve object identity. If an object moves, scales, or
  blooms, its hit target and semantic role should remain coherent.
- GPU simulation is good at continuity, residue, and field behavior; DOM is good
  at crisp text, focus, semantics, and forms.
- Broad-spectrum audio becomes pleasant when filtered through harmonic structure
  and short envelopes. Constant unshaped noise is fatigue wearing headphones.
- The most delightful interfaces use restraint. Calm makes excitement legible.

## Hard Rules

- No always-on control panels in fullscreen mode unless a debug mode explicitly
  requests them.
- No source-only fluid UI emitters without a visible owner.
- No audio double-fires on mouse down plus click/up.
- No text surfaces that cannot fit their container.
- No invented layout cleverness that visual smoke cannot exercise.

