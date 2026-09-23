# Landscape provenance

The landscape (turf, grass tufts, trees, rocks, bushes, flowers, cliffs, sand,
ocean, terrain elevation, sky reflections and the matte landscape lighting) is
an original Rust/WGSL implementation. No Unity shader source, meshes, textures,
prefabs or binaries are shipped; every texture is generated at startup from
formulas in this crate. The Unity Explorer sources listed below were read for
numeric settings and behaviour only, from an Apache-2.0 checkout.

## Turf and grass

`src/grass_blades.rs` / `src/grass_blades.wgsl`: two crossed cards per tuft,
eight vertices, deterministic placement per parcel, one filtered sample of an
original cutout atlas (`src/grass_cutouts.rs`, eight alpha variants with
coverage-preserving mipmaps, 171 KiB), wind and distance fading. A single matte
ground surface (`src/shell_texturing.rs`) replaces the former stacked shells;
`src/meadow_texture.rs` packs micro-relief, moss flecks and value variation into
one 341 KiB texture and `src/grass_palette.wgsl` blends it with one broad noise
band. The palette is a build constant (`GRASS_COLOR` in `src/grass_look.rs`):
`Green` keeps this repository's LIME roots and tips, `Red` is the painted red
meadow; both use identical geometry, textures and shader.

## Trees, rocks, bushes, flowers

`src/tree_geometry.rs`, `src/tree_leaves.rs`, `src/landscape_trees.rs`,
`src/tree_wind.wgsl`, `src/tree_lighting.wgsl`: curved indexed branch tubes
with transported frames, volumetric leaf sprays, an original 256-square RGBA
atlas of oval leaves and bark ridges (341 KiB), three detail levels
(1,758 / 1,022 / 566 vertices), wind deformation shared by the colour and
shadow passes, and a 56-vertex trunk collider. A smooth 64 m placement field
groups per-parcel candidates into groves and clearings; occupied and unknown
parcels and the parcel-centre arrival point stay clear.

`src/landscape_prop_geometry.rs`, `src/landscape_props.rs`,
`src/landscape_rigid.rs` / `src/landscape_rigid.wgsl`: faceted rocks from the
icosphere primitive with CPU-baked moss on up-facing facets, branching bushes
and five-petal flowers on the foliage atlas, two detail levels, at most nine
candidates per parcel, separate rock collision within 40 m. Rigid surfaces use
a 40-byte vertex (position, normal, colour) with no alpha test or wind.

## Coast, water and terrain

`src/landscape_coast_geometry.rs`, `src/coast_profile.rs` /
`src/coast_profile.wgsl`, `src/coast_surf.wgsl`, `src/landscape_water.wgsl`,
`src/ocean_texture.rs`, `src/landscape_coast.rs`: fractured cliff faces and a
sandy shelf built from the terrain boundary in 64 m chunks (672 vertices,
rounded corners under 900), a shared three-wave shoreline for mesh, sand wash
and ocean foam, a 256-square RG8 slope texture from a distorted fractal height
field (171 KiB with mips) sampled twice per ocean fragment, and four boundary
collision walls. The ocean sits at Y = -19 and opts out of scene-distance fog.

`crates/common/src/terrain.rs`, `src/terrain_loading.rs`,
`src/terrain_mesh.rs`, `src/terrain_support.rs`, `src/ground_mask.rs`: the
occupied-parcel field (from the realm's `world-manifest.json` or a World's
scene definitions), a camera-centred ring mesh with stitched detail levels, a
player-local collider (`SceneColliderData::from_terrain_mesh`), and recovery
of arrivals buried under newly streamed hills. A single-scene World can opt
out with `"landscapeTerrain": false` in its scene metadata.

## Lighting and sky

`src/landscape_lighting.wgsl` evaluates one matte diffuse response per
fragment from the directional, point and spot lights (with Bevy's shadows and
clusters) plus a trilight ambient term (`src/unity_ambient.rs`,
`src/unity_ambient.wgsl`) driven by the time of day; `src/unity_sun.rs` supplies
the sun direction, intensity and colour from a measured day cycle
(`tests/fixtures/sun-cycle.bin`, hold-outs in `sun-holdouts.bin`) whenever no
scene overrides the global light. `src/sky_reflection.rs` filters the
procedural sky cubemap into the primary camera's environment map. The sky's
daytime Mie coefficient drops from 42e-6 to 8e-6 and the purple night fill
fades with solar elevation; sky ray marching is clipped to the forward
atmosphere interval.

## References read (Apache-2.0 unity-explorer checkout, read-only)

`docs/landscape.md`, `Explorer/Assets/DCL/Landscape/**` (settings assets,
`GrassIndirectRenderer.cs`, `GenerateTreeInstancesJob.cs`, `TreeData.cs`,
`TerrainBoundariesGenerator.cs`, `TerrainFactory.cs`, `Ocean.prefab`
transforms, `DCL_Rock.shader` surface contract, stone/tree/grass `.asset`
numbers) and `PackagesLocal/decentraland.grassshader`. Proprietary packages
were excluded. Visual references were screenshots only; no pixels were
extracted. Geometry and texture budgets are regression-tested in the unit
tests of each module.
