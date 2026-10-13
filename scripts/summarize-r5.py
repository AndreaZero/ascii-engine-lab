"""Validate retained evidence and summarize cost/temporal limits for R5."""
from pathlib import Path
import hashlib
import json
import statistics

root = Path(__file__).resolve().parents[1]
out = root / 'evidence/r5'
def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

baseline = read(out / 'baseline/hashes.json')
for item in baseline:
    assert sha(out / 'baseline' / item['path']) == item['sha256'], item['path']
original_actor = (out / 'baseline/src/actor.rs').read_text(encoding='utf-8').split('// Independent, frozen implementation')[0].strip()
assert original_actor == (root / 'examples/support/r4_actor.rs').read_text(encoding='utf-8').strip()

cost = []
for workers in (1, 8):
    for mode in ('first', 'third', 'overhead'):
        variants = {}
        for variant in ('baseline', 'current'):
            runs = [read(out / f'cpu/rep{rep}-w{workers}/{variant}/{mode}/study.json') for rep in range(1, 4)]
            assert all(r['frozen_stability']['bitwise_stable'] for r in runs)
            variants[variant] = {
                'median_run_p50_ms': statistics.median(r['cpu_compose_p50_ms'] for r in runs),
                'run_p50_ms': [r['cpu_compose_p50_ms'] for r in runs],
                'run_p95_ms': [r['cpu_compose_p95_ms'] for r in runs],
                'snapshot_median_ms': statistics.median(f['snapshot_cpu_ms'] for r in runs for f in r['frames']),
            }
        cost.append({'workers': workers, 'camera': mode, **variants})

motion = []
for variant in ('baseline', 'current'):
    for mode in ('first', 'third', 'overhead'):
        folder = out / f'motion/{variant}/{mode}'
        data = read(folder / 'study.json')
        assert data['frozen_stability']['bitwise_stable']
        assert len(list(folder.glob('frame-*.png'))) == data['frame_count']
        assert all(f['counts']['border_hits'] == 0 for f in data['frames'])
        frames = data['frames'][1:]
        assert sum(f['changed_cells_from_previous'] for f in frames) > 0
        matched = sum(f['history_matched'] for f in frames)
        occupied = sum(f['counts']['occupied_cells'] for f in frames)
        motion.append({'variant': variant, 'camera': mode, 'frames': data['frame_count'],
                       'static_changed_cells': 0, 'clipped_border_cells': 0,
                       'mean_occupied': statistics.mean(f['counts']['occupied_cells'] for f in frames),
                       'mean_visible': statistics.mean(f['visible'] for f in frames),
                       'matched_fraction_of_occupied': matched / max(occupied, 1),
                       'glyph_changes_on_matched_fraction': sum(f['matched_glyph_changes'] for f in frames) / max(matched, 1),
                       'limits': 'ID at same screen cell only; excludes disocclusion, not a complete flicker metric.'})

backend_pairs = []
for mode in ('first','third','overhead'):
    for stage in ('core','light','water','cloth'):
        name = f'{mode}-{stage}.png'
        a, b = out / 'gpu-vulkan' / name, out / 'gpu-dx12' / name
        assert sha(a) == sha(b), name
        backend_pairs.append(name)
for backend in ('vulkan','dx12'):
    runs = read(out / f'gpu-{backend}/benchmark.json')['runs']
    assert len(runs) == 12
    assert all(r['static_repeat_different_cells'] == 0 and r['static_history_different_cells'] == 0 for r in runs)
runtime = read(out / 'runtime-checks.json')
assert all(r['exit_code'] == 0 for r in runtime)
godot = root.parent / 'ascii-game/project.godot'
assert sha(godot) == '1a50df261ce8ede17001685c048a28064f25049cda38e069e9d96c635c903410'

summary = {'baseline_hashes_preserved': len(baseline), 'derived_actor_matches_source_without_tests': True,
           'godot_project_preserved': True, 'gpu_equal_backend_pairs': backend_pairs,
           'runtime_checks': runtime, 'cpu_comparison': cost, 'motion': motion,
           'comparison_limits': 'Intentional artwork and structural-history changes; not equal-quality speedup. Composition CPU only, no GPU timestamp, no display FPS, one physical host. Three repetitions, blocks alternate order; retained outliers.'}
(out / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
for row in cost:
    print(f"{row['workers']} workers {row['camera']}: R4 {row['baseline']['median_run_p50_ms']:.3f} -> R5 {row['current']['median_run_p50_ms']:.3f} ms; R5 p95 runs {row['current']['run_p95_ms']}")
print(f"Preserved {len(baseline)} baseline hashes; 12 equal Vulkan/DX12 PNG pairs; static checks and motion bounds passed.")
