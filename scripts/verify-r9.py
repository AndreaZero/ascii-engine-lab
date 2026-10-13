"""Serialized R9 runtime/capture/CPU checks. Release binaries must already exist.
Never run concurrently with builds or an encoder. Raw output is local evidence.
"""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
import json
from pathlib import Path
import subprocess
import time

parser=argparse.ArgumentParser()
parser.add_argument('--phase', choices=['preview','runtime','matrix','cpu','motion'], required=True)
args=parser.parse_args()
root=Path(__file__).resolve().parent.parent
out=root/'evidence/r9'
metal=root/'target/release/examples/metal_study.exe'
stone=root/'target/release/examples/stone_study.exe'
records=[]

class Memory(ctypes.Structure):
    _fields_=[('cb',wintypes.DWORD),('PageFaultCount',wintypes.DWORD)]+[(n,ctypes.c_size_t) for n in ['PeakWorkingSetSize','WorkingSetSize','QuotaPeakPagedPoolUsage','QuotaPagedPoolUsage','QuotaPeakNonPagedPoolUsage','QuotaNonPagedPoolUsage','PagefileUsage','PeakPagefileUsage']]

memory_fn=ctypes.WinDLL('psapi').GetProcessMemoryInfo
memory_fn.argtypes=[wintypes.HANDLE,ctypes.POINTER(Memory),wintypes.DWORD]
memory_fn.restype=wintypes.BOOL

def run(name,binary,arguments):
    command=[str(binary),*map(str,arguments)]
    peak=None
    begin=time.perf_counter()
    with (out/f'{name}.log').open('w',encoding='utf-8') as log:
        # Hidden console helper; --window explicitly creates the tested surface.
        process=subprocess.Popen(command,cwd=root,stdout=log,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW)
        while process.poll() is None:
            counters=Memory();counters.cb=ctypes.sizeof(counters)
            if memory_fn(int(process._handle),ctypes.byref(counters),counters.cb):
                peak=max(peak or 0,counters.PeakWorkingSetSize)
            time.sleep(0.025)
        code=process.wait()
    records.append({'case':name,'command':command,'exit_code':code,'wall_seconds':time.perf_counter()-begin,'peak_process_working_set_bytes':peak,'executable_sha256':hashlib.sha256(binary.read_bytes()).hexdigest()})
    (out/f'runs-{args.phase}.json').write_text(json.dumps({'memory_scope':'Windows GetProcessMemoryInfo process peak working set, sampled while alive; includes host/driver allocations, not dedicated GPU memory. None means unavailable. Polling outside app timers.','runs':records},indent=2),encoding='utf-8')
    if code:
        raise RuntimeError(f'{name} failed: '+(out/f'{name}.log').read_text(encoding='utf-8')[-3000:])
    print(name,flush=True)

if args.phase=='preview':
    run('preview',metal,['--variant','all','--camera','all','--frames',180,'--capture-every',90,'--backend','vulkan','--out',out/'preview'])
elif args.phase=='runtime':
    for backend in ['vulkan','dx12']:
        run(f'metal-static-{backend}',metal,['--variant','all','--camera','all','--path','static','--frames',2,'--backend',backend,'--out',out/f'static-{backend}'])
        run(f'legacy-static-{backend}',stone,['--variant','all','--camera','all','--path','static','--frames',2,'--backend',backend,'--out',out/f'legacy-static-{backend}'])
    run('legacy-motion',stone,['--variant','trio','--camera','all','--frames',180,'--capture-every',90,'--backend','vulkan','--out',out/'legacy-motion'])
    run('metal-window',metal,['--window','--seconds',3,'--variant','forged','--camera','third','--backend','vulkan'])
elif args.phase=='matrix':
    for grid in ['economy','balanced','detail']:
        for path in ['static','orbit','light','approach']:
            run(f'matrix-{grid}-{path}',metal,['--variant','pair','--camera','all','--path',path,'--frames',180 if path!='static' else 60,'--grid',grid,'--workers',8,'--backend','vulkan','--capture-every',0,'--out',out/f'matrix/{grid}/{path}'])
elif args.phase=='cpu':
    for repeat in range(1,4):
        for workers in ([8,1] if repeat==2 else [1,8]):
            run(f'cpu-r{repeat}-w{workers}',metal,['--benchmark','--variant','pair','--camera','all','--path','showcase','--frames',180,'--workers',workers,'--out',out/f'cpu/r{repeat}-w{workers}'])
elif args.phase=='motion':
    run('motion',metal,['--variant','pair','--camera','all','--path','showcase','--frames',180,'--workers',8,'--backend','vulkan','--out',out/'motion'])
