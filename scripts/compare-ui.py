"""Create visual review artifacts from Windows client captures (requires Pillow).

These are diagnostics, not an automatic fidelity pass: fixture text and native
font rasterization can differ. Input dimensions are recorded before normalization.
"""
import json
from pathlib import Path
from PIL import Image, ImageChops, ImageStat

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / 'target/ui-preview'
PAIRS = {'empty': 'ui-summon', 'bind': 'ui-bind', 'opacity': 'ui-opacity',
         'rest': 'ui-rest', 'settings': 'ui-settings',
         'rest-preview': 'rest-preview-card', 'rest-fullscreen': 'rest-preview-fullscreen'}
report = {}
for actual_name, reference_name in PAIRS.items():
    source = OUT / (actual_name + '.png')
    if not source.exists():
        source = source.with_suffix('.bmp')
    if not source.exists():
        raise SystemExit(f'Missing Windows capture: {source}')
    with Image.open(source) as f:
        actual = f.convert('RGB')
    with Image.open(ROOT / 'docs/design' / (reference_name + '.png')) as f:
        reference = f.convert('RGB')
    if reference_name.startswith('ui-'):
        reference = reference.crop((0, 48, 1536, 1024))
    original_size = actual.size
    actual = actual.resize(reference.size, Image.Resampling.LANCZOS)
    difference = ImageChops.difference(reference, actual)
    Image.blend(reference, actual, 0.5).save(OUT / (actual_name + '-overlay.png'))
    difference.save(OUT / (actual_name + '-difference.png'))
    report[actual_name] = {'capture': original_size, 'reference': reference.size,
                           'mean_absolute_rgb_error': ImageStat.Stat(difference).mean}
(OUT / 'comparison.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
