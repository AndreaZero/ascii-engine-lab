"""Validate captures, matrix, source provenance and retain all performance tails."""
from pathlib import Path
import hashlib
import json
import re
import statistics

root=Path(__file__).resolve().parent.parent
out=root/'evidence/r7'
def read(p):return json.loads(p.read_text(encoding='utf-8-sig'))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
baseline=read(out/'baseline/hashes.json')
for item in baseline:assert sha(out/'baseline'/item['path'])==item['sha256'],item['path']
legacy=[]
for backend in ['vulkan','dx12']:
    for p in (out/f'legacy-{backend}').glob('*.png'):
        assert sha(p)==sha(root/f'evidence/r6/lab-{backend}'/p.name),p
        legacy.append(str(p.relative_to(root)))
assert len(legacy)==24
modes=['first','third','overhead']
room_pairs=0
stone_pairs=0
for mode in modes:
    for f in ['0000','0119']:
        for backend in ['vulkan','dx12']:
            p=out/f'room-{backend}/{mode}/frame-{f}.png'
            assert sha(p)==sha(root/f'evidence/r6/room-{backend}/{mode}'/p.name),p
            room_pairs+=1
    for variant in ['baseline','shape','dry','wet']:
        for f in ['0000','0001']:
            a=out/f'static-vulkan/{variant}/{mode}/frame-{f}.png'
            b=out/f'static-dx12/{variant}/{mode}/frame-{f}.png'
            assert sha(a)==sha(b),(a,b)
            assert sha(a)==sha(out/f'static-vulkan/{variant}/{mode}/frame-0000.png')
            stone_pairs+=1
    assert sha(out/f'static-vulkan/dry/{mode}/frame-0000.png')!=sha(out/f'static-vulkan/wet/{mode}/frame-0000.png')
matrix=[]
for grid in ['economy','balanced','detail']:
    for path in ['static','orbit','light','approach']:
        for mode in modes:
            dry=read(out/f'matrix/{grid}/{path}/dry/{mode}/study.json')
            wet=read(out/f'matrix/{grid}/{path}/wet/{mode}/study.json')
            for d in [dry,wet]:
                assert d['frozen_changed_cells']==0
                assert d['output_pixels']==[1920,1080]
                for f in d['frames']:
                    assert f['coverage']['stone_border_hits']==0
                    assert f['coverage']['stone_cells']>100
                first=d['frames'][0]
                last=d['frames'][-1]
                image_dir=out/f'matrix/{grid}/{path}'/d['variant']/mode
                if path=='static':assert sha(image_dir/'frame-0000.png')==sha(image_dir/f"frame-{d['frame_count']-1:04}.png")
                else:assert sha(image_dir/'frame-0000.png')!=sha(image_dir/f"frame-{d['frame_count']-1:04}.png")
                matched=sum(f['matched'] for f in d['frames'][1:])
                changes=sum(f['glyph_changes'] for f in d['frames'][1:])
                matrix.append({'grid':grid,'path':path,'camera':mode,'variant':d['variant'],'frames':d['frame_count'],'cpu_ms':d['compose_cpu_ms'],'glyph_change_fraction_of_matched':changes/matched if matched else None,'matched_fraction_of_all_slots':matched/((d['frame_count']-1)*d['grid'][0]*d['grid'][1])})
            for a,b in zip(dry['frames'],wet['frames']):
                for key in ['time','eye','target','light','coverage']:assert a[key]==b[key]
cpu=[]
for variant in ['dry','wet']:
    for mode in modes:
        for workers in [1,8]:
            cases=[read(out/f'cpu/r{r}-w{workers}/{variant}/{mode}/study.json') for r in [1,2,3]]
            p50s=[v['compose_cpu_ms']['p50'] for v in cases]
            p95s=[v['compose_cpu_ms']['p95'] for v in cases]
            cpu.append({'variant':variant,'camera':mode,'workers':workers,'p50_runs_ms':p50s,'p95_runs_ms':p95s,'median_p50_ms':statistics.median(p50s),'max_p95_ms':max(p95s),'exploratory_budget_p95_ms':16.7 if workers==8 else None,'eight_worker_budget_pass':max(p95s)<=16.7 if workers==8 else None})
memory=[]
for phase in ['runtime','matrix','cpu','motion']:
    for r in read(out/f'runs-{phase}.json')['runs']:
        assert r['exit_code']==0,r
        memory.append({'case':r['case'],'peak_process_working_set_bytes':r['peak_process_working_set_bytes'],'executable_sha256':r['executable_sha256']})
files=[root/'examples/stone_study.rs',root/'examples/support/stone_geometry.rs',root/'examples/support/stone_surface.rs',root/'crates/ascii-render/src/core.rs',root/'crates/ascii-render/src/frame.rs',root/'crates/ascii-render/src/glyph.wgsl',root/'Cargo.lock',root/'target/release/examples/stone_study.exe',root/'target/release/ascii-engine-lab.exe',root/'target/release/ascii-material-room.exe']
tests=(out/'tests-final.log').read_text(encoding='utf-8-sig')
assert 'test result: FAILED' not in tests
count=sum(int(v) for v in re.findall(r'test result: ok\. (\d+) passed',tests))
movie=out/'video/pietra-asciutta-bagnata-tre-visuali.mp4'
result={'revision':'R7 stone study','baseline_preserved':len(baseline),'legacy_pngs_identical':len(legacy),'room_pngs_identical':room_pairs,'stone_backend_pairs_identical':stone_pairs,'tests_executed':count,'matrix_cases':matrix,'cpu_repeated':cpu,'process_memory':memory,'sources':[{'path':str(p.relative_to(root)),'sha256':sha(p)} for p in files],'video':{'path':str(movie.relative_to(root)),'sha256':sha(movie),'verification':read(out/'video/verification.json')},'scope':'One Windowsx64 host, AMD RX6700XT fresh GPU adapter diagnostics per run, Ryzen7 3800X identified in earlier checks. Memory is process working set, not GPU VRAM. CPU benchmark excludes GPU/counting/IO; counters are matched screen-cell IDs, not full perceptual flicker measurement. Dry/wet geometry shared. No claim of photorealism or cross-hardware certification.'}
(out/'manifest.json').write_text(json.dumps(result,indent=2),encoding='utf-8')
print(json.dumps({k:result[k] for k in ['baseline_preserved','legacy_pngs_identical','room_pngs_identical','stone_backend_pairs_identical','tests_executed','cpu_repeated']},indent=2))
