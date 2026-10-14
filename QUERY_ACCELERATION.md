# R13 — bounded query cost

Status: verified, 14 October 2026. Bounded helper delivered; client integration
rejected for cost. Final default-runtime checks passed.
Baseline: R12 `8c27e5c70c8f44c354dacb2eb33506df7127e457`.

The exact bounded helper is available in the shared core, but ordinary Material
Room queries retain the R12 algorithm. The experiment reduced slab work while
increasing measured CPU cost. R13 does not claim a delivered frame-time speedup.
The experimental implementation is reproducible from commit
`16a17cd9df6d2deb209a1cdd4be5555db0f25bab`; its results are kept separate from
checks of the final default executable.

## Fixed contract

Reduce query work while preserving complete hit bits, tie priority, cells,
history and density. The v1, v2 and coverage JSON scenes, camera path, shading,
Center default and optional Conservative9 sampling remain unchanged.

The reusable core helper `intersect_box_before(ray, min, max, limit)` has the
contract `intersect_box(ray, min, max).filter(|hit| hit.0 < limit)`. The limit
is a ray parameter; it is a world distance only for a unit direction. The
original box and ellipsoid solvers remain unchanged. The new helper preserves slab
arithmetic and normal tie rules, but can stop examining axes when the current
entry distance is greater than `0.0001` and at least the strict limit. That
entry only increases on subsequent axes, so it cannot recover a result below
the limit. Inside rays retain the original exit-distance path. With an infinite
limit the ordinary helper calls the original solver and applies the strict
filter once; it does not add distance checks on each axis. The counted version
specializes the same slab loop without per-axis bound checks. A raw infinite
hit is still filtered out.

The experimental adapter passes its already validated nearest distance or the shadow limit.
Invalid candidates never tighten the bound. Equal distances retain the first
valid hit: v1 array order, v2 sorted IDs. Ellipsoids retain their original
solver and complete Hit reconstruction. Material Room shadows can stop after
the first fully validated blocker because all its material IDs are >=32,
excluding WATER and EMISSIVE roles. General Scene emissive/nearest shadow
behavior is unchanged.

The final client's Scene methods use the original R12 traversal and default
occlusion method. Explicit `trace_bounded_counted` / `occluded_bounded_counted`
methods retain the experiment for diagnostics, without selecting it at runtime.
Their counters must not be described as work performed by the ordinary path.

Object layout, independent quaternion transforms and snapshot preparation
follow R12. Different-time queries rebuild a snapshot as before. There is no
BVH: nearest queries still visit every object. No lower density, approximate
geometry or new quality mode is used to reduce cost.

## Numerical boundaries

Preserving existing floating-point results is stricter than preserving ideal
geometry. The ellipsoid quadratic can produce a finite hit outside the ideal
volume: unit sphere at zero, origin `(10000,1.01,0)`, direction `(-1,0,0)`.
The box solver treats components below `1e-8` as parallel, also permitting
legacy out-of-volume hits at long distances. A conventional world AABB could
remove these hits. R13 preserves them, without claiming numerical robustness
or assuming an arbitrary padding proves equivalence.

The bounded helper uses a monotonic quantity inside the same solver. Tests
compare every returned distance/normal bit with the original filtered result,
including NaN/infinite limits, nonfinite local inputs, signed zero, inside
rays, axis ties, threshold neighbors and 20,000 deterministic finite/raw bit
cases. This is a stated rejection argument plus a finite differential corpus,
not exhaustive enumeration of all floating-point inputs.

## Rejected candidates

Preparing inverse quaternions, deferring complete Hits and reusing directions
for consecutive identical rotations reduced selected operation counts but
increased elapsed cost. They are excluded from the selected path.

- The first deferred-Hit pilot stopped deliberately after 15 completed commands.
  V2 Nine/eight-worker p50 was 17.47/17.81/16.89 ms versus R12
  16.23/15.77/15.37. Local evidence: `evidence/r13/pilot-deferred-hit`.
- Adding direction reuse still regressed in the complete 144-case, three-repeat
  matrix. V2 Nine/eight-worker median p50 was 17.027/16.919/16.933 ms versus
  R12 15.919/15.424/15.158; coverage Nine was 24.107/22.704/21.589 versus
  20.092/19.521/19.575. Local source, binary, outputs and matrix:
  `evidence/r13/reuse-deferred`.
- Restoring complete Hit construction while keeping direction reuse failed
  its short pilot too: v2 Nine 19.231/18.418/20.772 versus
  15.936/14.951/15.000 ms. Evidence: `evidence/r13/trace-reuse`.
- The first bounded implementation checked the limit on every axis, even for
  unbounded queries. Its matrix was stopped after 21 completed commands when
  the infinite-limit specialization was prepared. Results were mixed and
  contended; they remain in `evidence/r13/bounded-general`, alongside its
  binary and 1,152 matching PNGs. This incomplete matrix is not a final result.

The deferred candidate avoided only 707 of 1,720,109 v2 primary Hit builds and
none in coverage. Direction-transform counts fell 37.5%/91.67% respectively;
this did not reduce elapsed time. Those counts do not describe the final path.
An earlier contended short pilot is separate under
`reuse-deferred/contended-pilot`: frozen R12 v2 Nine p50 reached 74 ms and a
candidate Center p95 reached 973 ms. Host readings without our benchmark were
79%, then 53/44/40% before the full rejected matrix. No unrelated process was
stopped or reprioritized. Pilots are not pooled with final measurements; host
contention does not erase consistent paired regressions.

## Verification and reproduction

`tests/support/r12_content.rs` freezes R12 producer blob
`cb24dde3d4726c71f36180605c66e42e557ae474`, using the unchanged original
primitive solvers. Independent tests compare all 12 Hit fields as bits, every
cell field and all FrameStats. JSON blobs are guarded independently of both
parsers. The corpus includes 238,545 camera/subcell rays, directed edges,
0/1/128 objects, order/ID ties, motion, invalid rays and time fallback. Another
216 composition frames cover both sampling policies, three views, one/eight
workers, repeated ticks and history reset.

Final checks: 324 workspace test executions passed, including historical
fixtures; fmt, clippy with warnings denied and locked release build passed.
The 20 producer parity tests and six bounded-helper tests also passed in release.
Both the experiment and final default match 1,152 R12 PNGs each; 36 experimental
Vulkan/DX12 pairs match. The final audit passes 54 packaged and 18 synthetic
records. Six experimental and six final-default native windows pass in all
three views and both sampling modes, at actual 1920x1080.

The original courtyard client also runs: 24 Vulkan/DX12 PNGs match its R12
captures and a Vulkan window completes 191 frames. Selected final coverage
frames at tick 90 were visually inspected in all three views and both policies.
Thin bars, occluders and sampling gaps retain the R12 appearance; this diagnostic
scene does not establish final artistic quality. Native-window completion is
runtime evidence, distinct from the sampled offscreen visual inspection.

`scripts/verify-r13.py` serializes captures, paired CPU measurements and real
windows. Preserve an R12 release binary at
`evidence/r13/baseline/ascii-material-room.exe` (omit `.exe` outside Windows).
To recreate it, export commit `8c27e5c` into a separate directory and build its
Material Room with locked dependencies before building current source. The
original Windows baseline SHA256 is
`17d355be61e50194c0763ecb2d49aee936eaba985de261bd38b78d372cb3fed6`.

Similarly export experiment commit `16a17cd` separately, build its Material Room
and preserve it in `evidence/r13/candidate`. Do not substitute the final default
binary: the paired comparison intentionally measures the rejected experiment.
Its Windows binary SHA256 is
`4b60f0ce940e49a767f49a89d906b1c5f57fc32e1870fc50c3d106f631027207`.
Build the current workspace binaries and the `query_audit` example separately.
Run script phases `baseline`, `runtime`, `measure`, `windows`, `default-runtime`,
`default-windows`, `summary`. Runtime/measure/windows
use the frozen candidate; default phases use the current target binary.
Run the release `query_audit --stress128` separately
at its default 240x135/one worker. It forwards all Scene hooks and compares
instrumented/plain cells and counters: 54 packaged records plus 18 separately
labelled synthetic 128-object records. It compares bounded diagnostic output
with the final ordinary traversal. Only after this audit has produced
`evidence/r13/query-audit/query-audit.json`, run `scripts/summarize-r13.py`.
Never overlap builds, audits, captures, encoding or measurements.

The paired matrix uses fixed JSON, 240x135 cells, 12 warmups, 120 frames per
view, one/eight workers and three AB/BA/AB repetitions. Both revisions use the
R12 coverage camera path. Snapshot and composition timers are separate. The
1,152 expected PNGs comprise 72 endpoints across three scenes, two policies
and Vulkan/DX12, plus 1,080 motion frames: six simulated seconds at 30 Hz per
view/policy. Six real windows run coverage for six seconds each at 1920x1080.
Their callback intervals and CPU submission/wait are not GPU timestamps,
scanout latency or proof of 60 FPS. Summary gates verify executable hashes,
scene data, matrix completeness and measurement metadata.

`intersect_box_before_counted` reports slab axes, including parallel checks,
not division counts. Client diagnostics replay it with infinite limit on the
same visited boxes, outside timing. Unvisited shadow objects are excluded
from that comparison. R12 full-scan primitive counts are explicitly derived
as object count times calls, not measured from the frozen binary. Normal
runtime does not invoke the bounded diagnostic, its counters or its replays.

In the 18 packaged samples per scene, v2 primary queries examine 38,958,956
box axes versus 41,666,771 for the infinite-limit replay of those same boxes
(6.50% fewer). Coverage primary queries examine 68,681,320 versus 74,434,015
(7.73% fewer). Primary primitive counts remain 23,328,000 and 34,992,000.
V2/coverage shadow axes are 4,684,584/5,009,521 versus same-visited-box replay
5,251,124/5,715,451; shadow primitive tests are 2,705,547/2,689,550 versus
derived R12 full scans 2,816,424/2,909,052. These are work counts, not CPU gains.

## Result and remaining gate

The complete paired matrix contains 144 cases / 17,280 measured CPU frames.
Median p50 values over three repetitions, first/third/overhead, in milliseconds:

| Scene, eight workers | R12 | Bounded experiment | p95 >16.7 ms, R12 / experiment |
| --- | --- | --- | --- |
| v2, Center | 4.308 / 4.171 / 4.677 | 5.348 / 4.607 / 5.594 | 0/9 / 0/9 |
| v2, Conservative9 | 15.260 / 15.114 / 15.365 | 18.606 / 18.167 / 18.528 | 8/9 / 9/9 |
| coverage, Center | 4.358 / 3.899 / 4.700 | 4.532 / 4.358 / 4.983 | 0/9 / 0/9 |
| coverage, Conservative9 | 21.944 / 21.721 / 22.361 | 25.644 / 23.207 / 23.871 | 9/9 / 9/9 |

Across one/eight-worker cases, 53/72 R12 and 54/72 experimental runs exceed
p95 16.7 ms; worst p95 is 201.52 / 209.95 ms. One-worker runs account for 36
exceedances per revision. These are the same moving workloads in an interleaved
comparison, not comparisons against older reports with a different camera path.
The host was not isolated, but the repeated paired result supplies no basis
for adopting this candidate. Operation reduction did not satisfy the elapsed
cost criterion. It was rejected, rather than presented as an acceleration.

Hardware: Ryzen 7 3800X / Radeon RX 6700 XT, Windows x64; rustc/cargo 1.97.1,
LLVM 22.1.6, locked wgpu 26.0.1. These are test conditions, not minimum specs.
The [compact evidence table](evidence/r13/summary.md) includes all one/eight-worker
costs, snapshot timings, experimental counts and separate window checks.
The final Material Room executable SHA256 is
`d0775df145a60a03ceeda44eba005117ab771c68d113cdd33ea93f5bcbd630d6`.

The general geometry/motion/cost gate stays open:
linear traversal, another physical machine, numerical robustness and dynamic
occupancy require further bounded work.

Only source, tests, scripts and compact reports are published. Generated
captures, binaries, raw measurements and private references remain local.
