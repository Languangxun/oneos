#!/usr/bin/env python3
"""Generate crates/oneos-splash/src/signature.data from a TTF/OTF font.

Apple "Hello" / InkTrail style: keep the real filled glyph outlines and let the
player reveal them with a thick ink stroke travelling along the contours.

Usage:
    pip install --user fonttools
    python3 tools/gen-signature.py --text OneOS --font tools/fonts/Caveat.ttf
"""

import argparse
import pathlib

from fontTools.pens.recordingPen import RecordingPen
from fontTools.ttLib import TTFont


def flatten_quadratic(p0, p1, p2, out, steps=8):
    for i in range(1, steps + 1):
        t = i / steps
        mt = 1.0 - t
        out.append(
            (
                mt * mt * p0[0] + 2 * mt * t * p1[0] + t * t * p2[0],
                mt * mt * p0[1] + 2 * mt * t * p1[1] + t * t * p2[1],
            )
        )


def flatten_cubic(p0, p1, p2, p3, out, steps=12):
    for i in range(1, steps + 1):
        t = i / steps
        mt = 1.0 - t
        out.append(
            (
                mt**3 * p0[0] + 3 * mt * mt * t * p1[0] + 3 * mt * t * t * p2[0] + t**3 * p3[0],
                mt**3 * p0[1] + 3 * mt * mt * t * p1[1] + 3 * mt * t * t * p2[1] + t**3 * p3[1],
            )
        )


def midpoint(a, b):
    return ((a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0)


def draw_glyph_contours(glyph_set, glyph_name):
    pen = RecordingPen()
    glyph_set[glyph_name].draw(pen)

    contours = []
    current = []
    start = None
    last = None

    for operator, points in pen.value:
        if operator == "moveTo":
            if current:
                contours.append(current)
            current = [points[0]]
            start = points[0]
            last = points[0]
        elif operator == "lineTo":
            current.append(points[0])
            last = points[0]
        elif operator == "qCurveTo":
            off_curve = [point for point in points[:-1] if point is not None]
            end = points[-1]
            if end is None:
                # TrueType special case: all points are off-curve, close by
                # wrapping around the implied midpoint contour.
                end = midpoint(off_curve[0], off_curve[-1]) if off_curve else last
            curve_start = last
            for index, control in enumerate(off_curve):
                if index + 1 < len(off_curve):
                    next_point = midpoint(control, off_curve[index + 1])
                else:
                    next_point = end
                flatten_quadratic(curve_start, control, next_point, current)
                curve_start = next_point
            if not off_curve:
                current.append(end)
            last = end
        elif operator == "curveTo":
            control1, control2, end = points
            flatten_cubic(last, control1, control2, end, current)
            last = end
        elif operator == "closePath":
            if current:
                contours.append(current)
            current = []
            last = start
        elif operator == "endPath":
            if current:
                contours.append(current)
            current = []

    if current:
        contours.append(current)
    return contours


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--text", default="OneOS")
    parser.add_argument("--font", default="tools/fonts/Caveat.ttf")
    parser.add_argument("--output", default="crates/oneos-splash/src/signature.data")
    args = parser.parse_args()

    font = TTFont(args.font)
    glyph_set = font.getGlyphSet()
    cmap = font.getBestCmap()
    hmtx = font["hmtx"]
    units_per_em = font["head"].unitsPerEm

    positioned = []
    cursor = 0.0
    for char in args.text:
        glyph_name = cmap.get(ord(char))
        if glyph_name is None:
            continue
        contours = draw_glyph_contours(glyph_set, glyph_name)
        positioned.append((contours, cursor))
        cursor += hmtx[glyph_name][0]

    xs = [x + offset for contours, offset in positioned for contour in contours for x, _ in contour]
    ys = [y for contours, _offset in positioned for contour in contours for _, y in contour]
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)
    width = max_x - min_x
    height = max_y - min_y
    scale = 1.0 / max(width, height)

    lines = [f"# {args.text} / {pathlib.Path(args.font).name} (OFL)"]
    lines.append(f"# em {units_per_em * scale:.5f}")

    for glyph_index, (contours, offset) in enumerate(positioned):
        lines.append(f"G {glyph_index}")
        for contour in contours:
            points = []
            for x, y in contour:
                nx = (x + offset - min_x) * scale - width * scale / 2.0
                ny = -((y - min_y) * scale - height * scale / 2.0)
                points.append(f"{nx:.5f} {ny:.5f}")
            lines.append("C " + " ".join(points))

    output = pathlib.Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("\n".join(lines) + "\n")

    total_points = sum(len(contour) for contours, _ in positioned for contour in contours)
    print(f"wrote {output}: {len(positioned)} glyphs, {total_points} points")


if __name__ == "__main__":
    main()
