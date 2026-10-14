# R14 compact evidence

Fixed 240x135 cells / 1920x1080, same scene and update plan across three views.
Single non-isolated Windows host. CPU and window callbacks are not GPU timestamps or guaranteed FPS.

Frozen baseline source: ac2dfd22c8b690cfca79d4949f058830f061efcc. Binary hashes are local Windows build provenance, not portable build requirements.
R13 Material Room SHA-256: `d0775df145a60a03ceeda44eba005117ab771c68d113cdd33ea93f5bcbd630d6`.
R14 Material Room SHA-256: `d7b2a1f27b6d490843f05fc09755f5415e45f969e0826081df55dadeaad3e127`.
R14 occupancy audit SHA-256: `b83eb353802593f065a2dc58a24baa610a94a18cdd9cb34957b1ceaa02c15548`.

Independent audit: 420 composition records at 240x135, 217,539 ray checks; 12 retained snapshots / 72 view-time checks; 48 transport probes and 48 removal checks. Compiled input scene/plan match runtime assets.

Legacy PNG equality to frozen R13: 120. Dynamic Vulkan/DX12 pairs: 1080.
All 11 commits occur in order; survivor generations remain stable, the recreated panel receives a new generation, and matched history is zero on commit frames.

## CPU cost

Median of three p50 values (ms), first / third / overhead. Legacy compares identical v2 content; dynamic is a different workload, not an optimization comparison.

| Workload | Policy | Workers | Revision | CPU including updates | p95 >16.7 ms | Worst p95 |
| --- | --- | ---: | --- | --- | ---: | ---: |
| legacy | center | 8 | r13 | 4.907 / 4.418 / 5.872 | 0/9 | 6.479 |
| legacy | center | 8 | r14 | 4.582 / 4.576 / 5.405 | 0/9 | 6.547 |
| legacy | conservative9 | 8 | r13 | 16.448 / 15.400 / 17.023 | 8/9 | 20.621 |
| legacy | conservative9 | 8 | r14 | 15.917 / 15.430 / 16.571 | 8/9 | 37.431 |
| dynamic | center | 1 | r14 | 25.473 / 18.841 / 24.694 | 9/9 | 34.328 |
| dynamic | conservative9 | 1 | r14 | 103.365 / 92.938 / 96.836 | 9/9 | 121.589 |
| dynamic | center | 8 | r14 | 4.666 / 3.970 / 5.030 | 0/9 | 6.792 |
| dynamic | conservative9 | 8 | r14 | 15.893 / 15.695 / 16.360 | 9/9 | 19.493 |

Commit-frame update p50 range: 0.2185–0.3319 ms; worst commit-frame p95 0.5759 ms.
Preflight and initial catch-up are excluded from frame timers. Each later update includes candidate construction, validation and atomic publication. Query snapshots stay immutable.

## Native windows

Seven seconds per case, 12 warmups excluded, all events completed. No configuration changes.

| View | Policy | Frames | Callback p50 / p95 ms | CPU total p50 ms |
| --- | --- | ---: | --- | ---: |
| first | center | 690 | 7.239 / 14.257 | 5.050 |
| first | conservative9 | 351 | 19.245 / 22.784 | 17.669 |
| third | center | 638 | 7.784 / 14.648 | 3.901 |
| third | conservative9 | 398 | 16.944 / 20.401 | 15.352 |
| overhead | center | 696 | 7.389 / 14.277 | 5.052 |
| overhead | conservative9 | 392 | 17.368 / 20.733 | 16.008 |

Raw measurements, generated captures and binaries remain local. Transport probes and independent oracle results are described in DYNAMIC_OCCUPANCY.md.
