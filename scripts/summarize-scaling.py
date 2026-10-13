"""Summarize existing runs; never replaces raw per-frame measurements."""
import hashlib
import json
import statistics
from pathlib import Path

root = Path(__file__).resolve().parents[1]
evidence = root / 'evidence/r2/scaling'
groups = {}
processes = {}
mismatches = []
frozen_failures = []
captures = 0
for directory in sorted(evidence.iterdir()):
    report_path = directory / 'benchmark.json'
    if not report_path.exists():
        continue
    report = json.loads(report_path.read_text(encoding='utf-8-sig'))
    variant, size, repeat = directory.name.split('-')
    process = json.loads((directory / 'process.json').read_text(encoding='utf-8-sig'))
    processes.setdefault((variant, size), []).append(process)
    for run in report['runs']:
        key = (size, run['camera'], run['stage'], variant)
        groups.setdefault(key, []).append(run)
        if run['static_repeat_different_cells'] or run['static_history_different_cells']:
            frozen_failures.append({'path': directory.name, 'camera': run['camera'], 'stage': run['stage']})
    if variant != 'r1':
        for capture in directory.glob('*.png'):
            reference = evidence / f'r1-{size}-r1' / capture.name
            captures += 1
            if not reference.exists() or hashlib.sha256(capture.read_bytes()).digest() != hashlib.sha256(reference.read_bytes()).digest():
                mismatches.append(str(capture.relative_to(root)))

records = []
for (size, camera, stage, variant), runs in sorted(groups.items(), key=lambda item: (*item[0][:3], 0 if item[0][3] == 'r1' else int(item[0][3][1:]))):
    record = {'size': size, 'camera': camera, 'stage': stage, 'variant': variant, 'repetitions': len(runs)}
    for metric in ['cpu_generation_ms', 'synchronized_offscreen_frame_ms', 'submit_and_wait_ms']:
        record[metric] = {}
        for quantile in ['p50', 'p95', 'p99']:
            values = [r[metric][quantile] for r in runs]
            record[metric][quantile] = {'median': statistics.median(values), 'min': min(values), 'max': max(values)}
    records.append(record)
utilization = []
for (variant, size), runs in sorted(processes.items(), key=lambda item: (0 if item[0][0] == 'r1' else int(item[0][0][1:]), item[0][1])):
    values = [r['process_cpu_percent_of_logical_capacity'] for r in runs if r['process_cpu_percent_of_logical_capacity'] is not None]
    utilization.append({'variant': variant, 'size': size, 'repetitions': len(runs),
                        'whole_process_logical_cpu_percent_median': statistics.median(values) if values else None})
summary = {'aggregation': 'Median of per-run percentiles; min/max are repeat spread, not confidence intervals.',
           'hardware': 'Ryzen 7 3800X 8 cores / 16 logical CPUs; RX 6700 XT Vulkan',
           'gpu_capture_comparisons': captures, 'gpu_capture_mismatches': mismatches,
           'frozen_failures': frozen_failures, 'runs': records, 'whole_process_utilization': utilization,
           'limits': ['No GPU timestamps or window FPS', 'Worker count is not occupied physical cores',
                      'Whole-process CPU percent includes startup, captures, warm-up, driver and waits',
                      'Local desktop, background activity/frequency/temperature not controlled',
                      'R1 measured before R2; worker order reversed in repetition 2, not a randomized controlled study']}
(evidence / 'summary.json').write_text(json.dumps(summary, indent=2), encoding='utf-8')
lines = ['# R2 — misure raccolte', '', summary['aggregation'], '',
         'Stage completo Cloth. Millisecondi; tre visuali dello stesso mondo. Ogni valore è la mediana dei percentili di ciascuna ripetizione, non il percentile di campioni uniti.', '',
         '| Output | Camera | Variante | Rip. | CPU p50 | CPU p95 | Intervallo p95 fra ripetizioni | Frame sync p95 |',
         '| --- | --- | --- | --- | --- | --- | --- | --- |']
for r in records:
    if r['stage'] != 'cloth':
        continue
    cpu = r['cpu_generation_ms']
    sync = r['synchronized_offscreen_frame_ms']['p95']['median']
    lines.append(f"| {r['size']} | {r['camera']} | {r['variant']} | {r['repetitions']} | {cpu['p50']['median']:.2f} | {cpu['p95']['median']:.2f} | {cpu['p95']['min']:.2f}–{cpu['p95']['max']:.2f} | {sync:.2f} |")
lines += ['', f'Confronti catture GPU con R1: {captures}; diverse: {len(mismatches)}. Test statici falliti: {len(frozen_failures)}.', '',
          'Percentuale CPU dell’intero processo sul tempo logico disponibile (16 processori logici); non saturazione dei core fisici o misura del solo compositore.', '',
          '| Variante | Output | CPU processo mediana |', '| --- | --- | --- |']
for u in utilization:
    value = u['whole_process_logical_cpu_percent_median']
    lines.append(f"| {u['variant']} | {u['size']} | {value:.1f}% |" if value is not None else f"| {u['variant']} | {u['size']} | non disponibile |")
lines += ['', 'Limiti: ' + '; '.join(summary['limits']) + '.', '']
(evidence / 'summary.md').write_text('\n'.join(lines), encoding='utf-8')
print(json.dumps({'runs': len(records), 'capture_comparisons': captures, 'capture_mismatches': len(mismatches), 'frozen_failures': len(frozen_failures)}))
