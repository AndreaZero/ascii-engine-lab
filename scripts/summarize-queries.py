"""R4 paired CPU data, historical hashes and render parity. No FPS conversion."""
import hashlib
import json
import statistics
from pathlib import Path

root = Path(__file__).resolve().parents[1]
out = root / 'evidence/r4'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
data = json.loads((out/'cpu-paired/benchmark.json').read_text())
groups = {}
for r in data['runs']:
    groups.setdefault((r['workers'],r['camera'],r['variant']),[]).append(r)
rows = []
for (workers,camera,variant), runs in sorted(groups.items()):
    rows.append(dict(workers=workers,camera=camera,variant=variant,repetitions=len(runs),
                     p50_ms=statistics.median(r['p50_ms'] for r in runs),
                     p95_ms=statistics.median(r['p95_ms'] for r in runs),
                     min_p95_ms=min(r['p95_ms'] for r in runs),
                     max_p95_ms=max(r['p95_ms'] for r in runs)))
base = out/'baseline'
manifest = json.loads((base/'r3-manifest.json').read_text())
checks = {name: sha(base/name)==value for name,value in manifest['source_sha256'].items() if (base/name).is_file()}
checks['ascii-engine-lab-r3.exe'] = sha(base/'ascii-engine-lab-r3.exe')==manifest['executable_sha256']
assert len(checks)==13 and all(checks.values()), checks
captures=[]
for backend in ('vulkan','dx12'):
    paths=sorted((out/f'gpu-{backend}').glob('*.png'))
    assert len(paths)==12,(backend,len(paths))
    for p in paths:
        reference=root/'evidence/r3/vulkan'/p.name
        captures.append(dict(backend=backend,file=p.name,sha256=sha(p),identical_r3=sha(p)==sha(reference)))
assert all(c['identical_r3'] for c in captures), captures
before=json.loads((out/'profile-before/profile.json').read_text())
after=json.loads((out/'profile-after/profile.json').read_text())
assert [(r['camera'],r['group'],r['queries'],r['positive_results']) for r in before['groups']]==[(r['camera'],r['group'],r['queries'],r['positive_results']) for r in after['groups']]
result=dict(cpu=rows,baseline_checks=checks,captures=captures,query_counts_equal=True,
            query_groups=before['groups'],limits=data['limits'],
            percentile_method='Median of per-run percentiles; min/max repeat spread is not a confidence interval')
(out/'summary.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
lines=['# R4 — risultati delle query','',result['percentile_method'],'',
       'CPU Cloth 240×135, 120 frame per variante/ripetizione dopo 12 warm-up; tre ripetizioni abbinate per frame. Nessun tempo GPU/FPS.', '',
       '| Worker | Camera | Variante | CPU p50 ms | CPU p95 ms | Intervallo p95 ms |',
       '| --- | --- | --- | --- | --- | --- |']
for r in rows:
    lines.append(f"| {r['workers']} | {r['camera']} | {r['variant']} | {r['p50_ms']:.2f} | {r['p95_ms']:.2f} | {r['min_p95_ms']:.2f}–{r['max_p95_ms']:.2f} |")
lines+=['',f"Baseline R3: {len(checks)} hash preservati. Catture: {len(captures)}/{len(captures)} identiche R3. Conteggi/risultati del replay invariati.",
        '','I replay prima/dopo avvengono in sessioni distinte; non attribuire il loro intero scarto alla modifica. Il confronto CPU abbinato sopra isola la variante nello stesso eseguibile.', '']
(out/'summary.md').write_text('\n'.join(lines),encoding='utf-8')
print('\n'.join(lines))
