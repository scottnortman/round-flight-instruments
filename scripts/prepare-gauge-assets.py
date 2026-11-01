"""Extract the unchanged supplied font and generate deterministic raster glyphs/panel commands."""
import base64
import hashlib
from pathlib import Path
import re
import zipfile
from PIL import Image, ImageDraw, ImageFont

root = Path(__file__).resolve().parents[1]
with zipfile.ZipFile(r'D:\Downloads\round-flight-instruments-handoff.zip') as archive:
    name = next(n for n in archive.namelist() if n.endswith('src/instrument-fragment.html'))
    html = archive.read(name).decode('utf-8')
data = base64.b64decode(re.search(r'data:font/otf;base64,([^\x27]+)', html)[1])
assets = root / 'crates/instrument-ui/assets'
assets.mkdir(parents=True, exist_ok=True)
(assets / 'MS33558C.otf').write_bytes(data)
font = ImageFont.truetype(str(assets / 'MS33558C.otf'), 48)
atlas = bytearray()
for code in range(32, 91):
    image = Image.new('L', (40, 56))
    draw = ImageDraw.Draw(image)
    # Preserve the handoff's documented numeral-one glyph substitution.
    glyph = 'I' if chr(code) == '1' else chr(code)
    draw.text((20, 28), glyph, font=font, anchor='mm', fill=255)
    pixels = list(image.getdata())
    atlas.extend((pixels[i] // 17) << 4 | (pixels[i+1] // 17) for i in range(0, len(pixels), 2))
(assets / 'glyphs-4bit.bin').write_bytes(atlas)
print('Original font SHA256:', hashlib.sha256(data).hexdigest())

# Keep only register facts from the SKU-specific official initialization table.
source = (root / 'tmp/imu/demo/ESP-IDF/ESP32-S3-Touch-LCD-2.8C-Test/main/LCD_Driver/ST7701S.c').read_text()
sequence = source.split('if (type == 1){', 1)[1].split('/**', 1)[0]
lines = ['// Generated register sequence from Waveshare SKU 29086 official demo.',
         '// See docs/DISPLAY.md for provenance. No GPIO or timing policy is copied.',
         'pub const INIT: &[(bool,u8)] = &[']
for kind, value in re.findall(r'SPI_Write(Comm|Data)\((0x[0-9A-Fa-f]+)\)', sequence):
    lines.append(f'    ({str(kind == "Data").lower()}, {value}),')
lines.append('];')
(root / 'firmware/console/src/panel_init.rs').write_text('\n'.join(lines)+'\n')
