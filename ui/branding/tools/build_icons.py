"""把 SVG 渲染出的 1024px PNG 母版打包成 Windows 图标资源。

用法（需要 Pillow）：
    python ui/branding/tools/build_icons.py --src <放有 1024px PNG 母版的目录>

母版文件名固定：fastnote-app-icon.png（完整版）、fastnote-app-icon-small.png（小尺寸简化版），
可选 fastnote-app-icon-mono.png / fastnote-app-icon-reversed.png。
渲染母版的方法见 ui/branding/README.md。

输出：
    ui/branding/fastnote.ico       Windows exe / 窗口 / 托盘图标
    ui/branding/png/*.png          各尺寸位图
"""

import argparse
import struct
import sys
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[3]
BRAND = ROOT / "ui" / "branding"

# 32px 及以下用简化版（细节会糊），40px 起用完整版。
SMALL_SIZES = [16, 20, 24, 32]
FULL_SIZES = [40, 48, 64, 96, 128, 256, 512, 1024]
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def render_set(src: Path) -> dict[int, Image.Image]:
    full = Image.open(src / "fastnote-app-icon.png").convert("RGBA")
    small = Image.open(src / "fastnote-app-icon-small.png").convert("RGBA")
    frames = {size: small.resize((size, size), Image.LANCZOS) for size in SMALL_SIZES}
    frames.update({size: full.resize((size, size), Image.LANCZOS) for size in FULL_SIZES})
    return frames


def write_ico(frames: dict[int, Image.Image], target: Path) -> None:
    """手写 ICO：每个尺寸放一张 PNG 帧（Vista+ 支持），这样可以混用两套绘制。"""
    blobs: list[tuple[int, bytes]] = []
    for size in ICO_SIZES:
        buf = target.parent / f".tmp-{size}.png"
        frames[size].save(buf, format="PNG", optimize=True)
        blobs.append((size, buf.read_bytes()))
        buf.unlink()

    header = struct.pack("<HHH", 0, 1, len(blobs))
    offset = 6 + 16 * len(blobs)
    entries = b""
    payload = b""
    for size, blob in blobs:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(blob), offset)
        offset += len(blob)
        payload += blob
    target.write_bytes(header + entries + payload)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--src", type=Path, required=True, help="1024px 母版 PNG 所在目录")
    args = parser.parse_args()

    frames = render_set(args.src)

    (BRAND / "png").mkdir(parents=True, exist_ok=True)
    for size in sorted(frames):
        frames[size].save(BRAND / "png" / f"fastnote-{size}.png", format="PNG", optimize=True)

    write_ico(frames, BRAND / "fastnote.ico")

    for variant in ("mono", "reversed"):
        master = args.src / f"fastnote-app-icon-{variant}.png"
        if master.exists():
            Image.open(master).convert("RGBA").resize((512, 512), Image.LANCZOS).save(
                BRAND / "png" / f"fastnote-{variant}-512.png", format="PNG", optimize=True
            )

    print("ok:", ", ".join(f"{size}px" for size in sorted(frames)))
    return 0


if __name__ == "__main__":
    sys.exit(main())