#!/usr/bin/env python3
"""Extrai sprites 64x64 @ 1bpp do sketch Arduino do Bongo Cat.

Fonte: https://github.com/sidharth-458/Bongo_cat-32/blob/main/sketch_sep25a.ino
Autor original dos sprites: @pixelbunny (citado no README do repo acima).
Licença: não declarada — ver NOTICE.md.

Uso (rodar uma vez, commitar os .bin gerados):
    python3 tools/extract_bongo_sprites.py \\
        --input  /tmp/bongo_sketch.ino \\
        --output dongle/src/assets/bongo/

Só os frames 0..12 são usados pela máquina de estados (idle 0..8, tap 9..12).
Os frames 13..19 existem no sketch mas são código morto; não são extraídos.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import sys


# Regex captura "const unsigned char PROGMEM frameN[] = { ...bytes... };"
FRAME_PATTERN = re.compile(
    r"const\s+unsigned\s+char\s+(?:PROGMEM\s+)?frame(\d+)\s*\[\s*\]\s*=\s*\{([^}]+)\}",
    re.MULTILINE,
)

BYTE_PATTERN = re.compile(r"0[xX][0-9a-fA-F]+")

FRAME_BYTES = 64 * 64 // 8  # 64x64 @ 1bpp = 512 bytes
FRAMES_TO_EXTRACT = range(0, 13)


def parse_frames(source: str) -> dict[int, bytes]:
    """Retorna {idx: bytes} pra cada array frame<idx> encontrado no .ino."""
    frames: dict[int, bytes] = {}
    for match in FRAME_PATTERN.finditer(source):
        idx = int(match.group(1))
        raw_bytes = [int(v, 16) for v in BYTE_PATTERN.findall(match.group(2))]
        frames[idx] = bytes(raw_bytes)
    return frames


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", required=True, type=pathlib.Path)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()

    source = args.input.read_text()
    frames = parse_frames(source)

    args.output.mkdir(parents=True, exist_ok=True)

    for idx in FRAMES_TO_EXTRACT:
        if idx not in frames:
            print(f"WARN: frame{idx} não encontrado no source", file=sys.stderr)
            continue
        data = frames[idx]
        if len(data) != FRAME_BYTES:
            print(
                f"WARN: frame{idx} tem {len(data)} bytes, esperado {FRAME_BYTES}",
                file=sys.stderr,
            )
        out_path = args.output / f"frame_{idx:02d}.bin"
        out_path.write_bytes(data)
        print(f"  {out_path.name}  ({len(data)} bytes)")

    print(f"\nextraídos {len(FRAMES_TO_EXTRACT)} frames em {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
