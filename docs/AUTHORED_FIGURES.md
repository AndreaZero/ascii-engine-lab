# R16 bounded figure authoring

This experimental producer is example content outside `ascii-render`. It creates
semantic hits directly; it has no mesh importer or intermediate world image.
It does not extend the Material Room project format. See [scope and results](../VISUAL_CAPACITY.md).

```powershell
cargo build --locked --release --example visual_capacity
Copy-Item examples/assets/figures/warden.json my-figure.json
# Edit your copy, then read the changed JSON without rebuilding:
target/release/examples/visual_capacity.exe --asset my-figure.json --figure warden --surface clay --camera first --distance near --window --seconds 60
target/release/examples/visual_capacity.exe --asset my-figure.json --surface structured --camera all --distance middle --path rotate --frames 60 --backend vulkan --out evidence/my-figure
```

`--figure` selects baseline/warden/courier and labels reports. `--asset` overrides
the authored JSON; the full loaded text is included in each report. Baseline is
always the R5 fixture in clay and ignores custom content: do not combine baseline
with `--asset`. Default assets are read from the source checkout, so this example
is not a replacement relocatable R15 package. Keep custom files outside the
published asset folder unless intentionally contributing original source data.

Use `--surface clay|structured`, `--distance near|middle|far`, `--camera
first|third|overhead|all`, and `--path static|rotate|camera|deform`. A native window
requires one camera; keys 1/2/3 switch views and Escape exits. Headless output
uses real wgpu PNGs; `--benchmark` skips GPU work. `--sampling center|conservative9`
is explicit, Center default. Density stays 240x135 and output 1920x1080.

The two supplied figures use the same parser and algorithms. To change the cut,
edit panel height and bottom/top half-width, position and orientation; change
part radii/centers for proportions. IDs are stable authored identities, not array
positions. Primitive traversal follows authored array order on equal distances;
reordering overlapping parts may change ties. There is no savegame identity.

## JSON version 1

Root fields: `version: 1`, optional descriptive `name`, and arrays `materials`,
`parts`, `panels`. Maximum 256 KiB, 16 materials, 48 parts, six panels; at least
one part is required. Unknown fields and duplicate numeric IDs are rejected.
JSON duplicate member names follow serde_json Value's last-wins behavior; this
is a bounded experiment, not the stricter Material Room document validator.

Materials require `id >=32`, linear `color: [r,g,b]` in [0,1]. Optional
`roughness` is [0.04,1], default .8; `feature_scale` is [.01,1] world units,
default .08; `structure` defaults true. Feature scale is a manual footprint
cutoff, not an exact measurement of normalized seam width. Roughness is an
authored scalar dielectric parameter; no fabric scattering model is claimed.

Parts require unique `id >=100`, existing `material`, `center`, positive `radii`
in [.005,2]. Optional `rotation` is Euler XYZ radians (default zero), `motion`
is per-axis sinusoidal displacement (each axis at most .15), and `speed` is
angular frequency within [-8,8], default1.6. Centers stay within +/-4.
This is bounded procedural motion, not skeletal skinning or locomotion.

Panels require unique `id`, existing `material`, `origin`, `height` [.05,2],
`bottom` and `top` half-widths [.02,1]. Local y rises from zero to height;
local x spans minus/plus interpolated half-width. Optional `rotation` uses XYZ
radians. `slope` (absolute <=.5) changes lower depth; `folds` [0,6], `amplitude`
(absolute <=.08), `phase`, `sway` (absolute <=.1) and `speed` (absolute <=8)
control bounded analytic folds and deformation. Defaults are 0/2/.02/0/.025/1.7.
Origins stay within +/-4; all numeric inputs must be finite and bounded.

The graph is opaque from both sides but has zero thickness. It cannot represent
overhangs, self-folding or collision. A fixed 32-interval/eight-bisection solver
can miss tangencies or two crossings in one interval. Add volumetric components
only where the intended shape requires them; do not call the solver universal.

Local coordinates follow components and deformation. Structure classifies
normalized borders/hem; folds affect geometry and shading, not a dedicated
fold-aware glyph grammar. Fine marks are removed at larger footprints. Snapshot
and surface rules remain deterministic, and Composer history keys use the same
classification. Surface color, ink density and geometric occupancy are different
quantities: judge all three in the captured comparisons.
