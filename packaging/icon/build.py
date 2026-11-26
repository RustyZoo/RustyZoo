#!/usr/bin/env python3
"""RustyZoo app icon: hand-placed pixel art, native 16, 24 and 32 grids.

    python3 build.py   # writes png/

Nearest neighbour only: every larger size is an integer upscale of a native grid.
"""
import pathlib

from PIL import Image

OUT = pathlib.Path(__file__).resolve().parent

PAL = {
    '.': None,
    'k': '#1f2416',
    'h': '#fff8e2', 'p': '#f1e8cf', 'q': '#bdbba0', 'Q': '#7f8676',
    'L': '#a9c25a', 'l': '#6c9238', 'G': '#416a31', 'g': '#2d4a22', 'd': '#1b2f24',
    'O': '#f3b45c', 'o': '#df7a3a', 'r': '#b04e2a', 'R': '#6c2c2e',
    'Y': '#fff0b0', 'y': '#f0cd68', 'b': '#c4923a', 'B': '#7a6232',
}

# native 16: two stone pillars with brass caps, arched rust sign with RZ, four bars
GATE16 = [
    '.kk...kkkk...kk.',
    'kYbk.kooork.kYbk',
    'kbBkkoorrrrkkbBk',
    'kkkkorrrrrrRkkkk',
    'khqkpprrpppRkhqk',
    'khqkprprrrpRkhqk',
    'khqkpprrrprRkhqk',
    'khqkprprprrRkhqk',
    'khqkprprpppRkhqk',
    'khqkRRRRRRRRkhqk',
    'kqqkkkkkkkkkkqqk',
    'khqkGyGyyGyGkhqk',
    'khqkGkGkkGkGkhqk',
    'khqkgkgkkgkgkhqk',
    'khqkgkgkkgkgkhqk',
    'kkkkkkkkkkkkkkkk',
]

# native 24 (ICO only)
GATE24 = [
    '..kk.....kkkkkk.....kk..',
    '.kYbk..kkooooorkk..kYbk.',
    '.kbBk.koorrrrrrrrk.kbBk.',
    'kkkkkkorrrrrrrrrrrkkkkkk',
    'kyyybkoppprrrppppRkyyybk',
    'kkkkkkopRRprrrRRpRkkkkkk',
    'khpqQkopRrpRrrrprRkhpqQk',
    'khpqQkoppprRrrprRRkhpqQk',
    'khpqQkopRpRrrprRrRkhpqQk',
    'khpqQkopRrprrppppRkhpqQk',
    'khpqQkrRRRRRRRRRRRkhpqQk',
    'kqqqqkkkkkkkkkkkkkkqqqqk',
    'khpqQkyGyGGyyGGyGykhpqQk',
    'khpqQkkGkGGkkGGkGkkhpqQk',
    'khpqQkkGkGGkkGGkGkkhpqQk',
    'khpqQkkGkGGkkGGkGkkhpqQk',
    'khpqQkkGkGGkkGGkGkkhpqQk',
    'kqqqqkkkkkkkkkkkkkkqqqqk',
    'khpqQkkgkggkkggkgkkhpqQk',
    'khpqQkkgkggkkggkgkkhpqQk',
    'khpqQkkgkggkkggkgkkhpqQk',
    'khpqQkkgkggkkggkgkkhpqQk',
    'khpqQkkgkggkkggkgkkhpqQk',
    'kkkkkkkkkkkkkkkkkkkkkkkk',
]

# native 32
GATE32 = [
    '..kkkk......kkkkkkkk......kkkk..',
    '.kYybbk...kkooooooorkk...kYybbk.',
    '.kybbBk..koorrrrrrrrrrk..kybbBk.',
    '..kBBk..korrrrrrrrrrrrrk..kBBk..',
    'kkkkkkkkoppppprrrpppppprkkkkkkkk',
    'kyyyyybkoppRRpprrrRRRppRkyyyyybk',
    'kbbbbbBkoppRrppRrrrrppRRkbbbbbBk',
    'kkkkkkkkopppppRRrrrppRRRkkkkkkkk',
    'khpppqQkoppRppRrrrppRRrRkhpppqQk',
    'khpppqQkoppRrpprrppRRrrRkhpppqQk',
    'khpppqQkoppRrppRrppRrrrRkhpppqQk',
    'khpppqQkoppRrppRrppppppRkhpppqQk',
    'khpppqQkrRRRRRRRRRRRRRRRkhpppqQk',
    'kqqqqqqkkkkkkkkkkkkkkkkkkqqqqqqk',
    'khpppqQkyGyGyGGyyGGyGyGykhpppqQk',
    'khpppqQkkGkGkGGkkGGkGkGkkhpppqQk',
    'khpppqQkkGkGkGGkkGGkGkGkkhpppqQk',
    'khpppqQkkGkGkGGkkGGkGkGkkhpppqQk',
    'khpppqQkkkkkkkkkkkkkkkkkkhpppqQk',
    'kqqqqqqkkGkGkGGkkGGkGkGkkqqqqqqk',
    'khpppqQkkGkGkGGkkGGkGkGkkhpppqQk',
    'khpppqQkkGkGkGGkkGGkGkGkkhpppqQk',
    'khpppqQkkgkgkggkkggkgkgkkhpppqQk',
    'khpppqQkkgkgkggkkggkgkgkkhpppqQk',
    'khpppqQkkgkgkggkkggkgkgkkhpppqQk',
    'kqqqqqqkkgkgkggkkggkgkgkkqqqqqqk',
    'khpppqQkkkkkkkkkkkkkkkkkkhpppqQk',
    'khpppqQkkgkgkggkkggkgkgkkhpppqQk',
    'khpppqQkkgkgkggkkggkgkgkkhpppqQk',
    'khpppqQkkgkgkggkkggkgkgkkhpppqQk',
    'khpppqQkkgkgkggkkggkgkgkkhpppqQk',
    'kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk',
]

def rgb(h):
    return tuple(int(h[i:i + 2], 16) for i in (1, 3, 5))


def sprite(rows):
    n = len(rows)
    assert all(len(r) == n for r in rows), [i for i, r in enumerate(rows) if len(r) != n]
    im = Image.new('RGBA', (n, n))
    im.putdata([rgb(PAL[c]) + (255,) if PAL[c] else (0, 0, 0, 0) for r in rows for c in r])
    return im


def up(im, k):
    return im.resize((im.width * k, im.height * k), Image.NEAREST)


def sizes(s16, s24, s32):
    """Every exported size -> RGBA image, nearest neighbour from the native grids."""
    out = {16: s16, 24: s24, 32: s32, 48: up(s16, 3), 64: up(s32, 2)}
    out.update({n: up(s32, n // 32) for n in (128, 256, 512, 1024)})
    return out


def main():
    ex = sizes(sprite(GATE16), sprite(GATE24), sprite(GATE32))
    (OUT / 'png').mkdir(exist_ok=True)
    for n in (16, 32, 48, 64, 128, 256, 512, 1024):
        ex[n].save(OUT / 'png' / f'rustyzoo-{n}.png', optimize=True)


if __name__ == '__main__':
    main()
