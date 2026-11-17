"""Record a reproducible, offline onboarding/workspace tour using Rune's virtual clock.
Run from any directory: python3 desktop-rune/scripts/demo-tour.py
Requires macOS, Swift and the built threadkeeper debug binary. No GitHub token needed.
"""
from pathlib import Path
import argparse, hashlib, json, os, shutil, subprocess, tempfile
root = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--verify', action='store_true', help='Replay twice and compare all checkpoint screenshots')
parser.add_argument('--gif', type=Path, default=root/'threadkeeper-demo.gif', help='GIF destination (default: repo root)')
parser.add_argument('--fps', type=int, default=12)
parser.add_argument('--output', type=Path, default=root/'.local/demo-tour')
args = parser.parse_args()
if not 1 <= args.fps <= 60:
    parser.error('--fps must be between 1 and 60')
args.output.mkdir(parents=True, exist_ok=True)
args.gif.parent.mkdir(parents=True, exist_ok=True)
binary = root/'desktop-rune/target/debug/threadkeeper'
if not binary.exists():
    parser.error('Build first: cargo build -p threadkeeper --offline')
events = []
shots = {}
def click(t, key): events.append(dict(t=t, type='click', key=key))
def shot(t, name):
    events.append(dict(t=t, type='mark', name=name)); shots[name] = t
shot(0.5, '01-welcome')
click(1.5, 'get-started')
shot(2, '02-connect-github')
click(2.5, 'github-token-input')
events.append(dict(t=2.7, type='type', text='mock_token', every=0.08))
shot(3.7, '03-mock-token')
click(4, 'connect-github-token')
shot(5, '04-follow-up')
for i, key in enumerate(['day', 'week', 'month', 'six-months', 'cold']):
    click(6+i*2, 'nav-'+key); shot(7+i*2, f'{5+i:02d}-{key}')
for i, key in enumerate(['review', 'assigned', 'created', 'participating']):
    click(16+i*2, 'dev-'+key); shot(17+i*2, f'{10+i:02d}-{key}')
click(23, 'entry-showcase-0')
events.append(dict(t=24, type='scroll', key='detail-scroll', dy=-800, dur=0.8))
shot(25, '14-discussion')
click(26, 'more-comment-comment-2'); shot(27, '15-expanded-comment')
click(28, 'collapse-comment-comment-2'); shot(29, '16-collapsed-comment')
click(30, 'collapse-comment-comment-2')
click(32, 'manage-repositories'); shot(33, '14-repositories')
click(34, 'settings-maintainers'); shot(35, '15-maintainers')
click(36, 'settings-customers'); shot(37, '16-vips')
click(38, 'highlight-customer-morgan')
events.append(dict(t=39, type='menu-pick', id='subtle-tint'))
click(40, 'save-customer-settings'); shot(41, '17-vip-saved')
click(42, 'settings-developer'); shot(43, '18-developer-settings')
events.append(dict(t=44, type='key', name='Escape'))
click(45, 'nav-all'); shot(46, '19-finish')
events.append(dict(t=47, type='mark', name='end'))
# Store the exact action list alongside outputs for review and reproducibility.
script = args.output/'actions.json'
script.write_text(json.dumps(events, indent=2)+'\n')
with tempfile.TemporaryDirectory(prefix='threadkeeper-tour-') as directory:
    frames=Path(directory)/'frames'
    env=dict(os.environ, THREADKEEPER_DEMO='1', TRACKER_DESKTOP_DATA_DIR=directory)
    env.pop('GITHUB_TOKEN', None)
    command=[str(binary), '--scene','demo-start','--script',str(script), '--theme','dark', '--size','1380x880','--scale','1', '--fps',str(args.fps),'--frames',str(47*args.fps),'--out',str(frames),'--pointer']
    subprocess.run(command,env=env,check=True)
    for name,t in shots.items():
        shutil.copy2(frames/f'frame_{round(t*args.fps):04d}.png',args.output/f'{name}.png')
    if args.verify:
        replay=Path(directory)/'replay'
        repeat=command.copy();repeat[repeat.index('--out')+1]=str(replay)
        subprocess.run(repeat,env=env,check=True)
        for name,t in shots.items():
            expected=(args.output/f'{name}.png').read_bytes()
            actual=(replay/f'frame_{round(t*args.fps):04d}.png').read_bytes()
            if expected!=actual:
                raise RuntimeError(f'Non-deterministic checkpoint: {name}')
        print(f'Verified {len(shots)} identical checkpoint screenshots across two runs.')
    (args.output/'manifest.json').write_text(json.dumps(dict(fps=args.fps,seconds=47,clock='2026-11-17T12:00:00Z',size='1380x880',theme='dark',screenshots={name:hashlib.sha256((args.output/f'{name}.png').read_bytes()).hexdigest() for name in shots}),indent=2)+'\n')
    cache=root/'.local/swift-module-cache'; cache.mkdir(parents=True,exist_ok=True)
    subprocess.run(['swift','-module-cache-path',str(cache),str(root/'desktop-rune/scripts/frames-gif.swift'),str(args.gif),str(frames),str(args.fps)],check=True)
    if shutil.which('ffmpeg'):
        subprocess.run(['ffmpeg','-y','-loglevel','error','-framerate',str(args.fps),'-i',str(frames/'frame_%04d.png'),'-c:v','libx264','-pix_fmt','yuv420p',str(args.output/'threadkeeper-demo.mp4')],check=True)
    else:
        print('GIF and screenshots generated. Install ffmpeg to also export MP4.')
print(f"README GIF: {args.gif}\nDiagnostics: {args.output}")
