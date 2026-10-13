"""Check R7 preservation, matched R8 comparisons and retain measured tails."""
from pathlib import Path
import hashlib
import json
import re
import statistics

root = Path(__file__).resolve().parent.parent
out = root / 'evidence/r8'
def read(p): return json.loads(p.read_text(encoding='utf-8-sig'))
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
baseline = read(out/'baseline/hashes.json')
for item in baseline:
    assert sha(out/'baseline'/item['path']) == item['sha256'], item['path']
unchanged = ['core.rs','frame.rs','glyph.wgsl']
for name in unchanged:
    assert sha(root/'crates/ascii-render/src'/name) == sha(out/'baseline/src'/name)
assert sha(root/'examples/support/stone_geometry.rs') == sha(out/'baseline/examples/support/stone_geometry.rs')
modes = ['first','third','overhead']
old_pngs = 0
backend_pairs = 0
for mode in modes:
    for variant in ['baseline','shape','dry','wet-r7','wet']:
        for frame in ['0000','0001']:
            a = out/f'static-vulkan/{variant}/{mode}/frame-{frame}.png'
            b = out/f'static-dx12/{variant}/{mode}/frame-{frame}.png'
            assert sha(a) == sha(b)
            assert sha(a) == sha(a.parent/'frame-0000.png')
            backend_pairs += 1
            if variant != 'wet':
                prior = 'wet' if variant == 'wet-r7' else variant
                for backend in ['vulkan','dx12']:
                    new = out/f'static-{backend}/{variant}/{mode}/frame-{frame}.png'
                    old = root/f'evidence/r7/static-{backend}/{prior}/{mode}/frame-{frame}.png'
                    assert sha(new) == sha(old), new
                    old_pngs += 1

def matched(a,b):
    assert a['frame_count'] == b['frame_count']
    assert a['grid'] == b['grid'] and a['output_pixels'] == b['output_pixels'] == [1920,1080]
    assert a['frozen_changed_cells'] == b['frozen_changed_cells'] == 0
    for fa,fb in zip(a['frames'],b['frames']):
        for key in ['eye','target','light','time','rgb_id_hash']: assert fa[key] == fb[key],key
        for key in ['stone_cells','stone_border_hits','stone_bounds']: assert fa['coverage'][key] == fb['coverage'][key],key
        assert fa['coverage']['stone_cells'] > 100 and fa['coverage']['stone_border_hits'] == 0
        for group in ['total','geometric_normal_groups']:
            ga = fa['coverage']['material_metrics'][group]
            gb = fb['coverage']['material_metrics'][group]
            if group == 'total': ga,gb = {'total':ga},{'total':gb}
            for name, va in ga.items():
                vb = gb[name]
                if va is None: assert vb is None; continue
                for key in ['cells','blank_cells','nonfinite_rgba_cells','invalid_glyph_cells']: assert va[key] == vb[key]
                assert va['invalid_glyph_cells'] == va['nonfinite_rgba_cells'] == 0
                assert va['color']['mean_linear_rgb_clamped'] == vb['color']['mean_linear_rgb_clamped']

matrix = []
for grid in ['economy','balanced','detail']:
    for path in ['static','orbit','light','approach']:
        for mode in modes:
            pair = [read(out/f'matrix/{grid}/{path}/{v}/{mode}/study.json') for v in ['wet-r7','wet']]
            matched(*pair)
            for d in pair:
                matches = sum(f['matched'] for f in d['frames'][1:])
                changes = sum(f['glyph_changes'] for f in d['frames'][1:])
                directory = out/f"matrix/{grid}/{path}/{d['variant']}/{mode}"
                same = sha(directory/'frame-0000.png') == sha(directory/f"frame-{d['frame_count']-1:04}.png")
                assert same == (path == 'static')
                matrix.append({'grid':grid,'path':path,'camera':mode,'variant':d['variant'],
                    'frames':d['frame_count'],'cpu_ms':d['compose_cpu_ms'],'submit_wait_ms':d['submit_wait_ms'],
                    'glyph_change_fraction_of_matched':changes/matches if matches else None,
                    'matched_fraction_of_all_slots':matches/((d['frame_count']-1)*d['grid'][0]*d['grid'][1])})

readability = []
motion_cases = []
for mode in modes:
    cases = {v:read(out/f'motion/{v}/{mode}/study.json') for v in ['dry','wet-r7','wet']}
    matched(cases['wet-r7'],cases['wet'])
    for variant,d in cases.items():
        motion_cases.append({'variant':variant,'camera':mode,'cpu_ms':d['compose_cpu_ms'],
            'submit_wait_ms':d['submit_wait_ms'],'frames':d['frame_count']})
    for variant,prior in [('dry','dry'),('wet-r7','wet')]:
        for frame in range(180):
            a = out/f'motion/{variant}/{mode}/frame-{frame:04}.png'
            b = root/f'evidence/r7/motion/{prior}/{mode}/frame-{frame:04}.png'
            assert sha(a) == sha(b), a
            old_pngs += 1
    for group in ['total','+X','-X','+Y','-Y','+Z','-Z']:
        record = {'camera':mode,'group':group,'variants':{}}
        for variant,d in cases.items():
            buckets = [f['coverage']['material_metrics']['total'] if group == 'total' else
                f['coverage']['material_metrics']['geometric_normal_groups'][group] for f in d['frames']]
            buckets = [b for b in buckets if b]
            if not buckets: record['variants'][variant] = None; continue
            count = sum(b['cells'] for b in buckets)
            record['variants'][variant] = {
                'cell_samples':count,
                'blank_fraction':sum(b['blank_cells'] for b in buckets)/count,
                'sparse_nonblank_fraction':sum(b['sparse_nonblank_cells'] for b in buckets)/count,
                'atlas_ink':sum(b['mean_atlas_ink']*b['cells'] for b in buckets)/count,
                'linear_energy_proxy':sum(b['color']['mean_channel_energy_proxy']*b['cells'] for b in buckets)/count}
        readability.append(record)

cpu = []
for variant in ['wet-r7','wet']:
    for mode in modes:
        for workers in [1,8]:
            cases = [read(out/f'cpu/r{r}-w{workers}/{variant}/{mode}/study.json') for r in [1,2,3]]
            medians = [d['compose_cpu_ms']['p50'] for d in cases]
            p95s = [d['compose_cpu_ms']['p95'] for d in cases]
            cpu.append({'variant':variant,'camera':mode,'workers':workers,'p50_runs_ms':medians,'p95_runs_ms':p95s,
                'median_p50_ms':statistics.median(medians),'max_p95_ms':max(p95s),
                'eight_worker_budget_pass':max(p95s)<=16.7 if workers==8 else None})
controls = []
for repeat in [1,2]:
    for grid,path in [('economy','static'),('economy','orbit'),('balanced','approach'),('detail','showcase')]:
        for mode in modes:
            pair = [read(out/f'control/r{repeat}/{grid}/{path}/{v}/{mode}/study.json') for v in ['wet-r7','wet']]
            matched(*pair)
            for d in pair:
                controls.append({'repeat':repeat,'grid':grid,'path':path,'camera':mode,'variant':d['variant'],
                    'frames':d['frame_count'],'cpu_ms':d['compose_cpu_ms'],
                    'submit_wait_ms':d['submit_wait_ms'],'budget_pass':d['compose_cpu_ms']['p95']<=16.7})
memory = []
for phase in ['runtime','matrix','cpu','motion','control']:
    for r in read(out/f'runs-{phase}.json')['runs']:
        assert r['exit_code'] == 0
        memory.append({'case':r['case'],'peak_process_working_set_bytes':r['peak_process_working_set_bytes'],
            'executable_sha256':r['executable_sha256']})
videos = []
for directory,filename in [('comparison','bagnato-prima-dopo-tre-visuali.mp4'),('materials','pietra-asciutta-bagnata-r8.mp4')]:
    p = out/'video'/directory/filename
    videos.append({'path':str(p.relative_to(root)),'sha256':sha(p),'verification':read(p.parent/'verification.json')})
files = [root/p for p in ['examples/stone_study.rs','examples/support/stone_geometry.rs','examples/support/stone_surface.rs',
    'examples/support/stone_metrics.rs','crates/ascii-render/src/core.rs','crates/ascii-render/src/frame.rs',
    'crates/ascii-render/src/glyph.wgsl','Cargo.lock','target/release/examples/stone_study.exe']]
tests = (out/'tests.log').read_text(encoding='utf-8-sig')
assert 'test result: FAILED' not in tests
result = {'revision':'R8 wet glyph readability','baseline_preserved':len(baseline),'r7_pngs_identical':old_pngs,
    'stone_backend_pairs_identical':backend_pairs,'tests_executed':sum(map(int,re.findall(r'test result: ok\. (\d+) passed',tests))),
    'matrix_cases':matrix,'cpu_repeated':cpu,'control_cases':controls,'motion_cases':motion_cases,
    'readability':readability,'process_memory':memory,
    'sources':[{'path':str(p.relative_to(root)),'sha256':sha(p)} for p in files],'videos':videos,
    'scope':'Same R7 geometry, light, camera, shader and per-cell RGB. R8 changes low-light glyph ink only. Atlas coverage/linear energy are proxies, not perceived realism or pixel luminance. One Windows host. CPU timer excludes audit, capture and IO. No GPU timestamp or FPS claim.'}
(out/'manifest.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps({k:result[k] for k in ['baseline_preserved','r7_pngs_identical','stone_backend_pairs_identical','tests_executed','cpu_repeated']},indent=2))
