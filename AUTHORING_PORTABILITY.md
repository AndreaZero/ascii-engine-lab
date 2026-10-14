# R15 — practical authoring and portable Windows package

14 October 2026. Baseline: R14 `b69bb1696f31c5b8eeb43de32e1f861a03a18117`.
This tranche makes the existing Material Room subset editable, savable and
reopenable as a self-contained project. It changes the independent client and
its tools, not the renderer's geometry, query order, ties, shading or sampling.
The courtyard remains the other client. No Godot gameplay, mesh-to-ASCII path,
new material study or universal editor is introduced.

## Contract and usable workflow

Project v1 embeds a validated scene v2, update sequence v1 and three camera
control records. No scene file or compilation-machine asset path is retained.
The compatible standalone v1/v2 scene and R14 update flags remain available;
`--project` is exclusive with those inputs. The no-argument client now uses
its embedded v1 sample, so a moved executable cannot silently read its build
checkout. Fonts and shaders were already embedded.

The `author` companion creates, imports, inspects and edits projects without
initializing a GPU. It supports insertion, duplication, removal, translation,
material assignment and a bounded add/remove/replace JSON-pointer patch.
A patch can change materials, geometry, base cameras and future events in one
validated candidate. Every candidate preflights the entire future timeline;
an invalid later event rejects the whole edit. Duplicating reserves IDs used by
future events too. Native translation preserves authored motion; this differs
from R14 runtime absolute transforms, which replace it.

The window provides object selection by ID, 0.1-unit world-axis translations,
duplication/removal, three independently retained camera slots and explicit
save. Edits change the authored starting scene and pause preview at tick zero.
F5 restarts preview; this is not baking the elapsed runtime into a savegame.
Material, shape, projection and timeline editing use the companion or project
data. There is no mouse picking, gizmo, general asset browser or undo stack.

`Ctrl+S` saves; dirty reload is rejected. Closing a dirty project, including
the timed-window path, asks the user to save or explicitly discard with
`Shift+Esc`. Control shortcuts are consumed before navigation, preventing
Ctrl+S/Ctrl+D from moving the camera. Scripted coverage/orbit is transient
headless capture state; it is not a saved interactive project camera.

See the [practical guide](clients/material-room/AUTHORING.md),
[client reference](clients/material-room/README.md) and
[package guide](docs/PORTABLE_PACKAGE.md). `Avvia-authoring.cmd` rebuilds both
client programs and creates/reopens an ignored local working project.

## Camera and disk semantics

Persisting `Camera::pose()` as new world-space eye/target points changes
rounding and can lose a component near the supported coordinate bounds.
R15 instead preserves the authored base view and exact f32 orbit/offset/zoom
controls. Tests compare actual center and subcell ray bits after text roundtrip,
including signed zero, near-vertical views and large translations. A bank
rejects replacement of its base view; edit the scene camera and rebuild it.

Projects are at most 2 MiB including their formatted representation; patches
are at most 1 MiB/128 operations, with pointers limited to 512 bytes/32 components
inside scene, updates and camera controls. Existing scene/event resource limits
still apply. Unknown/duplicate fields and nonfinite/out-of-range controls fail.

New files never overwrite existing destinations. Existing saves compare exact
bytes against the open/save baseline, take a cooperative sidecar lock, write
and sync a same-directory temporary file, then publish by rename/replacement.
Windows uses MoveFileExW; an unsuccessful replacement retains the old file and
dirty in-memory work. Symlink/directory destinations are rejected. Stale locks
are not stolen automatically.

This is not a filesystem compare-and-swap against non-cooperating writers:
another editor can race the final comparison and replacement. Close competing
editors; there is no automatic merge/force overwrite or crash-recovery journal.
Power-loss durability is not certified. `author save-as` copies the document
on disk; it cannot retrieve unsaved state from a separate running window.
Runtime namespaces/generations/history epochs are intentionally not persisted.

## Verification

The complete locked workspace/all-target suite passes **396 test executions**,
including historical fixture copies. New coverage includes 12 authoring tests,
six camera tests and two host tests. The CLI test launches the real companion;
other tests compare three-view cells, future replay, immutable originals,
atomic failure, simultaneous file sessions and a Windows replacement denied
by an open file without delete sharing. Fmt and clippy with warnings denied
pass; release workspace binaries/examples build successfully.

The first workspace run stopped on Access Denied for atomic rename inside the
sandbox's temporary directory. The exact focused test and complete suite pass
outside that sandbox. The release CLI workflow also succeeds inside the normal
workspace sandbox. The failed log is retained; atomic publication was not
weakened to make that environment pass. Cargo's existing incremental hard-link
warning falls back to copying and is not a compiler diagnostic in this change.

The release authoring acceptance runs ten commands: create, expected no-clobber
failure, duplicate, translate, assign material, apply a multi-field patch,
expected future-event failure, inspect, save-as and validate. Both failures
leave bytes unchanged. The resulting project has nine base objects, modified
material/shape/camera data and two future transactions; reopen retains the
same authored source. No core patch is needed to produce that variation.

Rendering preserves 48 legacy PNGs, 540 Vulkan/DX12 pairs across 1,080 new
project frames and six reopened images. Six four-second native client windows
complete both transactions at 240×135/1920×1080/eight workers; the courtyard
also opens successfully. Center callback p95 is 13.475–14.892 ms in these
single runs; Conservative9 is 30.296–39.944 ms. These are callback wall times,
not display FPS or a controlled R14 speed comparison. The new object's central
coverage is nonzero without border clipping in every view before its removal;
sampled images retain dark materials, regular patterns and stepped events.

Rendering, window and package results are recorded in the compact
[evidence summary](evidence/r15/summary.md). Raw logs, JSON, binaries, PNGs and
ZIPs stay local and are not published as source evidence.

## Portability boundary and remaining gate

The package contains Windows x64 MSVC binaries for both clients and `author`,
an embedded starter project, relative launchers, a PowerShell probe, license
notices and a hashed inventory/ZIP. The launcher creates a separate working
copy and never overwrites an existing one. The independent Python checker
verifies directory/ZIP bytes, relative paths, AMD64 PE/import tables and notices.
The runtime probe does not require Rust, Cargo, Python, Node or a checkout.

These binaries still import VCRUNTIME140.dll and Universal CRT. They are
prerequisites, not bundled or silently installed. The probe records OS/CPU/GPU,
requested backend/profile/workers, loader/init/timeout failures and native
reports. It does not substitute another backend or reduce density on failure.
License inventory includes 117 locked Windows-target dependencies, including
build dependencies conservatively; 11 archives lacking a standalone license
file retain their full source notices and declared standard license text.

The 556-file package passes independent directory/ZIP integrity checks and is
run outside the checkout in paths with spaces and Unicode. Fifteen probe cases
pass with PowerShell 7 and fifteen with Windows PowerShell 5.1, covering both
clients in all three views and six native windows per probe. The latter run
collects complete OS/CPU/GPU information outside the sandbox. Two launcher
runs verify first creation and reopening of an edited working copy.
The first PS5.1 attempt exposed an empty PSScriptRoot parameter default;
the default is now resolved in the script body and the full probe passes.

All current runtime evidence is from the same Windows x64 Ryzen 7 3800X /
Radeon RX 6700 XT host. Relocation and an empty working directory demonstrate
packaging independence, not portability to a second physical machine. No
second-machine execution has been supplied. Linux/macOS/ARM, other vendors,
minimum hardware and a guaranteed frame rate remain unverified.

The practical authoring subset is delivered; the combined creation/portability
gate remains **partial** until a real second-host result exists. General
geometry/motion/cost remains open, including R12 sample gaps/foreground dilation
and R13's rejected acceleration integration. Next work should consume the
portable probe on another physical host and address a demonstrated defect or
declare a bounded BLACKWATER V1 workload. A new showcase is not a substitute.

## Reproduce

```powershell
cargo fmt --all -- --check
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release --workspace --bins --examples
python scripts/verify-r15.py --phase author
python scripts/verify-r15.py --phase legacy
python scripts/verify-r15.py --phase views
python scripts/verify-r15.py --phase windows
python scripts/verify-r15.py --phase summary
```

Use a working Python runtime; this host's WindowsApps alias is unusable.
The author phase requires fresh output project names. Legacy comparisons need
the frozen local R14 binaries; they are intentionally not in Git. Package
assembly, hash checking and a second-host collection procedure are in the
package guide. Run heavy work serially at fixed density.
