"""Validate completed R9 evidence; do not run during capture/build/encoding.

Standard library only. Performance threshold misses are reported, not failures.
Missing or inconsistent evidence fails explicitly and writes a FAIL manifest.
"""
from functools import lru_cache
import hashlib
import json
import math
from pathlib import Path
import re
import statistics
import struct
import sys

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / 'evidence/r9'
MODES = ('first', 'third', 'overhead')
FINISHES = ('forged', 'oxidized')
VARIANTS = ('uniform', *FINISHES)
STONE_VARIANTS = ('baseline', 'shape', 'dry', 'wet-r7', 'wet')
GRIDS = {'economy': [120, 68], 'balanced': [160, 90], 'detail': [240, 135]}
PATHS = ('static', 'orbit', 'light', 'approach')
BUDGET_MS = 16.7


def require(condition, message):
    if not condition:
        raise ValueError(str(message))


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


@lru_cache(maxsize=None)
def sha(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(block)
    return digest.hexdigest()


def relative(path):
    return path.relative_to(ROOT).as_posix()


def finite_tree(value, label):
    if isinstance(value, float):
        require(math.isfinite(value), f'{label}: nonfinite number')
    elif isinstance(value, dict):
        for key, item in value.items():
            finite_tree(item, f'{label}/{key}')
    elif isinstance(value, list):
        for item in value:
            finite_tree(item, label)


def bucket_check(bucket, label):
    if bucket is None:
        return
    n = bucket['cells']
    require(n > 0, f'{label}: empty bucket must be null')
    require(bucket['invalid_glyph_cells'] == bucket['nonfinite_rgba_cells'] == 0,
            f'{label}: invalid glyph/RGBA')
    histogram = bucket['glyph_code_histogram']
    require(sum(histogram.values()) == n, f'{label}: histogram count')
    require(all(32 <= int(code) <= 126 and count > 0 for code, count in histogram.items()),
            f'{label}: histogram printable ASCII/count')
    require(0 <= bucket['blank_cells'] <= n and 0 <= bucket['sparse_nonblank_cells'] <= n,
            f'{label}: blank/sparse count')
    require(bucket['blank_cells'] + bucket['sparse_nonblank_cells'] <= n,
            f'{label}: overlapping blank/sparse counts')
    require(0 <= bucket['mean_atlas_ink'] <= 1, f'{label}: atlas occupancy')
    color = bucket['color']
    require(color is not None and color['samples'] == n, f'{label}: missing color samples')
    require(all(0 <= c <= 1 for c in color['mean_linear_rgb_clamped']), f'{label}: RGB range')


def study(directory, variant, mode, path, frames, grid, backend, workers=None, kind='metal'):
    data = read(directory / 'study.json')
    label = relative(directory)
    finite_tree(data, label)
    for key, expected in [('variant', variant), ('camera', mode), ('path', path),
                          ('frame_count', frames), ('fps_simulated', 30), ('grid', grid),
                          ('output_pixels', [1920, 1080]), ('frozen_changed_cells', 0),
                          ('warmup', 12), ('stage', 'light')]:
        require(data[key] == expected, f'{label}: {key} != {expected}')
    if workers is not None:
        require(data['workers'] == workers, f'{label}: workers')
    require(len(data['frames']) == frames, f'{label}: truncated frame records')
    if backend is None:
        require(data['gpu'] is None and data['submit_wait_ms'] is None, f'{label}: CPU-only scope')
    else:
        require(data['gpu']['selection']['backend'] == backend, f'{label}: requested backend')
        require(data['gpu']['adapter']['backend'].lower() == backend, f'{label}: actual backend')
    for metric in ['compose_cpu_ms', 'submit_wait_ms']:
        dist = data[metric]
        if dist is not None:
            require(0 <= dist['p50'] <= dist['p95'] <= dist['p99'], f'{label}: {metric} percentiles')
    for index, frame in enumerate(data['frames']):
        require(frame['frame'] == index and abs(frame['time'] - index / 30) < 1e-5,
                f'{label}: frame/time sequence')
        require(frame['compose_cpu_ms'] >= 0, f'{label}: negative CPU time')
        require(0 <= frame['glyph_changes'] <= frame['matched'] <= grid[0] * grid[1],
                f'{label}: history statistics')
        cov = frame['coverage']
        if cov is None:
            require(backend is None and 0 < index < frames - 1,
                    f'{label}: missing required coverage at frame {index}')
            continue
        n = cov[f'{kind}_cells']
        require(n > 0 and cov[f'{kind}_border_hits'] == 0, f'{label}: missing/clipped geometry')
        bounds = cov[f'{kind}_bounds']
        require(bounds is not None and 0 < bounds[0] <= bounds[2] < grid[0] - 1
                and 0 < bounds[1] <= bounds[3] < grid[1] - 1, f'{label}: bounds')
        metrics = cov['material_metrics']
        require(metrics['total']['cells'] == n, f'{label}: material coverage count')
        bucket_check(metrics['total'], label)
        groups = metrics['geometric_normal_groups']
        require(sum(b['cells'] for b in groups.values() if b) == n, f'{label}: orientation count')
        require(groups['invalid_normal'] is None, f'{label}: invalid geometric normals')
        for name, bucket in groups.items():
            bucket_check(bucket, f'{label}/{name}')
        if kind == 'metal':
            require(0 <= cov['channel_limit_cells'] <= n, f'{label}: channel-limit count')
            require(sum(cov['material_counts'].values()) == n, f'{label}: material ID counts')
    if path == 'static':
        first = data['frames'][0]
        for frame in data['frames'][1:]:
            for key in ['eye', 'target', 'light', 'rgb_id_hash']:
                require(frame[key] == first[key], f'{label}: static {key} changed')
            if frame['coverage'] is not None:
                require(frame['coverage'] == first['coverage'], f'{label}: static coverage changed')
            require(frame['changed_cells'] == frame['glyph_changes'] == 0,
                    f'{label}: static output changed')
    return data


def matched(a, b, label, kind='metal', different_rgb=True):
    for key in ['frame_count', 'grid', 'output_pixels', 'fps_simulated', 'path', 'camera',
                'light_rgb_linear', f'{kind}_center', f'{kind}_half_extents', 'floor_height']:
        require(a[key] == b[key], f'{label}: unmatched {key}')
    differences = 0
    color_differences = 0
    for left, right in zip(a['frames'], b['frames']):
        for key in ['frame', 'time', 'eye', 'target', 'light']:
            require(left[key] == right[key], f'{label}: unmatched {key}')
        differences += left['rgb_id_hash'] != right['rgb_id_hash']
        if left['coverage'] is None or right['coverage'] is None:
            require(left['coverage'] is None and right['coverage'] is None,
                    f'{label}: unmatched coverage availability at frame {left["frame"]}')
            continue
        for key in [f'{kind}_cells', f'{kind}_border_hits', f'{kind}_bounds']:
            require(left['coverage'][key] == right['coverage'][key], f'{label}: unmatched {key}')
        color_differences += (left['coverage']['material_metrics']['total']['color']['mean_linear_rgb_clamped']
                              != right['coverage']['material_metrics']['total']['color']['mean_linear_rgb_clamped'])
    if different_rgb:
        require(differences > 0 and color_differences > 0,
                f'{label}: material variants never change RGB/ID hash and measured RGB means')
    return differences


def pngs(directory, expected, static=False):
    expected_names = {f'frame-{index:04}.png' for index in expected}
    require({p.name for p in directory.glob('frame-*.png')} == expected_names,
            f'{relative(directory)}: missing/stale PNG frames')
    hashes = []
    for name in sorted(expected_names):
        path = directory / name
        with path.open('rb') as stream:
            header = stream.read(24)
        require(header[:8] == b'\x89PNG\r\n\x1a\n' and header[12:16] == b'IHDR'
                and struct.unpack('>II', header[16:24]) == (1920, 1080), f'{path}: PNG dimensions')
        hashes.append(sha(path))
    if static:
        require(len(set(hashes)) == 1, f'{directory}: static PNG changed')
    elif len(expected) > 1:
        require(len(set(hashes)) > 1, f'{directory}: motion PNGs unchanged')


def summary(data, case):
    frames = data['frames']
    matched_cells = sum(f['matched'] for f in frames[1:])
    audited = [f for f in frames if f['coverage'] is not None]
    require(audited and audited[0]['frame'] == 0 and audited[-1]['frame'] == len(frames) - 1,
            f'{case}: first/last coverage required for summary')
    total_cells = sum(f['coverage']['metal_cells'] for f in audited)
    buckets = [f['coverage']['material_metrics']['total'] for f in audited]
    return {'case': case, 'study_sha256': sha(ROOT / case / 'study.json'),
            'adapter': data['gpu']['adapter'] if data['gpu'] is not None else None,
            'variant': data['variant'], 'camera': data['camera'],
            'path': data['path'], 'grid': data['grid'], 'workers': data['workers'],
            'frames': data['frame_count'], 'cpu_ms': data['compose_cpu_ms'],
            'max_frame_cpu_ms': max(f['compose_cpu_ms'] for f in frames),
            'submit_wait_ms': data['submit_wait_ms'],
            'p95_over_exploratory_budget': data['compose_cpu_ms']['p95'] > BUDGET_MS,
            'glyph_change_fraction_of_matched': sum(f['glyph_changes'] for f in frames[1:]) / matched_cells if matched_cells else None,
            'matched_fraction_of_all_slots': matched_cells / ((len(frames) - 1) * math.prod(data['grid'])),
            'audited_frame_count': len(audited),
            'audited_frame_indices': [f['frame'] for f in audited],
            'visual_metric_scope': 'Only frames with a coverage audit. Weighted fractions/means divide by '
                                   'metal_cell_samples across those frames; the maximum also uses only audited frames. '
                                   'CPU timings and history statistics use the complete frame sequence. '
                                   'CPU-only runs require first/last audits and may omit intermediate audits.',
            'metal_cell_samples': total_cells,
            'blank_fraction': sum(b['blank_cells'] for b in buckets) / total_cells,
            'sparse_nonblank_fraction': sum(b['sparse_nonblank_cells'] for b in buckets) / total_cells,
            'mean_atlas_ink': sum(b['mean_atlas_ink'] * b['cells'] for b in buckets) / total_cells,
            'mean_channel_energy_proxy': sum(b['color']['mean_channel_energy_proxy'] * b['cells'] for b in buckets) / total_cells,
            'channel_limit_fraction': sum(f['coverage']['channel_limit_cells'] for f in audited) / total_cells,
            'max_frame_channel_limit_fraction': max(f['coverage']['channel_limit_cells'] / f['coverage']['metal_cells'] for f in audited)}


def performance(cases):
    return {'case_count': len(cases), 'exploratory_p95_budget_ms': BUDGET_MS,
            'over_budget_case_count': sum(c['p95_over_exploratory_budget'] for c in cases),
            'over_budget_cases': [c['case'] for c in cases if c['p95_over_exploratory_budget']],
            'max_case_percentiles_ms': {p: max(c['cpu_ms'][p] for c in cases) for p in ['p50', 'p95', 'p99']},
            'max_frame_cpu_ms': max(c['max_frame_cpu_ms'] for c in cases),
            'policy': 'Threshold misses are observations, not evidence-validation failures; no FPS/GPU-time claim.'}


def main():
    baseline = read(OUT / 'baseline/hashes.json')
    require(len(baseline) == 7 and len({b['path'] for b in baseline}) == 7, 'baseline inventory')
    for item in baseline:
        require(sha(OUT / 'baseline' / item['path']) == item['sha256'], f"baseline changed: {item['path']}")
    for name in ['src/glyph.wgsl', 'examples/support/stone_surface.rs', 'examples/stone_study.rs', 'r8-manifest.json']:
        item = next(b for b in baseline if b['path'] == name)
        require(sha(ROOT / item['source']) == item['sha256'], f'preserved source changed: {name}')

    checked_pngs = legacy_pngs = metal_backend_pairs = stone_backend_pairs = 0
    static_cases = []
    for backend in ['vulkan', 'dx12']:
        for mode in MODES:
            pair = []
            for variant in VARIANTS:
                directory = OUT / f'static-{backend}' / variant / mode
                d = study(directory, variant, mode, 'static', 2, GRIDS['detail'], backend)
                pngs(directory, [0, 1], static=True)
                checked_pngs += 2
                static_cases.append(summary(d, relative(directory)))
                if variant in FINISHES:
                    pair.append(d)
                if backend == 'dx12':
                    for index in [0, 1]:
                        name = f'frame-{index:04}.png'
                        require(sha(directory / name) == sha(OUT / 'static-vulkan' / variant / mode / name),
                                f'metal backend mismatch: {variant}/{mode}/{name}')
                        metal_backend_pairs += 1
            matched(*pair, f'static-{backend}/{mode}')
            for variant in STONE_VARIANTS:
                directory = OUT / f'legacy-static-{backend}' / variant / mode
                study(directory, variant, mode, 'static', 2, GRIDS['detail'], backend, kind='stone')
                pngs(directory, [0, 1], static=True)
                checked_pngs += 2
                for index in [0, 1]:
                    name = f'frame-{index:04}.png'
                    require(sha(directory / name) == sha(ROOT / f'evidence/r8/static-{backend}' / variant / mode / name),
                            f'R8 static regression: {directory}/{name}')
                    legacy_pngs += 1
                    if backend == 'dx12':
                        require(sha(directory / name) == sha(OUT / 'legacy-static-vulkan' / variant / mode / name),
                                f'stone backend mismatch: {directory}/{name}')
                        stone_backend_pairs += 1
    for mode in MODES:
        for variant in ['dry', 'wet-r7', 'wet']:
            directory = OUT / 'legacy-motion' / variant / mode
            current = study(directory, variant, mode, 'showcase', 180, GRIDS['detail'], 'vulkan', kind='stone')
            old = read(ROOT / 'evidence/r8/motion' / variant / mode / 'study.json')
            matched(current, old, relative(directory), kind='stone', different_rgb=False)
            require(all(a['rgb_id_hash'] == b['rgb_id_hash'] for a, b in zip(current['frames'], old['frames'])),
                    f'R8 motion RGB regression: {directory}')
            pngs(directory, [0, 90, 179])
            checked_pngs += 3
            for index in [0, 90, 179]:
                name = f'frame-{index:04}.png'
                require(sha(directory / name) == sha(ROOT / 'evidence/r8/motion' / variant / mode / name),
                        f'R8 motion PNG regression: {directory}/{name}')
                legacy_pngs += 1

    matrix = []
    for grid, dimensions in GRIDS.items():
        for path in PATHS:
            count = 60 if path == 'static' else 180
            for mode in MODES:
                pair = []
                for variant in FINISHES:
                    directory = OUT / 'matrix' / grid / path / variant / mode
                    d = study(directory, variant, mode, path, count, dimensions, 'vulkan', 8)
                    pngs(directory, [0, count - 1], static=path == 'static')
                    checked_pngs += 2
                    pair.append(d)
                    matrix.append(summary(d, relative(directory)))
                matched(*pair, f'matrix/{grid}/{path}/{mode}')

    cpu_cases, cpu_repeated, motion, preview = [], [], [], []
    cpu_data = {}
    for repeat in [1, 2, 3]:
        for workers in [1, 8]:
            for mode in MODES:
                pair = []
                for variant in FINISHES:
                    directory = OUT / f'cpu/r{repeat}-w{workers}' / variant / mode
                    d = study(directory, variant, mode, 'showcase', 180, GRIDS['detail'], None, workers)
                    require(not list(directory.glob('frame-*.png')), f'{directory}: CPU run has stale captures')
                    cpu_data[repeat, workers, mode, variant] = d
                    pair.append(d)
                    cpu_cases.append(summary(d, relative(directory)))
                matched(*pair, f'cpu/r{repeat}-w{workers}/{mode}')
    for variant in FINISHES:
        for mode in MODES:
            for workers in [1, 8]:
                runs = [cpu_data[r, workers, mode, variant] for r in [1, 2, 3]]
                reference = cpu_data[1, 1, mode, variant]
                for d in runs:
                    matched(d, reference, f'cpu repeat/{variant}/{mode}', different_rgb=False)
                    for a, b in zip(d['frames'], reference['frames']):
                        for key in ['rgb_id_hash', 'coverage', 'matched', 'glyph_changes', 'mean_color_delta', 'changed_cells']:
                            require(a[key] == b[key], f'CPU worker/repeat output mismatch: {variant}/{mode}/{key}')
                cpu_repeated.append({'variant': variant, 'camera': mode, 'workers': workers,
                                     'p50_runs_ms': [d['compose_cpu_ms']['p50'] for d in runs],
                                     'p95_runs_ms': [d['compose_cpu_ms']['p95'] for d in runs],
                                     'p99_runs_ms': [d['compose_cpu_ms']['p99'] for d in runs],
                                     'median_p50_ms': statistics.median(d['compose_cpu_ms']['p50'] for d in runs),
                                     'max_p95_ms': max(d['compose_cpu_ms']['p95'] for d in runs)})
    for phase, variants, captures, target in [('preview', VARIANTS, [0, 90, 179], preview),
                                              ('motion', FINISHES, list(range(180)), motion)]:
        for mode in MODES:
            pair = []
            for variant in variants:
                directory = OUT / phase / variant / mode
                d = study(directory, variant, mode, 'showcase', 180, GRIDS['detail'], 'vulkan')
                pngs(directory, captures)
                checked_pngs += len(captures)
                target.append(summary(d, relative(directory)))
                if variant in FINISHES:
                    pair.append(d)
            matched(*pair, f'{phase}/{mode}')

    prebounds_pngs = prebounds_frame_hashes = 0
    for mode in MODES:
        for variant in FINISHES:
            before = read(OUT / 'preview' / variant / mode / 'study.json')
            after = read(OUT / 'motion' / variant / mode / 'study.json')
            matched(before, after, f'prebounds/{variant}/{mode}', different_rgb=False)
            for a, b in zip(before['frames'], after['frames']):
                require(a['rgb_id_hash'] == b['rgb_id_hash'], f'prebounds RGB/ID changed: {variant}/{mode}')
                require(a['coverage'] == b['coverage'], f'prebounds material metrics changed: {variant}/{mode}')
                prebounds_frame_hashes += 1
            for index in [0, 90, 179]:
                name = f'frame-{index:04}.png'
                require(sha(OUT / 'preview' / variant / mode / name) == sha(OUT / 'motion' / variant / mode / name),
                        f'prebounds PNG changed: {variant}/{mode}/{name}')
                prebounds_pngs += 1

    prebounds = {'status': 'NOT_PRESENT', 'pngs_identical': prebounds_pngs,
                 'rgb_id_frame_hashes_identical': prebounds_frame_hashes,
        'hash_scope': 'RGB/ID hash excludes glyph codes; sampled PNGs compare complete pixels.'}
    prebounds_dir = OUT / 'prebounds'
    if prebounds_dir.exists():
        inventory = read(prebounds_dir / 'hashes.json')
        frozen = []
        for item in inventory:
            name = item['Path'].replace('\\', '/').rsplit('/', 1)[-1]
            require(name in ['metal_study.exe', 'metal_geometry.rs'], f'unknown prebounds file: {name}')
            require(sha(prebounds_dir / name) == item['Hash'].lower(), f'prebounds hash: {name}')
            frozen.append({'path': relative(prebounds_dir / name), 'sha256': item['Hash'].lower()})
        require(len(frozen) == 2 and len({p['path'] for p in frozen}) == 2, 'prebounds hash inventory')
        comparisons = []
        for variant in FINISHES:
            for mode in MODES:
                old_runs = []
                for repeat in [1, 2]:
                    directory = prebounds_dir / f'cpu-r{repeat}' / variant / mode
                    before = study(directory, variant, mode, 'showcase', 180, GRIDS['detail'], None, 8)
                    after = cpu_data[1, 8, mode, variant]
                    matched(before, after, relative(directory), different_rgb=False)
                    require(all(a['rgb_id_hash'] == b['rgb_id_hash'] and a['coverage'] == b['coverage']
                                for a, b in zip(before['frames'], after['frames'])),
                            f'prebounds CPU output mismatch: {directory}')
                    old_runs.append(summary(before, relative(directory)))
                final_runs = [cpu_data[r, 8, mode, variant] for r in [1, 2, 3]]
                comparisons.append({'variant': variant, 'camera': mode, 'workers': 8,
                                    'before_runs': old_runs,
                                    'before_median_p50_ms': statistics.median(d['cpu_ms']['p50'] for d in old_runs),
                                    'after_median_p50_ms': statistics.median(d['compose_cpu_ms']['p50'] for d in final_runs),
                                    'before_max_p95_ms': max(d['cpu_ms']['p95'] for d in old_runs),
                                    'after_max_p95_ms': max(d['compose_cpu_ms']['p95'] for d in final_runs)})
        prebounds.update(status='VERIFIED', sources=frozen, cpu_comparisons=comparisons,
                         measurement_scope='Two prebounds and three final runs in different, noninterleaved phases. '
                                           'Observed timings only; scheduling/thermal/other phase effects prevent an isolated acceleration claim.')

    expected_cases = {
        'preview': ['preview'],
        'runtime': ['metal-static-vulkan', 'legacy-static-vulkan', 'metal-static-dx12',
                    'legacy-static-dx12', 'legacy-motion', 'metal-window'],
        'matrix': [f'matrix-{g}-{p}' for g in GRIDS for p in PATHS],
        'cpu': [f'cpu-r{r}-w{w}' for r in [1, 2, 3] for w in ([8, 1] if r == 2 else [1, 8])],
        'motion': ['motion'],
    }
    memory = []
    for phase, expected in expected_cases.items():
        log = read(OUT / f'runs-{phase}.json')
        require([r['case'] for r in log['runs']] == expected, f'{phase}: run inventory/order')
        for run in log['runs']:
            require(run['exit_code'] == 0, f"failed process: {run['case']}")
            binary = ROOT / 'target/release/examples' / ('stone_study.exe' if run['case'].startswith('legacy-') else 'metal_study.exe')
            current = sha(binary) == run['executable_sha256']
            require(current or phase == 'preview', f"stale executable: {run['case']}")
            if phase == 'preview' and prebounds_dir.exists():
                require(run['executable_sha256'] == sha(prebounds_dir / 'metal_study.exe'),
                        'preview executable does not match preserved prebounds executable')
            peak = run['peak_process_working_set_bytes']
            require(peak is None or peak > 0, f"invalid memory: {run['case']}")
            memory.append({**run, 'phase': phase, 'matches_current_executable': current,
                           'memory_scope': log['memory_scope']})

    logs = {}
    for name in ['tests', 'tests-metal-final', 'build-initial', 'build', 'clippy', 'fmt']:
        path = OUT / f'{name}.log'
        text = path.read_text(encoding='utf-8-sig')
        require(not re.search(r'(^|\n)\s*error(?:\[|:)|test result: FAILED|could not compile', text),
                f'{name}.log contains failures')
        entry = {'path': relative(path), 'sha256': sha(path),
                 'warning_lines': [line for line in text.splitlines() if line.startswith('warning:')]}
        if name.startswith('tests'):
            results = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', text)
            require(results and all(int(failed) == 0 for _, failed, _ in results), 'test completion missing')
            entry.update(status='PASS_RECORDED', suites=len(results),
                         executed=sum(int(passed) for passed, _, _ in results),
                         ignored=sum(int(ignored) for _, _, ignored in results))
        elif name == 'fmt':
            require(not text.strip(), 'fmt output is nonempty; review diff before finalization')
            entry['status'] = 'NO_DIFF_OUTPUT_EXIT_NOT_RECORDED'
        else:
            require(re.search(r'Finished\s+`(?:release|dev)`', text), f'{name}: no completion marker')
            entry['status'] = 'COMPLETION_RECORDED_NO_ERROR'
        logs[name] = entry
    initial_tests = (OUT / 'tests.log').read_text(encoding='utf-8-sig')
    require(sha(OUT / 'tests.log') == sha(OUT / 'tests-initial.log'), 'initial test log changed')
    initial_metal = re.search(r'Running unittests examples[\\/]metal_study\.rs.*?'
                             r'test result: ok\. (\d+) passed;', initial_tests, re.DOTALL)
    require(initial_metal is not None, 'initial metal test suite not identified')
    initial_metal_count = int(initial_metal.group(1))
    final_metal_count = logs['tests-metal-final']['executed']
    require(logs['tests-metal-final']['suites'] == 1 and final_metal_count >= initial_metal_count,
            'final metal test suite incomplete')
    test_inventory = {'initial_workspace_executions': logs['tests']['executed'],
                      'initial_metal_executions': initial_metal_count,
                      'final_metal_executions': final_metal_count,
                      'actual_pass_executions': logs['tests']['executed'] + final_metal_count,
                      'equivalent_final_suite_executions': logs['tests']['executed'] - initial_metal_count + final_metal_count,
                      'scope': 'Actual executions include repeated metal tests. Equivalent final inventory replaces '
                               'the earlier metal suite; it is not a second full workspace execution or a count of independent behaviors.'}

    movie = OUT / 'video/materials/metallo-tre-visuali.mp4'
    verification = read(movie.parent / 'verification.json')
    stream = verification['streams'][0]
    require((stream['width'], stream['height'], stream['r_frame_rate'], int(stream['nb_read_frames']))
            == (2560, 1080, '30/1', 540), 'video dimensions/cadence/frames')
    require(float(verification['format']['duration']) == 18 and verification['full_decode'] == 'PASS',
            'video duration/full-decode evidence')
    encoding = read(movie.parent / 'encoding.json')
    require(len(encoding['commands']) == 4, 'video command inventory')
    source_paths = [
        'examples/metal_study.rs', 'examples/support/metal_geometry.rs', 'examples/support/metal_surface.rs',
        'examples/stone_study.rs', 'examples/support/stone_geometry.rs', 'examples/support/stone_surface.rs',
        'examples/support/stone_metrics.rs', 'crates/ascii-render/src/core.rs', 'crates/ascii-render/src/frame.rs',
        'crates/ascii-render/src/camera.rs', 'crates/ascii-render/src/gpu.rs', 'crates/ascii-render/src/glyph.wgsl',
        'Cargo.toml', 'Cargo.lock', 'scripts/verify-r9.py', 'scripts/encode-r9.py', 'scripts/finalize-r9.py',
        'target/release/examples/metal_study.exe', 'target/release/examples/stone_study.exe',
    ]
    return {'revision': 'R9 metal study', 'status': 'PASS', 'baseline_preserved': len(baseline),
            'legacy_r8_pngs_identical': legacy_pngs, 'metal_backend_pairs_identical': metal_backend_pairs,
            'stone_backend_pairs_identical': stone_backend_pairs, 'checked_pngs': checked_pngs,
            'tests_executed': test_inventory['actual_pass_executions'], 'test_inventory': test_inventory, 'logs': logs,
            'static_cases': static_cases, 'matrix_cases': matrix, 'cpu_cases': cpu_cases,
            'cpu_repeated': cpu_repeated, 'motion_cases': motion, 'preview_cases': preview,
            'prebounds_comparison': prebounds,
            'performance_summary': {name: performance(cases) for name, cases in
                                    [('matrix', matrix), ('cpu_serial', [c for c in cpu_cases if c['workers'] == 1]),
                                     ('cpu_eight_workers', [c for c in cpu_cases if c['workers'] == 8]), ('motion', motion)]},
            'process_memory': memory,
            'sources': [{'path': name, 'sha256': sha(ROOT / name)} for name in source_paths],
            'video': {'path': relative(movie), 'sha256': sha(movie), 'verification': verification,
                      'encoding_log_sha256': sha(movie.parent / 'encoding.json')},
            'scope': 'Matched metal geometry, camera and lighting; forged/oxidized RGB intentionally differ. '
                     'R8 stone PNG and RGB regression checks are separate. Baseline source contracts do not require recompilation. '
                     'Atlas ink/energy and channel-limit counts are proxies, not perceived luminance or pre-clamp saturation magnitude. '
                     'CPU excludes audit/capture/IO; synchronized submit/wait is not GPU time or FPS. '
                     'Memory is process working set, not VRAM. Preview may predate the final executable and is marked accordingly. '
                     'One host/two backends do not certify other hardware, physical accuracy or perceptual quality.'}


if __name__ == '__main__':
    try:
        result = main()
    except (OSError, ValueError, KeyError, TypeError, IndexError, StopIteration) as error:
        result = {'revision': 'R9 metal study', 'status': 'FAIL',
                  'error': f'{type(error).__name__}: {error}',
                  'scope': 'Incomplete/inconsistent evidence; no success or performance conclusion.'}
        OUT.mkdir(parents=True, exist_ok=True)
        (OUT / 'manifest.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
        print(json.dumps(result, indent=2), file=sys.stderr)
        sys.exit(1)
    (OUT / 'manifest.json').write_text(json.dumps(result, indent=2, allow_nan=False), encoding='utf-8')
    print(json.dumps({key: result[key] for key in ['status', 'baseline_preserved', 'legacy_r8_pngs_identical',
                     'metal_backend_pairs_identical', 'stone_backend_pairs_identical', 'tests_executed',
                     'performance_summary']}, indent=2))
