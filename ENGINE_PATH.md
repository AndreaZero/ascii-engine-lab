# From rendering laboratory to usable engine

This is the product path, not a count of visual studies or a percentage complete.
Current verified capabilities and measurements belong in [PROJECT_STATUS](PROJECT_STATUS.md).
The renderer remains experimental. BLACKWATER gameplay development is paused.

## Why a visual study can be engine development

A study must answer a reusable engineering question, with an explicit comparison,
acceptance criteria and measured costs. R7 introduced local material sampling;
R9 introduced conductor response; R10 investigates opt-in environment transport
and composition of different materials in one immutable scene. R8 primarily
refined the wet material's glyph vocabulary. That content improvement is useful,
but is not a new lighting capability.

Geometry, material artwork and the demonstration character remain client content.
The core owns visibility/composition, material/light contracts, temporal handling
and glyph presentation. A polished sample alone does not prove an engine is usable.
Do not keep adding material showcases without addressing the gaps below.

## Data gate: closed within the R11 scope

[R11](ALPHA_DATA.md) extends independent Material Room to consume local surfaces,
conductor parameters, the bounded environment contract and client-owned cameras
through validated version 2 data. Version 1 remains compatible; migration is
explicit and field errors identify invalid data. Both clients, legacy regressions
and a clean-source build/run guide passed on the same physical machine. This is
a bounded alpha integration result, not final art, an editor or production V1.

Acceptance criteria verified in that scope:

- Scene/material/camera settings change through client data without a patch to
  the renderer or imports of the courtyard's actor/world modules.
- One immutable client snapshot feeds primary, shadow and reflection queries.
  Reload is atomic; changed structural identities invalidate history explicitly.
- Both clients still run; legacy output remains protected by regression checks.
- A fresh checkout can follow the build/run guide with locked dependencies.
  A clean checkout on the same PC is not evidence for another physical machine.
- Repeated CLI/window/capture code is consolidated only where its behavior is
  actually shared. Do not move artistic fixture content into the renderer.

This gate does not require an editor or a universal asset format. If the common
scene misses its declared frame budget, retain that failure and choose a bounded
profiling/quality experiment before promising production performance.

## Remaining gates

[R12](GEOMETRY_COVERAGE.md) verifies a bounded primary-coverage option and a
comparison harness. Conservative9 recovers thin foreground, but introduces
silhouette expansion, residual sample gaps and extra query cost: 8/9 eight-worker
CPU-only runs exceed p95 16.7 ms. Center remains the default. R11's earlier 4/9
failures are retained; the general geometry/motion/cost gate is not closed.

[R13](QUERY_ACCELERATION.md) performs the bounded query-work/cost comparison on
those preserved scenes and paths. A reusable distance-limited box solver can
skip remaining slab axes, and experimental client-specific shadow queries can
stop early. Paired CPU cost regressed despite fewer axes, so the integration
was rejected and the ordinary client retains R12 traversal. Explicit bounded
diagnostics and the core helper remain available; no runtime speedup is claimed.
Cache/deferred-Hit candidates were also rejected. Primary traversal
remains linear. Numerical counterexamples
prevent treating an ideal geometric AABB as an automatically equivalent filter
for the existing f32 primitive solvers. A spatial hierarchy remains future work
requiring explicit numerical semantics and evidence, not a capability delivered
by these changes. The first unsuccessful pilot and host contention are retained.

[R14](DYNAMIC_OCCUPANCY.md) closes the bounded dynamic-occupancy lifetime gate
for Material Room: atomic insertion/removal/absolute transforms, stable survivor
handles, a new generation when an ID is reused, and immutable retained snapshots.
An optional core history epoch resets all glyph history on effective commits,
including indirect receivers. Independent reconstruction verifies primary,
shadow, diffuse and local-reflection state. This is not physics, fracture,
selective history invalidation or a faster spatial index. Publication currently
clones and validates a bounded document at each event; its cost is reported
separately and included in total CPU cost. General geometry/motion/cost stays open.

[R15](AUTHORING_PORTABILITY.md) delivers practical creation for this supported
subset: a self-contained scene/update/camera project, validated CLI edits,
native preview controls and protected save/reopen without core edits. Camera
controls preserve exact rays rather than baking rounded world-space poses.
The relocatable Windows package includes both clients and a diagnostic probe;
it still requires the MSVC runtime/UCRT. This is a bounded authoring tool, not
a complete editor or savegame system. The combined gate remains partial:
another physical machine is still an external validation gap, not something
a clean checkout, relocation or fewer workers can emulate. Consume that probe
on another physical host, then address demonstrated failures and declare the
BLACKWATER V1 subset before broader integration. Continue measuring at fixed
density and preserve the failed R13 acceleration result. No new writer or
future tranche is assigned automatically by this document.

| Gate | Engineering work | Evidence needed to close it |
| --- | --- | --- |
| Geometry coverage and motion | R12 opt-in primary sampling and R13 exact bounded-helper experiment verified; runtime integration rejected for cost, linear traversal retained, sample gaps and foreground dilation remain | Preserve R11/R12 scene/path and tie semantics, dense-reference coverage, three-view motion and CPU/window pacing without hidden quality reductions |
| Dynamic scene updates — bounded gate closed in R14 | Client-owned atomic occupancy changes and lifetime handles; global history epoch | Independent query/cell reconstruction, retained snapshots, rollback/anti-ABA, three-view replay and separate update cost; no physics or selective invalidation implied |
| Practical creation and portability — partial after R15 | Validated create/edit/preview/reload/save/reopen and relocatable Windows package/probe for the supported subset | Local authoring and relocation evidence; execution on another physical machine still required |
| V1 for a declared BLACKWATER subset | Fix supported scenes, actors, effects, distances, quality and hardware budget | A client supplies its state through documented APIs and the complete agreed workload passes; gameplay integration needs its own authorized task |

Fire, richer water, smoke/transparency and destructible environments require
separate bounded studies. Emissive color is not a fire system; changing occupancy
is not a complete fracture/physics system; procedural cloth is not cloth physics.
Prioritize these from the intended game subset rather than assuming that they
arrive automatically with a renderer.

## Method to preserve between coordinators

Choose one reusable capability or demonstrated defect per tranche. State its
contract, baseline, fixed workload and exit criteria before editing. Assign
disjoint ownership; serialize builds, benchmarks and captures. Preserve raw
failures and separate tests, visual judgment and performance evidence. Record
implemented, approximated and missing behavior. Publish reviewed source/reports,
excluding generated media, caches and private references. Update project status
and the website snapshot only after verification.

Do not infer engine completion from a nice video, a passing unit suite or one
fast CPU percentile. No delivery date, universal compatibility or superiority
over another engine has been established by these studies.
