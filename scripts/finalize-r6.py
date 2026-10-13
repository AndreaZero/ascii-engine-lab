"""Validate R6 evidence and capture provenance; run after runtime checks/video."""
from pathlib import Path
import hashlib
import json
import re

root = Path(__file__).resolve().parent.parent
out = root / 'evidence/r6'

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

baseline = out / 'baseline'
preserved = read(baseline / 'hashes.json')
for entry in preserved:
    assert sha(baseline / entry['path']) == entry['sha256'], entry['path']
unchanged = ['core.rs', 'camera.rs', 'budget.rs', 'gpu.rs', 'glyph.wgsl']
for name in unchanged:
    assert sha(baseline / 'src' / name) == sha(root / 'crates/ascii-render/src' / name), name
for name in ['actor.rs', 'world.rs']:
    assert sha(baseline / 'src' / name) == sha(root / 'src' / name), name
old_frame = (baseline / 'src/frame.rs').read_text().split('#[cfg(test)]')[0]
new_frame = (root / 'crates/ascii-render/src/frame.rs').read_text().split('#[cfg(test)]')[0]
assert old_frame == new_frame, 'frame runtime extraction changed'

pngs = []
for backend in ['vulkan', 'dx12']:
    files = sorted((out / f'lab-{backend}').glob('*.png'))
    assert len(files) == 12
    for path in files:
        assert sha(path) == sha(root / f'evidence/r5/gpu-{backend}' / path.name), path
        pngs.append({'path': str(path.relative_to(root)), 'sha256': sha(path), 'same_as_r5': True})

modes = ['first', 'third', 'overhead']
changed = []
for mode in modes:
    for frame in ['0000', '0119']:
        a = out / f'room-vulkan/{mode}/frame-{frame}.png'
        b = out / f'room-dx12/{mode}/frame-{frame}.png'
        c = out / f'room-changed/{mode}/frame-{frame}.png'
        assert sha(a) == sha(b), str(a)
        assert sha(a) != sha(c), str(c)
        changed.append({'camera': mode, 'frame': int(frame), 'backend_pixels_identical': True, 'data_edit_pixels_differ': True})

measurements = {}
for name in ['room-vulkan', 'room-dx12', 'room-changed']:
    report = read(out / name / 'run.json')
    assert report['objects'] == 7 and report['grid'] == [160, 90] and report['workers'] == 8
    expected = root / 'clients/material-room/assets/material-room.json' if name != 'room-changed' else out / 'changed-scene.json'
    assert report['input_scene'] == read(expected)
    for run in report['runs']:
        assert run['static_changed_cells'] == 0 and run['frame_count'] == 120
        assert run['cpu_snapshot_and_compose_ms']['p50'] > 0
    measurements[name] = {r['camera']: {k: r[k] for k in ['cpu_snapshot_and_compose_ms', 'submit_wait_ms']} for r in report['runs']}

tests = (out / 'tests.log').read_text(encoding='utf-8-sig')
test_count = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed', tests))
assert test_count == 95 and 'test result: FAILED' not in tests
dependencies = (out / 'dependencies.txt').read_text(encoding='utf-8-sig')
assert 'ascii-render v' in dependencies and 'ascii-engine-lab v' not in dependencies
source_paths = [root / 'Cargo.toml', root / 'Cargo.lock', root / 'Avvia-materiali.cmd']
for folder in ['src', 'crates', 'clients', 'tests']:
    source_paths.extend(p for p in (root / folder).rglob('*') if p.is_file())
source_paths.extend(root / f'target/release/{name}.exe' for name in ['ascii-engine-lab', 'ascii-material-room'])
source_hashes = [{'path': str(p.relative_to(root)), 'sha256': sha(p)} for p in sorted(set(source_paths))]
manifest = {
    'revision': 'R6 content separation / independent client',
    'baseline_hashes_preserved': len(preserved), 'r5_gpu_pngs_identical': len(pngs),
    'workspace_test_executions': test_count, 'extraction_parity_cases': 360,
    'runtime_source_copied_unchanged': unchanged + ['frame.rs (before tests)'],
    'courtyard_content_unchanged': ['src/actor.rs', 'src/world.rs'],
    'independent_client_direct_dependencies': dependencies,
    'data_edit_and_backend_checks': changed, 'measurements': measurements,
    'checks': read(out / 'runtime-checks.json'),
    'client_final_checks': read(out / 'runtime-checks-client-final.json'),
    'source_and_executables': source_hashes, 'baseline_gpu_comparisons': pngs,
    'scope': 'One existing Windows x64 host; no fresh OS/toolchain installation, no cross-hardware certification. 120 frames/case in a single sequential pass, not a repeated performance comparison.',
    'hardware_provenance': 'GPU/driver recorded afresh in each run.json. CPU identity Ryzen 7 3800X from earlier R2/R3 checks; not independently re-read in R6.',
}
movie = out / 'video/material-room-tre-visuali.mp4'
if movie.exists():
    manifest['video'] = {'path': str(movie.relative_to(root)), 'sha256': sha(movie), 'verification': read(out / 'video/verification.json')}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2), encoding='utf-8')
print(json.dumps({k: manifest[k] for k in ['baseline_hashes_preserved', 'r5_gpu_pngs_identical', 'workspace_test_executions', 'extraction_parity_cases', 'measurements']}, indent=2))
