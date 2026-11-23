#!/usr/bin/env python3
# gen_icon.py —— 生成应用图标（覆盖 icons/icon.png）
# 依赖 Pillow：pip3 install Pillow
# 用法：python3 gen_icon.py
import os

from PIL import Image, ImageDraw, ImageFont

SIZE = 1024
RADIUS = 200
OUT = "icons/icon.png"

# 渐变背景色（上 → 下）
TOP = (79, 70, 229)     # #4F46E5 indigo
BOT = (6, 182, 212)     # #06B6D4 cyan


def lerp(a, b, t):
    return int(a * (1 - t) + b * t)


def main():
    img = Image.new("RGBA", (SIZE, SIZE))
    draw = ImageDraw.Draw(img)
    for y in range(SIZE):
        t = y / (SIZE - 1)
        draw.line([(0, y), (SIZE, y)], fill=(lerp(TOP[0], BOT[0], t), lerp(TOP[1], BOT[1], t), lerp(TOP[2], BOT[2], t), 255))

    # 圆角遮罩
    mask = Image.new("L", (SIZE, SIZE), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, SIZE - 1, SIZE - 1], radius=RADIUS, fill=255)
    out = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    out.paste(img, (0, 0), mask)

    # 文字 N&B，居中
    text = "N&B"
    font = None
    for path in (
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "/Library/Fonts/Arial Bold.ttf",
    ):
        try:
            font = ImageFont.truetype(path, 360)
            break
        except OSError:
            continue
    if font is None:
        font = ImageFont.load_default()

    draw = ImageDraw.Draw(out)
    bbox = draw.textbbox((0, 0), text, font=font)
    tw, th = bbox[2] - bbox[0], bbox[3] - bbox[1]
    x = (SIZE - tw) / 2 - bbox[0]
    y = (SIZE - th) / 2 - bbox[1]
    draw.text((x + 5, y + 5), text, font=font, fill=(0, 0, 0, 90))   # 阴影
    draw.text((x, y), text, font=font, fill=(255, 255, 255, 255))

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    out.save(OUT)
    print(f"生成 {OUT}（{SIZE}x{SIZE}）")


if __name__ == "__main__":
    main()
