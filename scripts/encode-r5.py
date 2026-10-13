"""Encode actual wgpu frames, without interpolation or image enhancement.

The comparison crops empty side margins at native pixel scale. Titles are
video annotations, not engine output. Source PNGs remain untouched.
"""
import argparse
import json
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
parser.add_argument('--source', default='evidence/r5/motion')
parser.add_argument('--out', default='evidence/r5/video')
parser.add_argument('--ffmpeg', default='C:/ffmpeg/bin/ffmpeg.exe')
args = parser.parse_args()
source, out = Path(args.source).resolve(), Path(args.out).resolve()
out.mkdir(parents=True, exist_ok=True)
font = "fontfile='C\\:/Windows/Fonts/consola.ttf'"
commands = []

def run(arguments):
    command = [args.ffmpeg, '-hide_banner', '-loglevel', 'error', '-n', *map(str, arguments)]
    commands.append(command)
    subprocess.run(command, check=True)

def title(text, x, y, size=28):
    return f"drawtext={font}:text='{text}':x={x}:y={y}:fontsize={size}:fontcolor=0xc9d5d5"

encoding = ['-an', '-c:v', 'libx264', '-preset', 'medium', '-crf', '16',
            '-pix_fmt', 'yuv420p', '-movflags', '+faststart']
modes = [('first', 'PRIMA PERSONA'), ('third', 'TERZA PERSONA'), ('overhead', "VISTA DALL ALTO")]
for mode, label in modes:
    baseline = source / 'baseline' / mode
    current = source / 'current' / mode
    data = json.loads((current / 'study.json').read_text(encoding='utf-8-sig'))
    old = json.loads((baseline / 'study.json').read_text(encoding='utf-8-sig'))
    assert data['frame_count'] == old['frame_count']
    assert data['simulation_ticks_per_second'] == old['simulation_ticks_per_second']
    # Refuse a montage that might hide clipped or off-centre geometry.
    for manifest in (data, old):
        for frame in manifest['frames']:
            bounds = frame['counts']['occupied_bounds_inclusive']
            assert frame['counts']['border_hits'] == 0
            assert bounds and bounds[0] >= 60 and bounds[2] < 180, bounds
    fps, frames = data['simulation_ticks_per_second'], data['frame_count']
    run(['-framerate', fps, '-i', current / 'frame-%04d.png', '-frames:v', frames,
         '-vf', title(f'R5 / {label}', 56, 48), *encoding, out / f'current-{mode}.mp4'])
    filters = (f"[0:v]crop=960:1080:480:0,{title('PRIMA - R4', 48, 106)}[a];"
               f"[1:v]crop=960:1080:480:0,{title('ADESSO - R5', 48, 106)}[b];"
               f"[a][b]hstack=inputs=2,{title(label, 48, 42)}[v]")
    run(['-framerate', fps, '-i', baseline / 'frame-%04d.png',
         '-framerate', fps, '-i', current / 'frame-%04d.png',
         '-filter_complex', filters, '-map', '[v]', '-frames:v', frames,
         *encoding, out / f'comparison-{mode}.mp4'])

for prefix, destination in [('current', 'personaggio-tre-visuali.mp4'),
                            ('comparison', 'confronto-r4-r5.mp4')]:
    listing = out / f'{prefix}-concat.txt'
    listing.write_text(''.join(f"file '{prefix}-{mode}.mp4'\n" for mode, _ in modes), encoding='utf-8')
    run(['-f', 'concat', '-safe', '0', '-i', listing, '-c', 'copy',
         '-movflags', '+faststart', out / destination])

context = source.parent / 'context'
if context.exists():
    for mode, label in modes:
        assert len(list((context / mode).glob('sequence-*.png'))) == 180
        run(['-framerate', 30, '-i', context / mode / 'sequence-%04d.png',
             '-frames:v', 180, '-vf', title(f'CORTILE / {label}', 56, 48),
             *encoding, out / f'context-{mode}.mp4'])
    listing = out / 'context-concat.txt'
    listing.write_text(''.join(f"file 'context-{mode}.mp4'\n" for mode, _ in modes), encoding='utf-8')
    run(['-f', 'concat', '-safe', '0', '-i', listing, '-c', 'copy',
         '-movflags', '+faststart', out / 'personaggio-nel-cortile.mp4'])
(out / 'encoding.json').write_text(json.dumps({
    'commands': commands, 'source': str(source),
    'processing': 'Native GPU PNG frames at simulation cadence. No interpolation, retiming, denoising or colour enhancement. Comparison crops black side margins; added labels only.'
}, indent=2), encoding='utf-8')
print(out)
