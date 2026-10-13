"""Matched direct-GPU dry/wet comparison. Crops only margins, never the stone."""
from pathlib import Path
import json
import subprocess

root=Path(__file__).resolve().parent.parent
source=root/'evidence/r7/motion'
out=root/'evidence/r7/video'
out.mkdir(parents=True,exist_ok=True)
ffmpeg='C:/ffmpeg/bin/ffmpeg.exe'
ffprobe='C:/ffmpeg/bin/ffprobe.exe'
font="fontfile='C\\:/Windows/Fonts/consola.ttf'"
modes=[('first','PRIMA PERSONA'),('third','TERZA PERSONA'),('overhead','VISTA DALL ALTO')]
commands=[]

def read(path): return json.loads(path.read_text(encoding='utf-8-sig'))
def run(parts):
    cmd=[ffmpeg,'-hide_banner','-loglevel','error','-n',*map(str,parts)]
    commands.append(cmd)
    subprocess.run(cmd,check=True)
def title(text,x,y,size=28,condition=None):
    t=f"drawtext={font}:text='{text}':x={x}:y={y}:fontsize={size}:fontcolor=0xcbd4d4"
    if condition: t+=f":enable='{condition}'"
    return t

for mode,label in modes:
    a=read(source/'dry'/mode/'study.json');b=read(source/'wet'/mode/'study.json')
    assert a['frame_count']==b['frame_count']==180 and a['fps_simulated']==b['fps_simulated']==30
    for fa,fb in zip(a['frames'],b['frames']):
        for key in ['eye','target','light','time','coverage']: assert fa[key]==fb[key]
        bounds=fa['coverage']['stone_bounds']
        assert fa['coverage']['stone_border_hits']==0
        assert bounds and bounds[0]>=40 and bounds[2]<200, (mode,bounds)
    filters=(f"[0:v]crop=1280:1080:320:0,{title('ASCIUTTA',40,76)}[a];"
             f"[1:v]crop=1280:1080:320:0,{title('BAGNATA',40,76)}[b];"
             f"[a][b]hstack=inputs=2,{title(label,40,28)},"
             f"{title('CAMERA IN MOVIMENTO - LUCE FERMA',1300,28,24,'lt(t,3)')},"
             f"{title('CAMERA FERMA - LUCE IN MOVIMENTO',1300,28,24,'gte(t,3)')}[v]")
    run(['-framerate',30,'-i',source/'dry'/mode/'frame-%04d.png',
         '-framerate',30,'-i',source/'wet'/mode/'frame-%04d.png',
         '-filter_complex',filters,'-map','[v]','-frames:v',180,'-an','-c:v','libx264',
         '-preset','medium','-crf',16,'-pix_fmt','yuv420p','-movflags','+faststart',out/f'{mode}.mp4'])

listing=out/'concat.txt'
listing.write_text(''.join(f"file '{m}.mp4'\n" for m,_ in modes),encoding='utf-8')
movie=out/'pietra-asciutta-bagnata-tre-visuali.mp4'
run(['-f','concat','-safe',0,'-i',listing,'-c','copy','-movflags','+faststart',movie])
verification=json.loads(subprocess.check_output([ffprobe,'-v','error','-count_frames','-show_entries','stream=width,height,r_frame_rate,nb_read_frames:format=duration','-of','json',str(movie)]))
s=verification['streams'][0]
assert (s['width'],s['height'],s['r_frame_rate'],s['nb_read_frames'])==(2560,1080,'30/1','540')
assert float(verification['format']['duration'])==18.0
subprocess.run([ffmpeg,'-v','error','-xerror','-i',str(movie),'-f','null','-'],check=True)
verification['full_decode']='PASS'
(out/'verification.json').write_text(json.dumps(verification,indent=2),encoding='utf-8')
(out/'encoding.json').write_text(json.dumps({'commands':commands,'processing':'1080 native GPU pixels, 320-pixel side margins cropped from each image; whole stone coverage checked in every frame. Dry/wet matched tick/camera/light. No scaling, interpolation, denoising or colour enhancement. Labels added. Simulated30Hz is not measured FPS.'},indent=2),encoding='utf-8')
print(movie)
