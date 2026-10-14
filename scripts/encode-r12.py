"""R12 native-resolution center/Conservative9 comparison, simulated 30 Hz."""
from pathlib import Path
import json
import subprocess

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "evidence/r12/motion"
OUT = ROOT / "evidence/r12/video"
OUT.mkdir(parents=True, exist_ok=True)
FFMPEG = "C:/ffmpeg/bin/ffmpeg.exe"
FFPROBE = "C:/ffmpeg/bin/ffprobe.exe"
MODES = (("first", "PRIMA PERSONA"), ("third", "TERZA PERSONA"), ("overhead", "VISTA DALL ALTO"))
commands = []
parts = []
font = "fontfile='C\\:/Windows/Fonts/consola.ttf'"


def run(arguments):
    command = [FFMPEG, "-hide_banner", "-loglevel", "error", "-n", *map(str, arguments)]
    commands.append(command)
    subprocess.run(command, check=True, creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))


for mode, label in MODES:
    for sampling in ("center", "conservative9"):
        source = SOURCE / sampling
        report = json.loads((source / "run.json").read_text(encoding="utf-8"))
        assert report["output"] == [1920,1080] and report["simulation_ticks_per_second"] == 30
        assert len(list((source/mode).glob("frame-*.png"))) == 180
        name = f"{mode}-{sampling}.mp4"
        title = f"drawtext={font}:text='R12 / {label} / {sampling.upper()}':x=40:y=28:fontsize=27:fontcolor=0xc9d5d5"
        subtitle = f"drawtext={font}:text='Stessa scena e percorso - 30 Hz simulati - non misura FPS':x=40:y=65:fontsize=21:fontcolor=0x99a6a6"
        run(["-framerate",30,"-i",source/mode/"frame-%04d.png","-frames:v",180,
            "-vf",title+","+subtitle,"-an","-c:v","libx264","-preset","medium","-crf",16,
            "-pix_fmt","yuv420p","-movflags","+faststart",OUT/name])
        parts.append(name)
listing = OUT / "concat.txt"
listing.write_text("".join(f"file '{name}'\n" for name in parts), encoding="utf-8")
movie = OUT / "copertura-confronto-tre-visuali.mp4"
run(["-f","concat","-safe",0,"-i",listing,"-c","copy","-movflags","+faststart",movie])
verification = json.loads(subprocess.check_output([FFPROBE,"-v","error","-count_frames",
    "-show_entries","stream=width,height,r_frame_rate,nb_read_frames:format=duration","-of","json",str(movie)]))
stream = verification["streams"][0]
assert (stream["width"],stream["height"],stream["r_frame_rate"],stream["nb_read_frames"]) == (1920,1080,"30/1","1080")
assert float(verification["format"]["duration"]) == 36.0
subprocess.run([FFMPEG,"-v","error","-xerror","-i",str(movie),"-f","null","-"], check=True)
verification["full_decode"] = "PASS"
(OUT/"verification.json").write_text(json.dumps(verification,indent=2),encoding="utf-8")
(OUT/"encoding.json").write_text(json.dumps({"commands":commands,
    "processing":"Native 1920x1080 wgpu frames, center then conservative9 per view, labels only; no interpolation/resizing/enhancement. Simulated cadence, not measured FPS."},indent=2),encoding="utf-8")
print(movie)
