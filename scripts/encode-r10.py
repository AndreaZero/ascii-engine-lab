"""Encode native wgpu captures with labels, without image enhancement."""
from pathlib import Path
import json
import subprocess

root = Path(__file__).resolve().parent.parent
source = root / 'evidence/r10/motion/on'
out = root / 'evidence/r10/video'
out.mkdir(parents=True, exist_ok=True)
ffmpeg = 'C:/ffmpeg/bin/ffmpeg.exe'
ffprobe = 'C:/ffmpeg/bin/ffprobe.exe'
font = "fontfile='C\\:/Windows/Fonts/consola.ttf'"
modes = [('first', 'PRIMA PERSONA'), ('third', 'TERZA PERSONA'), ('overhead', 'VISTA DALL ALTO')]
commands = []
for mode, label in modes:
    report = json.loads((source/mode/'study.json').read_text())
    assert report['frame_count'] == 180 and report['fps_simulated'] == 30
    assert all(all(v > 0 for v in f['coverage']['counts']) for f in report['frames'])
    assert all(not any(f['coverage']['border_hits']) for f in report['frames'])
    filters = (f"drawtext={font}:text='R10 - {label}':x=40:y=28:fontsize=30:fontcolor=0xcbd4d4,"
               f"drawtext={font}:text='AMBIENTE ATTIVO - PIETRA / TESSUTO / METALLO':x=40:y=70:fontsize=23:fontcolor=0xcbd4d4")
    command = [ffmpeg,'-hide_banner','-loglevel','error','-n','-framerate','30','-i',str(source/mode/'frame-%04d.png'),
               '-vf',filters,'-frames:v','180','-an','-c:v','libx264','-preset','medium','-crf','16',
               '-pix_fmt','yuv420p','-movflags','+faststart',str(out/f'{mode}.mp4')]
    subprocess.run(command, check=True)
    commands.append(command)
listing = out/'concat.txt'
listing.write_text(''.join(f"file '{mode}.mp4'\n" for mode,_ in modes))
movie = out/'ambiente-comune-tre-visuali.mp4'
subprocess.run([ffmpeg,'-hide_banner','-loglevel','error','-n','-f','concat','-safe','0','-i',str(listing),
                '-c','copy','-movflags','+faststart',str(movie)],check=True)
verification = json.loads(subprocess.check_output([ffprobe,'-v','error','-count_frames','-show_entries',
    'stream=width,height,r_frame_rate,nb_read_frames:format=duration','-of','json',str(movie)]))
s = verification['streams'][0]
assert (s['width'],s['height'],s['r_frame_rate'],s['nb_read_frames']) == (1920,1080,'30/1','540')
assert float(verification['format']['duration']) == 18
subprocess.run([ffmpeg,'-v','error','-xerror','-i',str(movie),'-f','null','-'],check=True)
verification['full_decode'] = 'PASS'
(out/'verification.json').write_text(json.dumps(verification,indent=2))
(out/'encoding.json').write_text(json.dumps({'commands':commands,'processing':
    'Native 1920x1080 GPU captures; labels only, no resampling, interpolation, denoising or grading. Simulated cadence, not measured FPS.'},indent=2))
print(movie)
