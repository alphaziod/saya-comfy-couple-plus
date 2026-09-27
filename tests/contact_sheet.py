"""Contact sheet of campaign images, plus colour/detail measurements.

    python tests/contact_sheet.py campaign/ --prefix g078 [--cols 5] [--measure]

Writes campaign/sheet_<prefix>.jpg. --measure prints mean saturation, colourfulness (Hasler-Suesstrunk)
and gradient detail per image and on average. Needs Pillow and numpy.
"""

import argparse
import glob
import os

from PIL import Image, ImageDraw


def measures(path):
    import numpy as np
    im = Image.open(path).convert("RGB")
    a = np.asarray(im).astype(float)
    sat = np.asarray(im.convert("HSV"))[..., 1].astype(float).mean()
    rg = a[..., 0] - a[..., 1]
    yb = .5 * (a[..., 0] + a[..., 1]) - a[..., 2]
    col = np.sqrt(rg.std() ** 2 + yb.std() ** 2) + .3 * np.sqrt(rg.mean() ** 2 + yb.mean() ** 2)
    gy = np.asarray(im.convert("L")).astype(float)
    det = np.abs(np.diff(gy, axis=0)).mean() + np.abs(np.diff(gy, axis=1)).mean()
    return sat, col, det


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("folder")
    ap.add_argument("--prefix", required=True)
    ap.add_argument("--cols", type=int, default=5)
    ap.add_argument("--measure", action="store_true")
    a = ap.parse_args()
    files = sorted(glob.glob(os.path.join(a.folder, f"{a.prefix}*_seed*.png")), key=os.path.getmtime)
    if not files:
        raise SystemExit("no images")
    w, h = 400, 585
    rows = (len(files) + a.cols - 1) // a.cols
    sheet = Image.new("RGB", (a.cols * w, rows * h), "white")
    for i, f in enumerate(files):
        im = Image.open(f).convert("RGB").resize((w, h))
        d = ImageDraw.Draw(im)
        d.rectangle((0, 0, 260, 20), fill="black")
        d.text((5, 4), f"#{i + 1} {os.path.basename(f)[:-4]}", fill="yellow")
        sheet.paste(im, ((i % a.cols) * w, (i // a.cols) * h))
    out = os.path.join(a.folder, f"sheet_{a.prefix}.jpg")
    sheet.save(out, quality=90)
    print("wrote", out)
    if a.measure:
        vals = [measures(f) for f in files]
        for f, (s, c, d) in zip(files, vals):
            print(f"{os.path.basename(f):48} sat {s:5.1f}  colourfulness {c:5.1f}  detail {d:5.2f}")
        n = len(vals)
        print(f"{'MEAN':48} sat {sum(v[0] for v in vals) / n:5.1f}  colourfulness {sum(v[1] for v in vals) / n:5.1f}  detail {sum(v[2] for v in vals) / n:5.2f}")


if __name__ == "__main__":
    main()
