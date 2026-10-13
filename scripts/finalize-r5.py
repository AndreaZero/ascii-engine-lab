"""Record tested sources and deliverables without modifying archived evidence."""
from pathlib import Path
import datetime
import hashlib
import json
import re

root = Path(__file__).resolve().parents[1]
out = root / 'evidence/r5'
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))

summary = read(out / 'summary.json')
tests = sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', (out / 'tests.log').read_text(encoding='utf-8-sig'))))
assert tests == 78
protocol = read(out / 'cpu/protocol.json')
assert sha(root / 'target/release/examples/character_study.exe') == protocol['executable_sha256']
videos = read(out / 'video/validation.json')
assert len(videos) == 3 and all(v['full_decode'] == 'pass' for v in videos)
for entry in videos:
    stream = entry['probe']['streams'][0]
    assert (stream['width'], stream['height'], stream['nb_frames'], stream['r_frame_rate']) == (1920,1080,'540','30/1')

sources = [root / 'Cargo.toml', root / 'Cargo.lock', root / 'README.md', root / 'CHARACTER_STUDY.md']
for folder in ('src','examples','tests','scripts'):
    sources.extend(p for p in (root / folder).rglob('*') if p.is_file() and '__pycache__' not in str(p))
shared = root.parent / 'ascii-game'
sources.extend(shared / p for p in ('AGENTS.md','skills.md','docs/COORDINATION.md','docs/RENDERER_PLAN.md',
                                   'docs/CHAT_STARTERS.md','docs/ENGINE_CAPABILITIES.md','docs/ENGINE_MILESTONES.md'))
evidence = [p for p in out.rglob('*') if p.is_file() and p.relative_to(out).parts[0] != 'baseline'
            and p.name != 'manifest.json']
manifest = {
    'tranche': 'R5 character and cape', 'recorded_at': datetime.datetime.now().astimezone().isoformat(),
    'test_executions_including_historical_copies': tests,
    'checks': {'fmt':'pass','clippy_all_targets':'pass','release':'pass','window':'pass',
               'gpu_backend_equal_pairs':len(summary['gpu_equal_backend_pairs']),
               'baseline_r4_hashes_preserved':summary['baseline_hashes_preserved'],
               'godot_project_preserved':summary['godot_project_preserved']},
    'binaries': {str(p.relative_to(root)):sha(p) for p in [root/'target/release/ascii-engine-lab.exe',root/'target/release/examples/character_study.exe']},
    'sources': {str(p.relative_to(root.parent)).replace('\\','/'):sha(p) for p in sorted(set(sources))},
    'evidence': {str(p.relative_to(out)).replace('\\','/'):sha(p) for p in sorted(evidence)},
    'notes': ['Baseline R4 hashes are separately recorded in baseline/hashes.json; historical files unchanged.',
              'preview-a is an intermediate capture before narrowing the yoke seam; motion and context contain the final build.',
              'R5 intentionally changes figure/materials and structural history; no full R4 image-parity claim.',
              'CPU-only timings are not GPU or display FPS; videos replay at simulation cadence, not captured present cadence.',
              'Sampled image inspection and full video decoding, not an automated continuous perceptual review.',
              'The evidence map omits only the root archived-source baseline directory, which has its own hash list; baseline study frames and raw CPU runs are included.']
}
(out / 'manifest.json').write_text(json.dumps(manifest, indent=2),encoding='utf-8')
print(f'Manifest: {tests} test executions, {len(sources)} source/document files, {len(evidence)} evidence files.')
