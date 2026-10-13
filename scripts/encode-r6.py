"""Encode direct wgpu captures at simulated cadence; add view labels only."""
from pathlib import Path
import json
import subprocess

root = Path(__file__).resolve().parent.parent
source = root / 'evidence/r6/motion'
out = root / 'evidence/r6/video'
out.mkdir(parents=True, exist_ok=True)
ffmpeg = 'C:/ffmpeg/bin/ffmpeg.exe'
ffprobe = 'C:/ffmpeg/bin/ffprobe.exe'
report = json.loads((source / 'run.json').read_text())
assert report['output'] == [1920, 1080] and report['simulation_ticks_per_second'] == 30
commands = []

def run(arguments):
    command = [ffmpeg, '-hide_banner', '-loglevel', 'error', '-n', *map(str, arguments)]
    commands.append(command)
    subprocess.run(command, check=True)

modes = [('first', 'PRIMA PERSONA'), ('third', 'TERZA PERSONA'), ('overhead', 'VISTA DALL ALTO')]
font = "fontfile='C\\:/Windows/Fonts/consola.ttf'"
for mode, label in modes:
    assert len(list((source / mode).glob('frame-*.png'))) == 180
    title = f"drawtext={font}:text='R6 / CLIENT INDIPENDENTE / {label}':x=48:y=36:fontsize=28:fontcolor=0xc9d5d5"
    subtitle = f"drawtext={font}:text='Campioni tecnici modificabili da file - qualita artistica provvisoria':x=48:y=74:fontsize=22:fontcolor=0x99a6a6"
    run(['-framerate', 30, '-i', source / mode / 'frame-%04d.png', '-frames:v', 180,
         '-vf', title + ',' + subtitle, '-an', '-c:v', 'libx264', '-preset', 'medium',
         '-crf', 16, '-pix_fmt', 'yuv420p', '-movflags', '+faststart', out / f'{mode}.mp4'])
listing = out / 'concat.txt'
listing.write_text(''.join(f"file '{mode}.mp4'\n" for mode, _ in modes), encoding='utf-8')
movie = out / 'material-room-tre-visuali.mp4'
run(['-f', 'concat', '-safe', 0, '-i', listing, '-c', 'copy', '-movflags', '+faststart', movie])
verification = json.loads(subprocess.check_output([ffprobe, '-v', 'error', '-count_frames',
    '-show_entries', 'stream=width,height,r_frame_rate,nb_read_frames:format=duration', '-of', 'json', str(movie)]))
stream = verification['streams'][0]
assert (stream['width'], stream['height'], stream['r_frame_rate'], stream['nb_read_frames']) == (1920,1080,'30/1','540')
assert float(verification['format']['duration']) == 18.0
subprocess.run([ffmpeg, '-v', 'error', '-xerror', '-i', str(movie), '-f', 'null', '-'], check=True)
verification['full_decode'] = 'PASS'
(out / 'verification.json').write_text(json.dumps(verification, indent=2))
(out / 'encoding.json').write_text(json.dumps({'commands':commands,
    'processing':'Native wgpu PNGs, same simulated 30 Hz cadence; labels only, no interpolation, enhancement or resizing. Encoded cadence is not measured display FPS.'}, indent=2))
print(movie)
