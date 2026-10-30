"""Makes a synthetic photo library for bench-local.mjs: many tiny JPEGs (the scale of a library) or a few large ones (the cost of
reading a photo and drawing its pictures), each with EXIF dates spread over years.

    python bench-library.py <folder> tiny 5000
    python bench-library.py <folder> large 200
"""
import os
import random
import sys

from PIL import Image, ImageDraw, ExifTags

folder, kind, count = sys.argv[1], sys.argv[2], int(sys.argv[3])
os.makedirs(folder, exist_ok=True)
rng = random.Random(7)


def when():
    year, month, day = rng.randint(2015, 2026), rng.randint(1, 12), rng.randint(1, 28)
    return f'{year}:{month:02d}:{day:02d} {rng.randint(0, 23):02d}:{rng.randint(0, 59):02d}:{rng.randint(0, 59):02d}'


def exif_of(stamp, lat=None, lon=None):
    exif = Image.Exif()
    exif.get_ifd(ExifTags.IFD.Exif)[0x9003] = stamp
    exif[ExifTags.IFD.Exif] = exif.get_ifd(ExifTags.IFD.Exif)
    if lat is not None:
        gps = exif.get_ifd(ExifTags.IFD.GPSInfo)
        gps[1], gps[2], gps[3], gps[4] = 'N' if lat >= 0 else 'S', (abs(lat), 0.0, 0.0), 'E' if lon >= 0 else 'W', (abs(lon), 0.0, 0.0)
        exif[ExifTags.IFD.GPSInfo] = gps
    return exif


if kind == 'tiny':
    for i in range(count):
        year_dir = os.path.join(folder, f'tiny-{i % 40:02d}')
        os.makedirs(year_dir, exist_ok=True)
        im = Image.new('RGB', (64, 48), (rng.randint(0, 255), rng.randint(0, 255), rng.randint(0, 255)))
        im.save(os.path.join(year_dir, f'img_{i:05d}.jpg'), exif=exif_of(when(), rng.uniform(-60, 60), rng.uniform(-170, 170)))
else:
    base = Image.effect_noise((3000, 2000), 40).convert('RGB')
    for i in range(count):
        im = base.copy()
        ImageDraw.Draw(im).rectangle([i % 2500, i % 1500, i % 2500 + 400, i % 1500 + 300], fill=(i % 256, 120, 200))
        im.save(os.path.join(folder, f'large_{i:04d}.jpg'), quality=82, exif=exif_of(when(), rng.uniform(-60, 60), rng.uniform(-170, 170)))
print('ok', kind, count)
