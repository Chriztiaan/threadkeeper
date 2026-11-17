"""Render real Rune views with fictional data, then build a looping screenshot tour."""
from pathlib import Path
import os, subprocess, tempfile
root=Path(__file__).resolve().parents[2]
out=root/'docs'/'screenshots';out.mkdir(parents=True,exist_ok=True)
binary=root/'desktop-rune'/'target'/'debug'/'threadkeeper'
with tempfile.TemporaryDirectory(prefix='threadkeeper-showcase-') as directory:
    # Isolate preview cache reads from the live workspace, including detail and credentials.
    Path(directory,'snapshot.json').write_text('{}')
    Path(directory,'desktop-detail.json').write_text('{}')
    env=dict(os.environ,TRACKER_DESKTOP_DATA_DIR=directory)
    captures=[('comments','showcase-comments','1380x880','dark'),('vips','showcase-vips','960x720','dark'),('welcome','welcome','680x760','dark'),('follow-up','showcase-followup','1380x880','dark'),('developer-work','showcase-developer','1380x880','dark'),('github-setup','github-setup','680x760','dark')]
    for name,scene,size,theme in captures:
        subprocess.run([str(binary),'--snap',str(out/f'{name}.png'),'--scene',scene,'--theme',theme,'--scale','1','--size',size],env=env,check=True)
    # One additional view using a deterministic scripted selection in the actual native UI.
    script=Path(directory,'select.json')
    script.write_text('[{"t":0.1,"key":"entry-showcase-1","type":"click"}]')
    subprocess.run([str(binary),'--snap',str(out/'pull-request.png'),'--scene','showcase-followup','--script',str(script),'--theme','dark','--scale','1','--size','1380x880'],env=env,check=True)
cache=root/'.local'/'swift-module-cache';cache.mkdir(parents=True,exist_ok=True)
subprocess.run(['swift','-module-cache-path',str(cache),str(root/'desktop-rune/scripts/tour.swift'),str(out/'threadkeeper-tour.gif'),*[str(out/f'{name}.png') for name in ['follow-up','pull-request','developer-work']]],check=True)
print(out)
