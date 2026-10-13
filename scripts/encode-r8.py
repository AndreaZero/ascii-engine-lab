"""Encode matched native GPU captures; no enhancement or resampling."""
from pathlib import Path
import json
import subprocess

root = Path(__file__).resolve().parent.parent
source = root / 'evidence/r8/motion'
base = root / 'evidence/r8/video'
ffmpeg = 'C:/ffmpeg/bin/ffmpeg.exe'
ffprobe = 'C:/ffmpeg/bin/ffprobe.exe'
font = "fontfile='C\\:/Windows/Fonts/consola.ttf'"
modes = [('first', 'PRIMA PERSONA'), ('third', 'TERZA PERSONA'), ('overhead', 'VISTA DALL ALTO')]

def read(path): return json.loads(path.read_text(encoding='utf-8-sig'))
def title(text, x, y, size=28, condition=None):
    text = f"drawtext={font}:text='{text}':x={x}:y={y}:fontsize={size}:fontcolor=0xcbd4d4"
    return text + (f":enable='{condition}'" if condition else '')

for kind, left, right, label_left, label_right, filename in [
    ('comparison', 'wet-r7', 'wet', 'BAGNATO R7 - PRIMA', 'BAGNATO R8 - ADESSO', 'bagnato-prima-dopo-tre-visuali.mp4'),
    ('materials', 'dry', 'wet', 'ASCIUTTA', 'BAGNATA R8', 'pietra-asciutta-bagnata-r8.mp4'),
]:
    out = base / kind
    out.mkdir(parents=True, exist_ok=True)
    commands = []
    def run(parts):
        command = [ffmpeg, '-hide_banner', '-loglevel', 'error', '-n', *map(str, parts)]
        commands.append(command)
        subprocess.run(command, check=True)
    for mode, label in modes:
        a = read(source / left / mode / 'study.json')
        b = read(source / right / mode / 'study.json')
        assert a['frame_count'] == b['frame_count'] == 180
        assert a['fps_simulated'] == b['fps_simulated'] == 30
        for fa, fb in zip(a['frames'], b['frames']):
            for key in ['eye', 'target', 'light', 'time']: assert fa[key] == fb[key]
            for key in ['stone_bounds', 'stone_border_hits', 'stone_cells']:
                assert fa['coverage'][key] == fb['coverage'][key]
            if kind == 'comparison': assert fa['rgb_id_hash'] == fb['rgb_id_hash']
            bounds = fa['coverage']['stone_bounds']
            assert fa['coverage']['stone_border_hits'] == 0
            assert bounds and bounds[0] >= 40 and bounds[2] < 200
        filters = (f"[0:v]crop=1280:1080:320:0,{title(label_left,40,76)}[a];"
                   f"[1:v]crop=1280:1080:320:0,{title(label_right,40,76)}[b];"
                   f"[a][b]hstack=inputs=2,{title(label,40,28)},"
                   f"{title('CAMERA IN MOVIMENTO - LUCE FERMA',1300,28,24,'lt(t,3)')},"
                   f"{title('CAMERA FERMA - LUCE IN MOVIMENTO',1300,28,24,'gte(t,3)')}[v]")
        run(['-framerate',30,'-i',source/left/mode/'frame-%04d.png',
             '-framerate',30,'-i',source/right/mode/'frame-%04d.png',
             '-filter_complex',filters,'-map','[v]','-frames:v',180,'-an','-c:v','libx264',
             '-preset','medium','-crf',16,'-pix_fmt','yuv420p','-movflags','+faststart',out/f'{mode}.mp4'])
    listing = out / 'concat.txt'
    listing.write_text(''.join(f"file '{mode}.mp4'\n" for mode, _ in modes), encoding='utf-8')
    movie = out / filename
    run(['-f','concat','-safe',0,'-i',listing,'-c','copy','-movflags','+faststart',movie])
    verification = json.loads(subprocess.check_output([ffprobe,'-v','error','-count_frames','-show_entries',
        'stream=width,height,r_frame_rate,nb_read_frames:format=duration','-of','json',str(movie)]))
    s = verification['streams'][0]
    assert (s['width'],s['height'],s['r_frame_rate'],s['nb_read_frames']) == (2560,1080,'30/1','540')
    assert float(verification['format']['duration']) == 18.
    subprocess.run([ffmpeg,'-v','error','-xerror','-i',str(movie),'-f','null','-'],check=True)
    verification['full_decode'] = 'PASS'
    (out/'verification.json').write_text(json.dumps(verification,indent=2),encoding='utf-8')
    (out/'encoding.json').write_text(json.dumps({'commands':commands,'processing':
        'Native 1080px GPU captures, checked side-margin crops and labels only. No resampling, interpolation, denoising or color grading. Simulated cadence, not FPS.'},indent=2),encoding='utf-8')
    print(movie, flush=True)
