# R13 compact evidence

Paired R12/bounded-experiment binaries, unchanged scene JSON, coverage camera path, 240x135 cells.
The bounded path was not adopted as the runtime default. Final Material Room retains R12 traversal.
Experiment source: `16a17cd9df6d2deb209a1cdd4be5555db0f25bab`.
CPU-only, 12 warmups + 120 frames per view, three interleaved repetitions.
Single non-isolated Windows host; these numbers are not FPS or GPU timestamps.

Experiment PNG equality to R12: 1152; backend pairs: 36.
Final default PNG equality to R12: 1152.

## CPU snapshot/camera/composition

Median of three p50 values (ms); p95 failures count individual runs above 16.7 ms.

| Scene | Sampling | Workers | Revision | First | Third | Overhead | p95 >16.7 | Worst p95 |
| --- | --- | ---: | --- | ---: | ---: | ---: | ---: | ---: |
| v2 | center | 1 | R12 | 22.659 | 23.542 | 25.646 | 9/9 | 33.790 |
| v2 | center | 1 | bounded experiment | 27.121 | 27.440 | 29.521 | 9/9 | 35.914 |
| v2 | center | 8 | R12 | 4.308 | 4.171 | 4.677 | 0/9 | 6.580 |
| v2 | center | 8 | bounded experiment | 5.348 | 4.607 | 5.594 | 0/9 | 11.252 |
| v2 | conservative9 | 1 | R12 | 102.734 | 96.780 | 94.994 | 9/9 | 201.515 |
| v2 | conservative9 | 1 | bounded experiment | 112.198 | 113.586 | 106.487 | 9/9 | 163.984 |
| v2 | conservative9 | 8 | R12 | 15.260 | 15.114 | 15.365 | 8/9 | 29.479 |
| v2 | conservative9 | 8 | bounded experiment | 18.606 | 18.167 | 18.528 | 9/9 | 47.941 |
| coverage | center | 1 | R12 | 20.324 | 19.897 | 23.575 | 9/9 | 35.436 |
| coverage | center | 1 | bounded experiment | 22.316 | 20.074 | 23.177 | 9/9 | 41.092 |
| coverage | center | 8 | R12 | 4.358 | 3.899 | 4.700 | 0/9 | 6.005 |
| coverage | center | 8 | bounded experiment | 4.532 | 4.358 | 4.983 | 0/9 | 7.752 |
| coverage | conservative9 | 1 | R12 | 132.880 | 127.070 | 126.073 | 9/9 | 175.450 |
| coverage | conservative9 | 1 | bounded experiment | 152.042 | 142.402 | 137.767 | 9/9 | 209.948 |
| coverage | conservative9 | 8 | R12 | 21.944 | 21.721 | 22.361 | 9/9 | 41.061 |
| coverage | conservative9 | 8 | bounded experiment | 25.644 | 23.207 | 23.871 | 9/9 | 32.342 |

Snapshot timer p50 ranges across the 72 runs of each revision (ms):

- R12: 0.001400 to 0.004200.
- bounded experiment: 0.001600 to 0.003400.

## Untimed bounded-experiment work

These counters describe the explicit bounded diagnostic, not the final default traversal.

Totals per scene across three views, both sampling modes, t=0/3/6. R12 primitive/rotation counts are derived N*calls, not instrumented timings.

| Scene | Category | Calls | R12 derived kernels | Actual kernels | Actual direction transforms | Full Hit builds | Candidate hits | Box axes | Unbounded axes, visited boxes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| packaged-v1 | primary | 2916000 | 20412000 | 20412000 | 20412000 | 1646610 | 1646610 | 39695523 | 41970614 |
| packaged-v1 | secondary | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| packaged-v1 | shadow | 282696 | 1978872 | 1921022 | 1921022 | 22153 | 22153 | 3745290 | 4214702 |
| packaged-v2 | primary | 2916000 | 23328000 | 23328000 | 23328000 | 1719402 | 1719402 | 38958956 | 41666771 |
| packaged-v2 | secondary | 604398 | 4835184 | 4835184 | 4835184 | 96935 | 96935 | 7519682 | 7587341 |
| packaged-v2 | shadow | 352053 | 2816424 | 2705547 | 2705547 | 33509 | 33509 | 4684584 | 5251124 |
| coverage-v2 | primary | 2916000 | 34992000 | 34992000 | 34992000 | 1529072 | 1529072 | 68681320 | 74434015 |
| coverage-v2 | secondary | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| coverage-v2 | shadow | 242421 | 2909052 | 2689550 | 2689550 | 30497 | 30497 | 5009521 | 5715451 |

Primary/secondary still visit every object. Box axes include parallel-axis checks, not division counts. The unbounded comparison only covers the same visited boxes; shadow early exits hide later candidates, whose previous work cannot be inferred. Counts are not elapsed-time percentages.

## Bounded-experiment window callback pacing

Six seconds per case, 12 warmups, unchanged configuration, requested/actual 1920x1080. Wall-clock callback intervals are not GPU time or scanout latency.

| View | Sampling | Frames | Interval p50 ms | Interval p95 ms | CPU p50 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| first | center | 320 | 16.810 | 28.305 | 7.706 |
| first | conservative9 | 125 | 45.593 | 56.950 | 37.422 |
| third | center | 377 | 14.336 | 25.190 | 5.777 |
| third | conservative9 | 243 | 24.314 | 26.212 | 22.847 |
| overhead | center | 593 | 9.996 | 10.710 | 4.581 |
| overhead | conservative9 | 228 | 25.075 | 33.573 | 23.424 |

## Final default window callback pacing

Same window protocol, separately verified current executable. These are not the paired CPU-only experiment measurements.

| View | Sampling | Frames | Interval p50 ms | Interval p95 ms | CPU p50 ms |
| --- | --- | ---: | ---: | ---: | ---: |
| first | center | 327 | 16.429 | 26.961 | 7.231 |
| first | conservative9 | 126 | 42.642 | 66.469 | 33.748 |
| third | center | 293 | 17.692 | 33.846 | 8.856 |
| third | conservative9 | 249 | 22.710 | 29.392 | 20.755 |
| overhead | center | 589 | 9.962 | 12.771 | 4.292 |
| overhead | conservative9 | 255 | 22.202 | 32.858 | 20.536 |

Rejected deferred-hit/direction-cache variants and their negative measurements remain separate local evidence. No pilot is pooled with final measurements. Raw JSON/CSV, binaries and generated media are excluded from Git.
