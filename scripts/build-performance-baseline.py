"""Build 1a026fd with the current measurement protocol, without rendering changes."""
import argparse
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    revision = '1a026fd'
    with tempfile.TemporaryDirectory(prefix='dunyu-baseline-') as temp:
        work = Path(temp)
        archive = subprocess.check_output(['git', 'archive', revision], cwd=root)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            for member in tar.getmembers():
                if not (work / member.name).resolve().is_relative_to(work) or member.issym() or member.islnk():
                    raise ValueError('Unsafe archive entry')
            tar.extractall(work)
        shutil.copy2(root / 'src/desktop/diagnostics.rs', work / 'src/desktop/diagnostics.rs')
        source = root.joinpath('src/desktop/rest_ui.rs').read_text()
        marker = '\n#[cfg(test)]\npub(super) unsafe fn benchmark('
        benchmark = marker + source.split(marker, 1)[1].split('\n#[cfg(test)]\nmod tests {', 1)[0]
        path = work / 'src/desktop/rest_ui.rs'
        text = path.read_text()
        for signature, name in [
            ('    pub unsafe fn close(&mut self) {', 'rest.close'),
            ('    unsafe fn create(&mut self, r: RECT, full: bool, phase: Phase, host: HWND) {', 'rest.create'),
        ]:
            assert signature in text
            text = text.replace(signature, signature + '\n        #[cfg(test)]\n        let _measurement = super::ui::diagnostics::measure("' + name + '");', 1)
        text = text.replace('                ShowWindow(*h, SW_SHOWNOACTIVATE);', '                #[cfg(test)]\n                let _show = super::ui::diagnostics::measure("rest.show_and_paint");\n                ShowWindow(*h, SW_SHOWNOACTIVATE);', 1)
        text = text.replace('let font = CreateFontIndirectW(&lf);', 'let font = { #[cfg(test)] let _font = super::ui::diagnostics::measure("rest.font"); CreateFontIndirectW(&lf) };')
        path.write_text(text + benchmark)
        env = os.environ.copy()
        env['DUNYU_DIAGNOSTIC_REVISION'] = revision + '-performance-v2-baseline'
        cmd = ['cargo', 'test', '--locked', '--release', '--bin', 'dunyu', '--no-run', '--message-format=json', '--target-dir', str(root / 'target')]
        if args.target:
            cmd += ['--target', args.target]
        result = subprocess.run(cmd, cwd=work, env=env, text=True, stdout=subprocess.PIPE, check=True)
        artifacts = [json.loads(line) for line in result.stdout.splitlines() if line.startswith('{')]
        exe = next(r['executable'] for r in reversed(artifacts) if r.get('executable') and r['profile']['test'])
        dest = root / 'target/dunyu-ui-baseline.exe'
        shutil.copy2(exe, dest)
        print(dest)


if __name__ == '__main__':
    main()
