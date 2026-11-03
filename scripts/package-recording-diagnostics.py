"""Build and package the Windows recording checks; optional --target for cross builds."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import zipfile

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--target')
args = parser.parse_args()
root = Path(__file__).resolve().parent.parent
flags = ['--target', args.target] if args.target else []
subprocess.run(['cargo', 'build', '--locked', '--release', *flags], cwd=root, check=True)
result = subprocess.run(['cargo', 'test', '--locked', '--release', *flags, '--bin', 'dunyu', '--no-run', '--message-format=json'], cwd=root, check=True, text=True, stdout=subprocess.PIPE)
artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
test = next(a['executable'] for a in reversed(artifacts) if a.get('reason') == 'compiler-artifact' and a.get('profile', {}).get('test') and a.get('executable'))
release = root / 'target' / (args.target or '') / 'release'
package = root / 'target' / 'recording-diagnostics-v4'
package.mkdir(parents=True, exist_ok=True)
files = {'dunyu.exe': release / 'dunyu.exe', 'dunyu-recording-tests.exe': Path(test),
         'LICENSE': root / 'LICENSE', 'OFL.txt': root / 'resources/fonts/OFL.txt',
         'USAGE.md': root / 'docs/USAGE.md'}
for name in ['run-recording-diagnostics.cmd', 'run-recording-diagnostics.ps1', 'run-hotkey-diagnostics.cmd']:
    files[name] = root / 'scripts' / name
for name, source in files.items():
    shutil.copyfile(source, package / name)
(package / 'README.txt').write_text('Exit DunYu, extract the entire ZIP, then run run-recording-diagnostics.cmd.\nKeep the test window visible and do not type. Temporary settings only.\nFour fresh processes compare none, log, foreground and raw observers.\nSend the entire recording-results-* directory after the run.\nPASS covers automated recording only, not physical Win+Q takeover.\nFor manual app tracing use run-hotkey-diagnostics.cmd with your existing data folder.\n', encoding='utf-8')
names = [*files, 'README.txt']
(package / 'SHA256SUMS.txt').write_text(''.join(hashlib.sha256((package / n).read_bytes()).hexdigest() + '  ' + n + '\n' for n in names), encoding='ascii')
archive = root / 'target' / 'DunYu-hotkey-diagnostics-v4.zip'
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as output:
    for name in [*names, 'SHA256SUMS.txt']:
        output.write(package / name, name)
print(archive)
print(hashlib.sha256(archive.read_bytes()).hexdigest())
