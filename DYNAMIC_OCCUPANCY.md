# R14 — dynamic occupancy and snapshot identities

R14 adds client-owned insert, remove and absolute-transform transactions to
Material Room. Every query in a composed frame sees one immutable snapshot.
The renderer receives a small optional history epoch, not mutable game state.
The sample opens a door, moves a target, inserts/removes a blocker and removes
then recreates a panel with the same numeric ID and a new lifetime.

## Contract

`RuntimeScene::apply_transaction(expected_revision, operations)` validates a
candidate before publishing its document, live handles, revision and history
epoch together. `SceneRevision` includes a namespace and revision;
`ObjectHandle` includes namespace, numeric ID and generation. Foreign and stale
handles/revisions fail. Surviving handles stay stable. Reusing a removed ID gets
a new generation even when no frame was rendered between the two commits.
There is no unbounded tombstone map: only live handles, at most 128, are retained.
Handles and epochs are process-local tokens, not a persistent save-file format.

One batch accepts at most 128 operations and one operation per ID. The validated
final scene has at most 128 objects. Removing and reinserting one ID requires
two transactions. Insert uses the document's existing v1/v2 object schema and
material IDs; SetTransform supplies absolute position, half extents and yaw and
removes any authored procedural motion. Bit-identical transforms without motion
are no-ops; signed zero differences and removing motion are real changes.
Failed, empty and effective-no-op batches preserve revision, epoch and generation
counter. Checked counters return an error on exhaustion instead of wrapping.

The candidate currently clones and reparses the bounded JSON document at each
effective event. This is a correctness-first publication boundary, not an
incremental spatial index. Old snapshots retain their own document and identity
table, including queries at times other than their prepared time. All query and
shading hooks delegate to that same inner snapshot; traversal and primitive arithmetic
remain the ordinary R12/R13 path. V1 authored tie order and v2 stable ID tie
order are preserved.

`Scene::history_epoch()` defaults to zero. Composer reads it once before worker
queries and clears all previous glyph cells on any change, including a return
to zero or an older snapshot. Material Room allocates non-recycled process-wide
tokens for independent runtimes and effective commits. Other producers must
coordinate their token identities or explicitly reset Composer when switching
worlds. Equal tokens retain the existing history rules. The conservative global
reset covers indirect shadow/reflection dependencies; this is not selective
per-object invalidation. Hit IDs and surface-history hashing are unchanged.

## Replay client and controls

The public runtime API works independently of the demo replay. `--updates FILE`
loads version 1 event data: strictly increasing absolute 30 Hz ticks and insert,
remove or transform operations. The plan is limited to 1 MiB, 256 events and
128 operations/event, with event ticks from zero through one day. Unknown or
duplicate fields, invalid geometry and invalid future references are rejected.
The whole plan is preflighted on disposable state before playback; events then
execute real runtime transactions. Catch-up applies every due event in order;
atomicity is per event, not the whole multi-event catch-up. Playback is one pass,
and after the last event retains final occupancy while authored motion continues.

`Avvia-occupazione.cmd` rebuilds and opens the sample. Keys 1/2/3 choose the three
authored cameras, Space pauses, C changes sampling and R reloads scene plus plan
atomically and restarts this dynamic replay. An invalid reload retains the
existing world, clock, camera and glyph history. Ordinary runs without updates
keep their previous reload/time behavior. No simulation state lives in the core.

```sh
cargo build --locked --release --workspace --bins
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/dynamic-room-v2.json --updates clients/material-room/assets/dynamic-updates-v1.json --validate
cargo run --locked --release -p ascii-material-room -- --scene clients/material-room/assets/dynamic-room-v2.json --updates clients/material-room/assets/dynamic-updates-v1.json --sequence 180 --camera all --profile detail --workers 8 --sampling center --output evidence/my-occupancy
cargo run --locked --release -p ascii-material-room --example occupancy_audit -- --cols 240 --rows 135 --output evidence/my-occupancy-audit
```

## Verification protocol

Baseline is published R13 `ac2dfd22c8b690cfca79d4949f058830f061efcc`.
The acceptance oracle rebuilds each expected state independently from JSON using
the frozen R12 producer (the ordinary R13 producer retained that path). It checks
hit bits, composed cells, history, retained snapshots and out-of-prepared-time
fallbacks. Four controlled one-cell probes isolate actual Composer primary,
shadow, diffuse-normal and local-specular queries and require changed RGB plus
oracle equality; removal must undo each effect. Those probes share an axis to
isolate transport. The full room uses three distinct authored cameras.

`scripts/verify-r14.py` serializes baseline/legacy, dynamic GPU, CPU and window
phases. Preserve R13 binaries under `evidence/r14/baseline` before building R14.
Run phases `baseline`, `legacy`, `audit`, `dynamic`, `measure`, `windows`, then `summary`.
The script validates scene/plan, density, policy, worker count, all event
revisions/generations and reset history on commit frames. Generated media,
binaries and raw logs stay local; the compact summary is versioned.
This local evidence gate pins the SHA-256 hashes of the preserved Windows R13
binaries; those hashes are not a portable build requirement. A separately built
baseline needs its own reviewed provenance and hash record before repeating the
paired comparison. The ordinary client/audit commands above do not depend on
these local baseline binaries.

The completed serialized checks pass 376 workspace test executions
(including historical fixture copies), fmt, clippy with `-D warnings` and release
builds of both clients and all examples. A further 23 client acceptance tests
(12 new checks and 11 frozen-producer tests) and four core history tests pass in
release. The first compile found two missing
test-fixture constant exports; clippy then found two unused imports. Both were
corrected and the failed logs retained. Windows incremental-cache hard-link
warnings fell back to copying and did not prevent successful checks.

The 240x135 independent audit passes 420 composition records over 35 boundary
ticks, three cameras, both policies and one/eight workers; 217,539 ray checks,
12 retained snapshots with 72 view/time checks, 48 transport probes and 48 removal
checks. No numeric ID or primitive tie semantics changed. The 120 legacy PNGs
(96 client and 24 courtyard) are identical to frozen R13 across Vulkan and DX12.

GPU sequences contain 180 frames for each of three views, two policies and two
backends: 2,160 PNGs and 1,080 identical Vulkan/DX12 pairs. All 11 effective events
occur in order, surviving generations are stable and history matches are zero on
commit frames. Initial frames match R13; the mid-sequence occupancy differs.
An auxiliary ad-hoc metadata check initially used the Windows default text
encoding; repeating it with explicit UTF-8 passed. The checked-in verifier uses
explicit UTF-8 throughout.

Visual inspection sampled Center frames before insertion, after removal and
after recreation, plus the open-door state with Conservative9 in all three
views. The colored glyph families distinguish the subjects and no subject is
clipped by the image border in those samples. The blocker intentionally hides
most of the red target from the third camera during part of the sequence. The
door changes in discrete steps; smooth interpolation is not implemented. These
are technical occupancy samples, not new artwork or perceptual-motion approval.

## Measured cost

Median of three p50 values, milliseconds, first / third / overhead. Total CPU
includes update, snapshot and composition. The dynamic room differs from legacy
v2, so their values are not an optimization comparison.

| Workload | Policy | Workers | Revision | CPU p50 medians | Runs p95 >16.7 ms | Worst p95 |
| --- | --- | ---: | --- | --- | ---: | ---: |
| Legacy v2 | Center | 8 | R13 | 4.907 / 4.418 / 5.872 | 0/9 | 6.479 |
| Legacy v2 | Center | 8 | R14 | 4.582 / 4.576 / 5.405 | 0/9 | 6.547 |
| Legacy v2 | Conservative9 | 8 | R13 | 16.448 / 15.400 / 17.023 | 8/9 | 20.621 |
| Legacy v2 | Conservative9 | 8 | R14 | 15.917 / 15.430 / 16.571 | 8/9 | 37.431 |
| Dynamic | Center | 1 | R14 | 25.473 / 18.841 / 24.694 | 9/9 | 34.328 |
| Dynamic | Center | 8 | R14 | 4.666 / 3.970 / 5.030 | 0/9 | 6.792 |
| Dynamic | Conservative9 | 1 | R14 | 103.365 / 92.938 / 96.836 | 9/9 | 121.589 |
| Dynamic | Conservative9 | 8 | R14 | 15.893 / 15.695 / 16.360 | 9/9 | 19.493 |

Commit-frame update p50 spans 0.2185–0.3319 ms; worst commit-frame p95 is
0.5759 ms. It includes candidate construction, validation and publication.
This matrix does not establish a runtime speedup: legacy medians vary in both
directions, and the R14 legacy Nine worst p95 is materially higher (37.431 ms).
That spike is retained, not discarded. No work was hidden by reducing density or
omitting update cost. General coverage/motion/cost remains an open gate.

The cost matrix uses three serialized repetitions. The unchanged v2 workload
compares R13/R14 in AB/BA/AB order at eight workers, 90 measured frames per view,
for both policies. The dynamic workload is separate: 180 frames per view with
one/eight workers and both policies. This totals 72 cases and 9,720 measured
frames. Every run uses 240x135 cells, 1920x1080 output metadata, the authored
camera with no orbit; all dynamic runs use the same fixed plan, while legacy
runs have no update plan. CPU-only mode does not capture
images or wait on the GPU. Twelve warmups are excluded. Event cost applies to
this small seven/eight-object room and does not establish a 128-object update
budget. The 128-object limit is covered by functional tests, not a performance
guarantee. All percentile exceedances remain in the report.

## Native-window verification

Six seven-second Vulkan runs complete all 11 events with no configuration changes,
240x135 cells and actual 1920x1080 windows. Center records 690 / 638 / 696 frames;
Conservative9 records 351 / 398 / 392. Callback p50/p95 in the three views is
7.239/14.257, 7.784/14.648 and 7.389/14.277 ms for Center, versus
19.245/22.784, 16.944/20.401 and 17.368/20.733 ms for Nine. Twelve warmups are
excluded. These native callback measurements are separate from inspection of
GPU-captured frames and do not establish scanout timing or universal FPS.
The independent courtyard completes a two-second Vulkan smoke with 190 frames.
The release CLI also validates the shipped scene and all 11 future events.

The aggregate provenance/metadata/parity gate passes. Full compact numbers and
binary provenance are in [the evidence summary](evidence/r14/summary.md).

## Scope and remaining limits

This closes only the bounded occupancy/lifetime integration. It does not add
collision response, fracture, physics, an editor, save-game
identity persistence, a spatial hierarchy or faster query traversal. General
coverage/motion/cost and another physical machine remain open. Conservative9
still widens silhouettes and misses between its samples. Environment transport
remains one normal visibility ray and at most one local specular bounce, not GI.
The technical colored shapes are not final artwork; transport probes do not
certify recognizable reflected objects or artistic reflection quality.

Performance reports must include event publication cost separately from snapshot
and composition. Preflight and initial catch-up are outside frame timers. GPU
submission/wait and native callback intervals are not GPU timestamps or guaranteed
FPS. All current execution evidence is from one non-isolated Windows x64 host,
Ryzen 7 3800X/Radeon RX 6700 XT, not a minimum hardware specification.
