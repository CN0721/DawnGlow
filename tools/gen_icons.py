#!/usr/bin/env python3
"""生成 DawnGlow 启动图标（纯 Python，仅用 zlib）。

输出到 android/res：
  mipmap-*/ic_launcher.png            传统方形图标
  mipmap-*/ic_launcher_round.png      圆形图标
  mipmap-*/ic_launcher_background.png 自适应背景层（渐变天空）
  mipmap-*/ic_launcher_foreground.png 自适应前景层（太阳 + 云带）
  mipmap-anydpi-v26/ic_launcher(.round).xml
"""
import os
import struct
import zlib

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "android", "res")

DENSITIES = {
    "mdpi": 1.0,
    "hdpi": 1.5,
    "xhdpi": 2.0,
    "xxhdpi": 3.0,
    "xxxhdpi": 4.0,
}


def write_png(path, w, h, buf):
    def chunk(typ, data):
        c = struct.pack(">I", len(data)) + typ + data
        return c + struct.pack(">I", zlib.crc32(typ + data) & 0xFFFFFFFF)

    raw = bytearray()
    stride = w * 4
    for y in range(h):
        raw.append(0)
        raw += buf[y * stride : (y + 1) * stride]
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


def lerp(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


STOPS = [
    (0.00, (0x0E, 0x13, 0x30)),
    (0.50, (0x2C, 0x25, 0x55)),
    (0.72, (0x7A, 0x3D, 0x6B)),
    (0.85, (0xE8, 0x71, 0x4E)),
    (0.93, (0xFF, 0xC1, 0x77)),
    (1.00, (0x2A, 0x15, 0x26)),
]


def grad_color(t):
    t = max(0.0, min(1.0, t))
    for i in range(len(STOPS) - 1):
        t0, c0 = STOPS[i]
        t1, c1 = STOPS[i + 1]
        if t0 <= t <= t1:
            k = (t - t0) / (t1 - t0) if t1 > t0 else 0.0
            return lerp(c0, c1, k)
    return STOPS[-1][1]


def blend(buf, w, h, x, y, rgb, a):
    if x < 0 or y < 0 or x >= w or y >= h or a <= 0:
        return
    i = (y * w + x) * 4
    sa = min(1.0, a)
    da = buf[i + 3] / 255.0
    oa = sa + da * (1 - sa)
    if oa <= 0:
        return
    for k in range(3):
        buf[i + k] = int(round((rgb[k] * sa + buf[i + k] * da * (1 - sa)) / oa))
    buf[i + 3] = int(round(oa * 255))


def smoothstep(edge0, edge1, x):
    t = max(0.0, min(1.0, (x - edge0) / (edge1 - edge0) if edge1 != edge0 else 0.0))
    return t * t * (3 - 2 * t)


def fill_sky(buf, w, h):
    for y in range(h):
        c = grad_color(y / max(1, h - 1))
        for x in range(w):
            i = (y * w + x) * 4
            buf[i] = c[0]
            buf[i + 1] = c[1]
            buf[i + 2] = c[2]
            buf[i + 3] = 255


def draw_sun(buf, w, h, cx, cy, r):
    for y in range(int(cy - r * 2.2), int(cy + r * 2.2) + 1):
        for x in range(int(cx - r * 2.2), int(cx + r * 2.2) + 1):
            d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
            glow = smoothstep(r * 2.2, r * 0.9, d) * 0.55
            disc = smoothstep(r + 1.2, r - 1.2, d)
            a = max(disc, glow)
            if a > 0:
                col = (0xFF, 0xEC, 0xB8) if disc > glow else (0xFF, 0xA8, 0x60)
                blend(buf, w, h, x, y, col, a)


def draw_cloud(buf, w, h, cx, cy, half_w, half_h, color, a):
    for y in range(int(cy - half_h), int(cy + half_h) + 1):
        for x in range(int(cx - half_w), int(cx + half_w) + 1):
            nx = (x - cx) / half_w
            ny = (y - cy) / half_h
            d = (nx * nx + ny * ny) ** 0.5
            cov = smoothstep(1.0, 0.35, d)
            if cov > 0:
                blend(buf, w, h, x, y, color, cov * a)


def for_legacy(size, rounded):
    buf = bytearray(size * size * 4)
    fill_sky(buf, size, size)
    draw_sun(buf, size, size, size / 2, size * 0.70, size * 0.17)
    draw_cloud(buf, size, size, size * 0.30, size * 0.58, size * 0.20, size * 0.035,
               (0xFF, 0xD9, 0xA0), 0.55)
    draw_cloud(buf, size, size, size * 0.74, size * 0.50, size * 0.16, size * 0.028,
               (0xFF, 0xC9, 0x90), 0.45)
    if rounded:
        cx = cy = size / 2
        r = size / 2 - 0.5
        for y in range(size):
            for x in range(size):
                d = ((x - cx) ** 2 + (y - cy) ** 2) ** 0.5
                cov = smoothstep(r + 1.0, r - 1.0, d)
                if cov < 1.0:
                    i = (y * size + x) * 4
                    buf[i + 3] = int(buf[i + 3] * cov)
    return buf


def for_background(size):
    buf = bytearray(size * size * 4)
    fill_sky(buf, size, size)
    return buf


def for_foreground(size):
    buf = bytearray(size * size * 4)
    draw_sun(buf, size, size, size / 2, size * 0.60, size * 0.13)
    draw_cloud(buf, size, size, size * 0.34, size * 0.50, size * 0.13, size * 0.022,
               (0xFF, 0xE2, 0xB0), 0.70)
    draw_cloud(buf, size, size, size * 0.66, size * 0.44, size * 0.11, size * 0.018,
               (0xFF, 0xD2, 0xA0), 0.60)
    return buf


def main():
    for name, scale in DENSITIES.items():
        d = os.path.join(ROOT, f"mipmap-{name}")
        os.makedirs(d, exist_ok=True)
        legacy = int(48 * scale)
        adaptive = int(108 * scale)
        write_png(os.path.join(d, "ic_launcher.png"), legacy, legacy, for_legacy(legacy, False))
        write_png(os.path.join(d, "ic_launcher_round.png"), legacy, legacy, for_legacy(legacy, True))
        write_png(os.path.join(d, "ic_launcher_background.png"), adaptive, adaptive, for_background(adaptive))
        write_png(os.path.join(d, "ic_launcher_foreground.png"), adaptive, adaptive, for_foreground(adaptive))
        print(f"mipmap-{name}: legacy={legacy} adaptive={adaptive}")

    anydpi = os.path.join(ROOT, "mipmap-anydpi-v26")
    os.makedirs(anydpi, exist_ok=True)
    adaptive_xml = (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
        '  <background android:drawable="@mipmap/ic_launcher_background" />\n'
        '  <foreground android:drawable="@mipmap/ic_launcher_foreground" />\n'
        '</adaptive-icon>\n'
    )
    with open(os.path.join(anydpi, "ic_launcher.xml"), "w") as f:
        f.write(adaptive_xml)
    with open(os.path.join(anydpi, "ic_launcher_round.xml"), "w") as f:
        f.write(adaptive_xml)
    print("adaptive xml written")


if __name__ == "__main__":
    main()
