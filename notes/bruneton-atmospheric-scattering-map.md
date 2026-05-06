# Bruneton Atmospheric Scattering Map

## Core Shape

Bruneton's atmospheric scattering implementation is a precomputed transport
pipeline. It does not raymarch the full sky lighting problem every frame. It
precomputes reusable lookup textures for a spherical atmosphere, then runtime
shaders query those textures for sky radiance, aerial perspective, and surface
irradiance.

The live model separates:

- `transmittance_texture`: 2D lookup over altitude/radius and view zenith.
- `scattering_texture`: packed 4D lookup stored as a 3D texture over radius,
  view zenith, sun zenith, and view-sun angle.
- optional `single_mie_scattering_texture`: stores the full Mie component
  instead of reconstructing it from one packed channel.
- `irradiance_texture`: 2D lookup over altitude/radius and sun zenith for
  ground/surface irradiance.
- temporary delta textures for one scattering order at a time.

This is not just a sky shader. It is a texture-backed light-transport cache.

## Precompute Order

The precomputation is an explicit pass graph:

1. Compute transmittance to the top atmosphere boundary.
2. Compute direct irradiance into a temporary irradiance texture.
3. Compute first-order Rayleigh and Mie single scattering.
4. For scattering order 2 through N:
   - compute scattering density from previous scattering/irradiance terms.
   - compute indirect irradiance and accumulate it.
   - compute multiple scattering and accumulate it.

The important engine lesson is the ownership of deltas. Temporary textures hold
the current order; final textures accumulate usable totals. That keeps the math
and resource lifecycle legible.

## Parameterization

Bruneton reduces atmospheric light transport to lookup coordinates:

- Transmittance depends on radius `r` and view zenith cosine `mu`.
- Scattering depends on radius `r`, view zenith `mu`, sun zenith `mu_s`, and
  view-sun angle `nu`.
- Irradiance depends on radius `r` and sun zenith `mu_s`.

The newer implementation removes Earth-specific magic constants from coordinate
mapping and uses parameterizations intended to work for other planets. This is
the part to steal for Aquarium: field caches should encode their domain
coordinates honestly, not hide scale tricks in shader literals.

## Physics Contract

The atmosphere parameters are explicit:

- solar irradiance and sun angular radius.
- bottom and top atmosphere radii.
- Rayleigh density profile and wavelength-dependent scattering.
- Mie density profile, scattering, extinction, and phase asymmetry.
- absorption density/extinction, including ozone in the newer implementation.
- ground albedo.
- minimum sun elevation/cosine for precomputed precision.

Bruneton also keeps radiometric and photometric quantities distinct. The GLSL
uses macros because GLSL lacks units; the same functions compile as C++ with
dimension types so unit mistakes can fail at compile time. This is unusually
sane. Horrifyingly sane, even.

## Runtime Contract

Runtime shaders do not re-solve the atmosphere. They ask:

- `GetSkyRadiance`: sky along a view ray, plus transmittance.
- `GetSkyRadianceToPoint`: aerial perspective between camera and a point.
- `GetSunAndSkyIrradiance`: direct sun and sky irradiance for surfaces.

This gives a clean split for our renderer:

- far atmosphere and global sky color come from precomputed transport caches.
- local gassy SDF fog still raymarches or froxel-integrates.
- solid/gassy SDF surfaces use precomputed sky/sun irradiance as one lighting
  source, then local agent/cursor/emissive fog sources add on top.

## Color Pipeline

Bruneton supports two RGB paths:

- Precompute spectral radiance at RGB wavelengths, then convert to luminance at
  runtime with approximate constants.
- Precompute luminance by integrating more wavelengths with CIE color matching
  functions, slower but better for spectral variation.

For Aquarium, keep the first build pragmatic: use RGB/luminance caches and HDR
composition. If we later make Epiphany skies emotionally/spectrally important,
move toward multi-wavelength precompute. Do not pretend three arbitrary colors
are physically neutral if they are steering mood and legibility.

## Lessons For Aquarium

- Atmosphere/fog lighting should have precomputed or cached transport fields.
  The local fog raymarch should not shoulder global sky multiple scattering.
- Keep global atmosphere, local fog, solids, and particles in the same HDR
  composition story: sky radiance, aerial perspective, local in-scatter,
  transmittance, and stochastic coverage must agree.
- Use pass graphs with named temporary delta textures. Avoid monolithic shader
  rituals where nobody knows whether a value is direct light, single scattering,
  accumulated multiple scattering, or display color.
- Treat domain mapping as public API. Texture coordinates should derive from
  radius/angle/distance variables with documented ranges.
- Build CPU/reference versions of critical shader math where possible. The
  Bruneton trick of compiling shared equations into both GLSL and C++ is exactly
  the kind of discipline that keeps beautiful math from becoming beautiful lies.
- For gassy SDFs, consider a Bruneton-like cache only when the volume has stable
  large-scale structure. Dynamic local gas stays stochastic/temporal; stable
  horizon-scale atmosphere gets precomputed transport.

## Sources

- Official documentation:
  `https://ebruneton.github.io/precomputed_atmospheric_scattering/`
- Official implementation clone:
  `.epiphany-aquarium/precomputed_atmospheric_scattering`
- Main files:
  - `atmosphere/constants.h`
  - `atmosphere/definitions.glsl`
  - `atmosphere/functions.glsl`
  - `atmosphere/model.cc`
  - `atmosphere/reference/*`
- Original paper:
  Bruneton and Neyret, "Precomputed Atmospheric Scattering", Eurographics
  Symposium on Rendering 2008.
