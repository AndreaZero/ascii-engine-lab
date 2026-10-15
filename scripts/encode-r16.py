"""Assemble native R16 captures without rescaling, recoloring or interpolation.

Run only after capture/benchmark work. ffmpeg must be on PATH or at C:/ffmpeg/bin.
"""
from pathlib import Path
import json
import shutil
import subprocess

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "evidence/r16/video"
OUT.mkdir(parents=True, exist_ok=True)
FFMPEG = shutil.which("ffmpeg") or "C:/ffmpeg/bin/ffmpeg.exe"
FFPROBE = shutil.which("ffprobe") or "C:/ffmpeg/bin/ffprobe.exe"
records = []


def run(args):
    result = subprocess.run(args, cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    records.append({"command": args, "exit": result.returncode})
    (OUT / "encoding.json").write_text(json.dumps(records, indent=2), encoding="utf-8")
    if result.returncode:
        raise RuntimeError(result.stderr)
    return result.stdout


segments = []
for mode in ("first", "third", "overhead"):
    for path in ("rotate", "camera", "deform"):
        filename = f"{mode}-{path}.mp4"
        args = [FFMPEG, "-y", "-v", "error"]
        for figure in ("warden", "courier"):
            source = ROOT / f"evidence/r16/motion/{figure}-{path}-vulkan/{mode}/frame-%04d.png"
            args += ["-framerate", "30", "-i", str(source)]
        # Crop only the empty side margins, retaining source pixel scale.
        # Captured bounds are checked separately; no subject is cropped.
        label = f"R16 | {mode} | {path} | warden (left) / courier (right) | simulated 30 Hz"
        filters = ("[0:v]crop=960:1080:480:0[a];[1:v]crop=960:1080:480:0[b];"
                   "[a][b]hstack=inputs=2,drawtext=fontfile='C\\:/Windows/Fonts/arial.ttf':"
                   f"text='{label}':x=32:y=32:fontsize=24:fontcolor=white:box=1:boxcolor=black[v]")
        args += ["-filter_complex", filters, "-map", "[v]", "-frames:v", "60", "-c:v", "libx264",
                 "-preset", "medium", "-crf", "16", "-pix_fmt", "yuv420p", str(OUT / filename)]
        run(args)
        segments.append(filename)
concat = OUT / "segments.txt"
concat.write_text("\n".join(f"file '{f}'" for f in segments), encoding="utf-8")
final = OUT / "figure-three-views.mp4"
run([FFMPEG, "-y", "-v", "error", "-f", "concat", "-safe", "0", "-i", str(concat),
     "-c", "copy", "-movflags", "+faststart", str(final)])
run([FFMPEG, "-v", "error", "-i", str(final), "-f", "null", "-"])
metadata = json.loads(run([FFPROBE, "-v", "error", "-count_frames", "-show_streams", "-of", "json", str(final)]))
stream = metadata["streams"][0]
assert int(stream["nb_read_frames"]) == 540 and stream["width"] == 1920 and stream["height"] == 1080
(OUT / "validation.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")
print(final)
