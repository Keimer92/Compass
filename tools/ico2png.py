#!/usr/bin/env python3
"""Convierte un .ico a PNGs sin dependencias externas (solo stdlib).

Pop!_OS no trae ImageMagick ni icoutils, y el launcher de COSMIC necesita PNG
(freedesktop). Los .ico de este repo son de dos tipos:

  - `Icon.ico`: una sola entrada 128x128 codificada como BMP-DIB (sin PNG
    embebido). Hay que decodificar el BITMAPINFOHEADER y los píxeles BGRA
    (bottom-up) para reconstruir la imagen.
  - `Default.ico`: varias entradas ya comprimidas como PNG. Se extraen tal cual.

Salida: assets/icon/compass-<size>.png
"""

import struct
import sys
import zlib
from pathlib import Path

# Tamaños que el launcher y las barras de COSMIC suelen pedir.
TARGETS = (16, 24, 32, 48, 64, 128)


def read_ico_entries(data: bytes):
    """Devuelve [(width, height, payload, kind)] para cada entrada del .ico."""
    _res, _type, count = struct.unpack_from("<HHH", data, 0)
    entries = []
    for i in range(count):
        off = 6 + i * 16
        w, h, _ncol, _r1, _planes, bpp, size, dataoff = struct.unpack_from(
            "<BBBBHHII", data, off
        )
        entries.append((w or 256, h or 256, data[dataoff : dataoff + size], bpp))
    return entries


def decode_dib(payload: bytes):
    """Decodifica una entrada BMP-DIB de 32bpp a una lista de filas RGBA."""
    header_size = struct.unpack_from("<I", payload, 0)[0]
    width, height_half = struct.unpack_from("<ii", payload, 4)
    bit_count = struct.unpack_from("<H", payload, 14)[0]
    if bit_count != 32:
        raise ValueError(f"solo se soporta BMP-DIB de 32bpp (este es de {bit_count})")
    # En un ICO, la altura del header incluye la máscara AND: es el doble del alto real.
    height = height_half // 2

    pixels_off = struct.unpack_from("<I", payload, header_size - 4)[0]
    # Ese offset es relativo al inicio del DIB. En la práctica coincide con
    # header_size, pero lo leemos del header por si cambia.
    pixels_off = max(pixels_off, header_size)

    rows = []
    stride = width * 4
    for y in range(height):
        start = pixels_off + y * stride
        row = bytearray()
        for x in range(width):
            b, g, r, a = payload[start + x * 4 : start + x * 4 + 4]
            row += bytes((r, g, b, a))
        rows.append(row)
    # BMP es bottom-up: la primera fila del buffer es la de abajo.
    rows.reverse()
    return width, height, rows


def encode_png(width: int, height: int, rows) -> bytes:
    """Serializa filas RGBA a un PNG (color type 6, 8 bits por canal)."""
    raw = b"".join(b"\x00" + bytes(row) for row in rows)

    def chunk(tag: bytes, payload: bytes) -> bytes:
        body = tag + payload
        return struct.pack(">I", len(payload)) + body + struct.pack(
            ">I", zlib.crc32(body) & 0xFFFFFFFF
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def resize_nearest(rows, width, height, new_w, new_h):
    """Reduce con muestreo por caja (area promedio) para no perder el detalle."""
    out = []
    for y in range(new_h):
        y0, y1 = y * height // new_h, max((y + 1) * height // new_h, y * height // new_h + 1)
        row = bytearray()
        for x in range(new_w):
            x0 = x * width // new_w
            x1 = max((x + 1) * width // new_w, x0 + 1)
            r = g = b = a = n = 0
            for yy in range(y0, min(y1, height)):
                src = rows[yy]
                for xx in range(x0, min(x1, width)):
                    i = xx * 4
                    r += src[i]
                    g += src[i + 1]
                    b += src[i + 2]
                    a += src[i + 3]
                    n += 1
            if n:
                row += bytes((r // n, g // n, b // n, a // n))
        out.append(row)
    return out


def main():
    root = Path(__file__).resolve().parent.parent
    ico_path = root / "assets" / "icon" / "Icon.ico"
    out_dir = ico_path.parent
    data = ico_path.read_bytes()
    entries = read_ico_entries(data)

    base_w, base_h, rows = None, None, None
    # Preferimos la entrada más grande; si viene en PNG se re-decodifica igual
    # porque ya viene en RGBA y no hay entrada DIB mayor.
    for w, h, payload, _bpp in sorted(entries, key=lambda e: -e[0]):
        if payload[:8] == b"\x89PNG\r\n\x1a\n":
            # Extraemos el PNG tal cual y lo reescribimos en los tamaños pedidos
            # usando el mayor disponible como fuente.
            out_dir.joinpath(f"compass-{w}.png").write_bytes(payload)
            print(f"  extraido PNG {w}x{h} (embebido en el .ico)")
            if base_w is None:
                base_w, base_h = w, h
        else:
            base_w, base_h, rows = decode_dib(payload)

    if rows is None:
        sys.exit("Icon.ico no contiene ninguna entrada BMP-DIB de 32bpp")

    print(f"  fuente BMP-DIB {base_w}x{base_h}")
    for size in TARGETS:
        if size > base_w:
            continue
        out_rows = resize_nearest(rows, base_w, base_h, size, size)
        path = out_dir / f"compass-{size}.png"
        path.write_bytes(encode_png(size, size, out_rows))
        print(f"  escrito {path.name}")


if __name__ == "__main__":
    main()
