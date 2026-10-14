# R15 evidence summary

Baseline R14: `b69bb1696f31c5b8eeb43de32e1f861a03a18117`.
Windows x64, Ryzen 7 3800X, Radeon RX 6700 XT; one physical host, ordinary
desktop load. Heavy jobs serialized. No renderer/core source changed in R15.

## Checks

- Locked workspace/all-target tests: **396 executions passed**, including
  historical copies. New coverage: 12 authoring, six camera and two host tests.
- Fmt and clippy `--locked --workspace --all-targets -- -D warnings`: PASS.
- Release workspace binaries and examples: PASS.
- Ten release author commands: PASS, including two expected rejections with
  unchanged file bytes. Nine base objects/two events, saved/reopened project.
- **48 legacy PNGs identical** to frozen R14: v1, v2 and R14 dynamic data,
  three views, Center/Conservative9, plus twelve courtyard stage/view images.
- **1,080 project sequence PNGs**, 90 frames per view/policy/backend;
  **540 Vulkan/DX12 pairs identical**. Three views, 240×135, 1920×1080, eight
  workers. Both commits occur at ticks 30/60 and clear history in every run.
- **Six reopened images identical** to the original saved project sequence.
- Six native Material Room windows, four requested seconds, all PASS and both
  transactions completed; independent courtyard window PASS, 195 frames.

## Native-window measurements

CPU total includes update, snapshot and composition; excludes initialization,
full authoring validation, saving and GPU work. First twelve frames excluded.
Intervals are callback wall times, not display FPS or GPU timestamps. These
are single smoke runs, not a controlled performance comparison with R14.

| View | Sampling | Frames | CPU p50 / p95 ms | Callback p50 / p95 ms |
| --- | --- | ---: | ---: | ---: |
| First | Center | 393 | 5.343 / 7.078 | 8.041 / 14.260 |
| Third | Center | 374 | 5.856 / 10.436 | 9.577 / 14.892 |
| Overhead | Center | 391 | 6.503 / 8.685 | 9.185 / 13.475 |
| First | Conservative9 | 173 | 21.258 / 27.308 | 22.679 / 30.296 |
| Third | Conservative9 | 147 | 24.053 / 36.353 | 26.706 / 39.944 |
| Overhead | Conservative9 | 148 | 23.442 / 35.391 | 25.209 / 37.432 |

Conservative9 still exceeds the exploratory 16.7 ms budget. No speedup or
minimum-hardware/60 FPS promise follows from authoring functionality.

## Visual review and limits

Inspected Center GPU frame 30 in all three views: authored scene remains
recognizable as analytic columns, lintel, moving panel and samples; the new
metal object is dark but has visible cells. Its central-ray coverage is
501/207/140 cells initially and 448/193/142 after transformation, with zero
border cells in each view; it is absent after tick 60. The floor reaches the
frame border intentionally. This is a sampled screenshot review, not live
desktop inspection or exhaustive perceptual evaluation of the keyboard UI.
Dark surfaces, repetitive patterns and stepped event motion remain. R15 does
not claim improved material appearance or exact geometric coverage.

## Provenance and failures retained

Package integrity: 556 payload files, 117 dependency-license entries, AMD64
executables and expected VCRUNTIME140/UCRT imports. Candidate directory/ZIP
and relocated copies pass the independent checker. Extracted outside the
repository into a path with spaces and Unicode; every probe process uses an
empty working directory. PowerShell 7 completes 15 cases (validation, two
clients × three headless views, six windows). The launcher creates its working
copy and reopens an edited copy without overwriting it, verified in two actual
Vulkan window launches.

Windows PowerShell 5.1 initially failed before cases because its parameter
default did not populate PSScriptRoot. Resolving the default inside the script
body fixes it; three minimal missing-manifest diagnostics and a regenerated
package's complete **15-case PS5.1 probe pass**. The final PS5.1 run collects
Windows 11 Pro 10.0.26300, eight-core/sixteen-thread Ryzen 7 3800X and RX 6700 XT
driver 32.0.21045.5002, with no inventory errors. PS7's sandbox CIM-denied
inventory is retained separately; the later collection ran outside sandbox.
Both runs remain the same physical host. No second-machine gate is closed.

Package logs: `pack-candidate.log`, `probe-relocated.log`, `check-relocated.json`,
`launcher-first.log`, `launcher-reopen.log`, `probe-powershell51.log` (failure),
`pack-fixed.log`, `probe-powershell51-fixed.log`, `check-fixed.json`.

Release executable SHA-256:

- Material Room: `19aa589139dd2d859621c53559543db7f354503f4bbbf44b9772147a574deff3`
- Author companion: `f87aaddce4224e8cc0712fa5f13e5d4cdeae20cef520f7c6905b11d48ddc682b`
- Courtyard: `37238dbc678fe7ea6e0c3ae2d1fa829941e328e528e838752d152d6379c01608`
- Frozen R14 Material Room: `d7b2a1f27b6d490843f05fc09755f5415e45f969e0826081df55dadeaad3e127`

The first workspace test run stopped at Access Denied for atomic publication
under the sandbox temporary directory. Focused and complete unsandboxed tests
passed; the real release CLI also passed inside the workspace sandbox. No
weaker save path was substituted. Existing Cargo incremental hard-link
warnings use copy fallback. The package collector initially identified eleven
upstream archives without standalone license files; their full source notices
and declared standard license texts are now retained. ZIP timestamps predating
1980 are normalized by Compress-Archive; content hashes remain unchanged.

Raw evidence: `tests-workspace*.log`, `test-save-unsandboxed.log`, `fmt.log`,
`clippy.log`, `build-release.log`, `runs-*.json`, `author-summary.json`,
`summary.json`, `legacy/`, `views/`, `windows/`. Generated media, binaries,
raw host paths/logs and archives remain local and excluded from Git.

See [report](../../AUTHORING_PORTABILITY.md) for contracts and residual gates.
