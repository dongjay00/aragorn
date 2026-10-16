#!/usr/bin/env python3
"""Aragorn 아이콘과 GitHub 대표 이미지를 만든다. 표준 라이브러리만 쓴다.

32×32 픽셀 아트 한 장(게임보이 화면 속 안두릴)을 정수 배율로 키워 각 형식으로 내보낸다.
아이콘을 고치면 이 파일의 그림을 고치고 다시 실행한다: python3 scripts/gen-icons.py

만드는 파일:
- crates/aragorn-desktop/assets/icon/aragorn.svg   원본 벡터 (32×32 격자)
- crates/aragorn-desktop/assets/icon/aragorn.png   창 아이콘, Linux AppImage (256×256)
- crates/aragorn-desktop/assets/icon/aragorn.ico   Windows 실행 파일·설치 파일
- crates/aragorn-desktop/assets/icon/aragorn.icns  macOS 앱 번들
- docs/images/social-preview.png                   GitHub 대표 이미지 (1280×640)
"""

import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ICON_DIR = ROOT / "crates/aragorn-desktop/assets/icon"
SOCIAL = ROOT / "docs/images/social-preview.png"

SIZE = 32
CLEAR = (0, 0, 0, 0)


def rgb(hex_color):
    return tuple(int(hex_color[i : i + 2], 16) for i in (0, 2, 4)) + (255,)


# 게임보이 화면 4색과 칼날 반사광
DARKEST, DARK, LIGHT, LIGHTEST = rgb("0f380f"), rgb("306230"), rgb("8bac0f"), rgb("9bbc0f")
SHINE = rgb("e0f8d0")
# 본체
BODY, EDGE, BEZEL, BUTTON = rgb("c9c8bf"), rgb("2b2b33"), rgb("5a5f6e"), rgb("a3195b")


class Image:
    def __init__(self, width, height, fill=CLEAR):
        self.width, self.height = width, height
        self.pixels = [[fill] * width for _ in range(height)]

    def set(self, x, y, color):
        if 0 <= x < self.width and 0 <= y < self.height:
            self.pixels[y][x] = color

    def rect(self, x0, y0, x1, y1, color):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                self.set(x, y, color)

    def round_rect(self, x0, y0, x1, y1, r, color):
        for y in range(y0, y1 + 1):
            for x in range(x0, x1 + 1):
                cx = min(max(x, x0 + r), x1 - r)
                cy = min(max(y, y0 + r), y1 - r)
                if (x - cx) ** 2 + (y - cy) ** 2 <= r * r + r * 0.6:
                    self.set(x, y, color)

    def sprite(self, x0, y0, rows, palette):
        for dy, row in enumerate(rows):
            for dx, ch in enumerate(row):
                if ch in palette:
                    self.set(x0 + dx, y0 + dy, palette[ch])

    def paste(self, other, x0, y0):
        for y, row in enumerate(other.pixels):
            for x, color in enumerate(row):
                if color[3]:
                    self.set(x0 + x, y0 + y, color)

    def scaled(self, k):
        """정수 배율로 키운다 (최근접)."""
        out = Image(self.width * k, self.height * k)
        out.pixels = [[c for c in row for _ in range(k)] for row in self.pixels for _ in range(k)]
        return out

    def halved(self):
        """2×2 평균으로 줄인다 (16px 아이콘용). 투명도를 고려해 색을 평균한다."""
        out = Image(self.width // 2, self.height // 2)
        for y in range(out.height):
            for x in range(out.width):
                block = [self.pixels[2 * y + dy][2 * x + dx] for dy in (0, 1) for dx in (0, 1)]
                alpha = sum(c[3] for c in block)
                if alpha:
                    color = tuple(round(sum(c[i] * c[3] for c in block) / alpha) for i in range(3))
                    out.pixels[y][x] = color + (round(alpha / 4),)
        return out

    def png(self):
        raw = b"".join(b"\0" + bytes(v for c in row for v in c) for row in self.pixels)

        def chunk(kind, data):
            return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

        header = struct.pack(">IIBBBBB", self.width, self.height, 8, 6, 0, 0, 0)
        return (
            b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", header)
            + chunk(b"IDAT", zlib.compress(raw, 9))
            + chunk(b"IEND", b"")
        )

    def svg(self):
        rects = "".join(
            f'<rect x="{x}" y="{y}" width="1" height="1" fill="#{c[0]:02x}{c[1]:02x}{c[2]:02x}"/>'
            for y, row in enumerate(self.pixels)
            for x, c in enumerate(row)
            if c[3]
        )
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {self.width} {self.height}" '
            f'shape-rendering="crispEdges">{rects}</svg>\n'
        )


def draw_icon():
    icon = Image(SIZE, SIZE)

    # 본체: 오른쪽 아래만 크게 둥근 DMG 모양. 테두리는 안쪽 픽셀 중 바깥과 닿은 것.
    corners = ((8, 3, 2), (23, 3, 2), (8, 28, 2), (20, 25, 5))

    def inside(x, y):
        if not (6 <= x <= 25 and 1 <= y <= 30):
            return False
        for cx, cy, r in corners:
            if (x < cx if cx < 16 else x > cx) and (y < cy if cy < 16 else y > cy):
                return (x - cx) ** 2 + (y - cy) ** 2 <= r * r + r * 0.6
        return True

    for y in range(SIZE):
        for x in range(SIZE):
            if inside(x, y):
                border = any(not inside(x + dx, y + dy) for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)))
                icon.set(x, y, EDGE if border else BODY)

    icon.round_rect(9, 3, 22, 17, 1, BEZEL)
    icon.rect(10, 4, 21, 16, LIGHTEST)
    icon.sprite(
        10,
        5,
        [
            ".....DD.....",
            "....DSLD....",
            "....DSLD....",
            "....DSLD....",
            "....DSLD....",
            "....DSLD....",
            "..D.DSLD.D..",
            "..DDDDDDDD..",
            ".....MD.....",
            ".....MD.....",
            "....DDDD....",
        ],
        {"D": DARKEST, "M": DARK, "L": LIGHT, "S": SHINE},
    )

    icon.sprite(9, 19, [".X.", "XXX", ".X."], {"X": EDGE})
    icon.rect(19, 21, 20, 22, BUTTON)
    icon.rect(22, 19, 23, 20, BUTTON)
    icon.rect(13, 25, 14, 25, BEZEL)
    icon.rect(17, 25, 18, 25, BEZEL)
    return icon


def icon_sizes(icon):
    """크기 → PNG 바이트. 16은 2×2 평균, 나머지는 정수 배율."""
    sizes = {16: icon.halved().png()}
    for k in (1, 2, 4, 8, 16, 32):
        sizes[SIZE * k] = icon.scaled(k).png()
    return sizes


def ico(sizes):
    """PNG를 담은 ICO (Windows Vista 이후)."""
    entries = [(s, sizes[s]) for s in (16, 32, 64, 128, 256)]
    offset = 6 + 16 * len(entries)
    header = struct.pack("<HHH", 0, 1, len(entries))
    table, data = b"", b""
    for size, png in entries:
        dim = 0 if size == 256 else size  # 256은 0으로 적는다
        table += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    return header + table + data


def icns(sizes):
    """PNG를 담은 ICNS (macOS 10.7 이후). 레티나 항목은 2배 크기 PNG를 쓴다."""
    types = [
        (b"icp4", 16), (b"icp5", 32), (b"icp6", 64),
        (b"ic07", 128), (b"ic08", 256), (b"ic09", 512),
        (b"ic11", 32), (b"ic12", 64), (b"ic13", 256), (b"ic14", 512), (b"ic10", 1024),
    ]
    body = b"".join(kind + struct.pack(">I", 8 + len(sizes[s])) + sizes[s] for kind, s in types)
    return b"icns" + struct.pack(">I", 8 + len(body)) + body


# 5×7 비트맵 글꼴 (대표 이미지 문구에 쓰는 글자만)
FONT = {
    "A": [".XXX.", "X...X", "X...X", "XXXXX", "X...X", "X...X", "X...X"],
    "B": ["XXXX.", "X...X", "X...X", "XXXX.", "X...X", "X...X", "XXXX."],
    "C": [".XXX.", "X...X", "X....", "X....", "X....", "X...X", ".XXX."],
    "E": ["XXXXX", "X....", "X....", "XXXX.", "X....", "X....", "XXXXX"],
    "G": [".XXX.", "X...X", "X....", "X.XXX", "X...X", "X...X", ".XXXX"],
    "L": ["X....", "X....", "X....", "X....", "X....", "X....", "XXXXX"],
    "M": ["X...X", "XX.XX", "X.X.X", "X.X.X", "X...X", "X...X", "X...X"],
    "N": ["X...X", "XX..X", "X.X.X", "X..XX", "X...X", "X...X", "X...X"],
    "O": [".XXX.", "X...X", "X...X", "X...X", "X...X", "X...X", ".XXX."],
    "R": ["XXXX.", "X...X", "X...X", "XXXX.", "X.X..", "X..X.", "X...X"],
    "T": ["XXXXX", "..X..", "..X..", "..X..", "..X..", "..X..", "..X.."],
    "U": ["X...X", "X...X", "X...X", "X...X", "X...X", "X...X", ".XXX."],
    "Y": ["X...X", "X...X", ".X.X.", "..X..", "..X..", "..X..", "..X.."],
    "/": ["....X", "....X", "...X.", "..X..", ".X...", "X....", "X...."],
    " ": ["....."] * 7,
}


def text(image, x, y, string, scale, color):
    for ch in string:
        glyph = Image(5, 7)
        glyph.sprite(0, 0, FONT[ch], {"X": color})
        image.paste(glyph.scaled(scale), x, y)
        x += 6 * scale


def social_preview(icon):
    image = Image(1280, 640, DARKEST)
    image.paste(icon.scaled(14), 80, 96)  # 448×448
    text(image, 592, 236, "ARAGORN", 13, SHINE)
    text(image, 592, 372, "GAME BOY / COLOR", 5, LIGHTEST)
    text(image, 592, 432, "EMULATOR", 5, LIGHTEST)
    return image


def main():
    icon = draw_icon()
    sizes = icon_sizes(icon)
    ICON_DIR.mkdir(parents=True, exist_ok=True)
    SOCIAL.parent.mkdir(parents=True, exist_ok=True)
    (ICON_DIR / "aragorn.svg").write_text(icon.svg())
    (ICON_DIR / "aragorn.png").write_bytes(sizes[256])
    (ICON_DIR / "aragorn.ico").write_bytes(ico(sizes))
    (ICON_DIR / "aragorn.icns").write_bytes(icns(sizes))
    SOCIAL.write_bytes(social_preview(icon).png())
    for path in sorted(ICON_DIR.iterdir()) + [SOCIAL]:
        print(f"{path.relative_to(ROOT)} ({path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
