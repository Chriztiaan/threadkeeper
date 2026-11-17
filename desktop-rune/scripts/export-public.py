"""Export the reviewed app files without local Git history or private team data.
This does not initialize, commit, or push a repository.
"""
from pathlib import Path
import argparse, json, shutil
root=Path(__file__).resolve().parents[2]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--output',type=Path,default=root/'public-export')
args=parser.parse_args()
out=args.output.resolve()
if out.exists() and any(out.iterdir()):
    parser.error('Output must be empty; existing exports are never overwritten.')
for name,key in [('tracker.config.json','repositories'),('maintainers.json','maintainers')]:
    if json.loads((root/name).read_text()).get(key):
        parser.error(f'{name} still contains a private roster; public defaults must be empty.')
files=['.gitignore','Cargo.toml','Cargo.lock','rust-toolchain.toml','README.md','tracker.config.json','maintainers.json','people.example.json','vips.example.json']
folders=['.cargo','.github','tracker-core/src','tracker-core/tests','desktop-rune/src','desktop-rune/scripts','docs']
files+=['tracker-core/Cargo.toml','tracker-core/README.md','desktop-rune/Cargo.toml','desktop-rune/README.md','desktop-rune/.gitignore','desktop-rune/rust-toolchain.toml']
files+=['desktop-rune/assets/'+name for name in ['README.md','threadkeeper-app-icon.png','threadkeeper-mark.svg','threadkeeper-mark-body.svg']]
for folder in folders:
    files += [str(p.relative_to(root)) for p in (root/folder).rglob('*') if p.is_file() and not any(part.startswith('.') or part=='__pycache__' for part in p.relative_to(root/folder).parts)]
for name in files:
    src=root/name
    if src.is_symlink():raise SystemExit(f'Refusing symlink: {name}')
    dest=out/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dest)
print(f'Exported {len(files)} files to {out}. No Git history, live rosters, credentials, caches or build output copied.')
