"""Validate R10 evidence and summarize existing runs; never invent missing data."""
from pathlib import Path
import hashlib
import json
import re
import statistics

root = Path(__file__).resolve().parent.parent
base = root/'evidence/r10'
read = lambda p: json.loads(p.read_text(encoding='utf-8-sig'))
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
checks = {}
for name in ['metal', 'stone']:
    originals = sorted((base/f'baseline/{name}').rglob('*.png'))
    assert originals, name
    for p in originals:
        other = base/f'legacy-{name}'/p.relative_to(base/f'baseline/{name}')
        assert sha(p) == sha(other), (p, other)
    checks[f'{name}_legacy_png_pairs'] = len(originals)
images = sorted((base/'static-vulkan').rglob('*.png'))
assert images
for p in images:
    assert sha(p) == sha(base/'static-dx12'/p.relative_to(base/'static-vulkan'))
checks['backend_png_pairs'] = len(images)
for mode in ['first', 'third', 'overhead']:
    a = read(base/f'static-vulkan/off/{mode}/study.json')
    b = read(base/f'static-vulkan/on/{mode}/study.json')
    assert a['frames'][0]['rgb_id_hash'] != b['frames'][0]['rgb_id_hash']
    assert a['frames'][0]['coverage']['counts'] == b['frames'][0]['coverage']['counts']
cpu = []
for variant in ['off', 'on']:
    for camera in ['first', 'third', 'overhead']:
        for workers in [1, 8]:
            runs = [read(base/f'cpu/r{r}-w{workers}/{variant}/{camera}/study.json') for r in range(1,4)]
            cpu.append({'variant':variant,'camera':camera,'workers':workers,
                'median_p50_ms':statistics.median(r['compose_cpu_ms']['p50'] for r in runs),
                'p50_range_ms':[min(r['compose_cpu_ms']['p50'] for r in runs),max(r['compose_cpu_ms']['p50'] for r in runs)],
                'worst_p95_ms':max(r['compose_cpu_ms']['p95'] for r in runs),
                'p95_budget_exceeded_runs':sum(r['compose_cpu_ms']['p95']>16.7 for r in runs)})
phases = {}
for phase in ['matrix','motion']:
    reports = [read(p) for p in sorted((base/phase).rglob('study.json'))]
    assert reports
    for r in reports:
        assert r['frozen_changed_cells'] == 0
        assert len(r['frames']) == r['frame_count']
        for f in r['frames']:
            assert all(v > 0 for v in f['coverage']['counts'])
            assert not any(f['coverage']['border_hits'])
            if r['path'] == 'static':
                assert f['changed_cells'] == 0
    changes = [f['glyph_changes']/f['matched'] for r in reports for f in r['frames'] if f['matched']]
    phases[phase] = {'cases':len(reports),'frames':sum(r['frame_count'] for r in reports),
        'p95_budget_exceeded_cases':sum(r['compose_cpu_ms']['p95']>16.7 for r in reports),
        'worst_p95_ms':max(r['compose_cpu_ms']['p95'] for r in reports),
        'mean_matched_glyph_change_fraction':statistics.mean(changes),
        'max_matched_glyph_change_fraction':max(changes),
        'all_subjects_present_no_border_hits':True}
tests = (base/'tests.log').read_text(encoding='utf-8-sig')
counts = [int(n) for n in re.findall(r'test result: ok\. (\d+) passed', tests)]
assert counts and 'test result: FAILED' not in tests
memory = {}
for phase in ['runtime','matrix','cpu','motion']:
    runs = read(base/f'runs-{phase}.json')['runs']
    assert all(r['exit_code']==0 for r in runs)
    values = [r['peak_process_working_set_bytes']/1048576 for r in runs if r['peak_process_working_set_bytes'] is not None]
    memory[phase] = [min(values),max(values)] if values else None
video = read(base/'video/verification.json')
assert video['full_decode'] == 'PASS'
result = {'checks':checks,'test_pass_executions':sum(counts),'cpu':cpu,'phases':phases,
    'peak_process_working_set_MiB':memory,'video':video,
    'scope':'CPU composition only. Median of 3 p50 values, not pooled percentiles. Working set is not VRAM. History counters are not perceptual flicker scores. One host, not universal FPS.'}
(base/'summary.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result,indent=2))
