from pathlib import Path
import argparse, plistlib, shutil, subprocess, tomllib
parser=argparse.ArgumentParser(description="Package the native Threadkeeper executable as a macOS app.")
parser.add_argument("--profile", choices=["debug","release"], default="debug")
parser.add_argument("--target", help="Optional Rust target triple")
parser.add_argument("--version")
parser.add_argument("--build-number", default="1")
args=parser.parse_args()
root = Path(__file__).resolve().parents[1]
app = root / 'dist' / 'Threadkeeper.app'
macos = app / 'Contents' / 'MacOS'
resources = app / 'Contents' / 'Resources'
macos.mkdir(parents=True, exist_ok=True)
resources.mkdir(parents=True, exist_ok=True)
staged = macos / 'threadkeeper.new'
binary=root/'target'
if args.target: binary=binary/args.target
binary=binary/args.profile/'threadkeeper'
shutil.copy2(binary, staged)
staged.chmod(0o755)
staged.replace(macos / 'threadkeeper')
info = {'CFBundleName':'Threadkeeper','CFBundleDisplayName':'Threadkeeper',
 'CFBundleIdentifier':'app.threadkeeper.desktop','CFBundleVersion':args.build_number,'CFBundleShortVersionString':args.version or tomllib.loads((root/'Cargo.toml').read_text())['package']['version'],
 'CFBundleExecutable':'threadkeeper','CFBundlePackageType':'APPL','NSHighResolutionCapable':True,
 'NSPrincipalClass':'NSApplication','LSMinimumSystemVersion':'13.0','CFBundleIconFile':'AppIcon'}
with (app / 'Contents' / 'Info.plist').open('wb') as out: plistlib.dump(info,out)
iconset=root/'dist'/'AppIcon.iconset';iconset.mkdir(exist_ok=True)
seed=root/'dist'/'icon.png'
cache=root.parent/'.local'/'swift-module-cache';cache.mkdir(parents=True,exist_ok=True)
if not (resources/'AppIcon.icns').exists() or max((root/'scripts'/'icon.swift').stat().st_mtime, (root/'assets'/'threadkeeper-app-icon.png').stat().st_mtime) > (resources/'AppIcon.icns').stat().st_mtime:
 shutil.copy2(root/'assets'/'threadkeeper-app-icon.png',seed)
 for points in [16,32,128,256,512]:
  for scale in [1,2]:
   pixels=str(points*scale);suffix='@2x' if scale==2 else ''
   subprocess.run(['/usr/bin/sips','-z',pixels,pixels,str(seed),'--out',str(iconset/f'icon_{points}x{points}{suffix}.png')],check=True,stdout=subprocess.DEVNULL)
 subprocess.run(['/usr/bin/iconutil','-c','icns',str(iconset),'-o',str(resources/'AppIcon.icns')],check=True)
print(app)
