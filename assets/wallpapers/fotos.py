#!/usr/bin/env python3
"""Converte fotos para modelos de papel de parede do KalangOS.

Cada foto vira ``16x9/Kalango_<Modelo>_<LxA>.jpg`` nas três resoluções que a
área de trabalho usa (1920×1080, 2560×1440 e 3840×2160) mais a miniatura
``Kalango_<Modelo>_miniatura.jpg`` (384×216), como os modelos desenhados por
``gerar.py``. A lista de modelos fica em ``src/shell/wallpaper.rs`` (``MODELS``) e
os nomes na tela em ``ui/theme.slint`` (``wallpaper-names``).

O formato é lido pelo conteúdo, não pela extensão: as fotos do Sertão vieram
como WebP com o nome terminado em ``.jpg``. Fotos que não são 16:9 são cortadas
no meio; a ampliação é Lanczos com um pouco de nitidez (não inventa detalhe,
só evita a imagem borrada nas telas grandes). Os metadados XMP da foto
original são mantidos.

Uso::

    python3 assets/wallpapers/fotos.py PASTA_DAS_FOTOS

com a tabela ``FOTOS`` abaixo (arquivo de origem → nome do modelo).
Precisa do Pillow.
"""

import sys
from pathlib import Path

from PIL import Image, ImageFilter

# Arquivo na pasta de origem → nome do modelo (sem espaços nem acentos).
FOTOS = {
    "kalango-sertao-dia-por-sol.jpg": "SertaoPorDoSol",
    "kalango-sertao-noite-lua.jpg": "SertaoViaLactea",
    "lua chapada aurora.jpg": "ChapadaEstrelas",
    "lua chapada noite.jpg": "ChapadaLua",
    "pedra calango.jpg": "CalangoPedra",
    "por do sol chapada.jpg": "ChapadaPorDoSol",
    "por do sol sertao.jpg": "SertaoEntardecer",
}

TAMANHOS = [(1920, 1080), (2560, 1440), (3840, 2160)]
MINIATURA = (384, 216)
DESTINO = Path(__file__).resolve().parent / "16x9"


def recorte_16x9(img: Image.Image) -> Image.Image:
    """Corta a foto no meio para a proporção 16:9."""
    w, h = img.size
    alvo = 16 / 9
    if abs(w / h - alvo) < 0.01:
        return img
    if w / h > alvo:
        nw = round(h * alvo)
        x = (w - nw) // 2
        return img.crop((x, 0, x + nw, h))
    nh = round(w / alvo)
    y = (h - nh) // 2
    return img.crop((0, y, w, y + nh))


def redimensionar(img: Image.Image, tamanho: tuple[int, int]) -> Image.Image:
    """Lanczos; ampliando, um pouco de nitidez para compensar."""
    nova = img.resize(tamanho, Image.Resampling.LANCZOS)
    if tamanho[0] > img.size[0]:
        nova = nova.filter(ImageFilter.UnsharpMask(radius=1.2, percent=60, threshold=2))
    return nova


def gravar(img: Image.Image, caminho: Path, xmp: bytes | None) -> None:
    extra = {"xmp": xmp} if xmp else {}
    img.save(caminho, "JPEG", quality=90, optimize=True, progressive=True, **extra)


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    origem = Path(sys.argv[1])
    for arquivo, modelo in FOTOS.items():
        with Image.open(origem / arquivo) as foto:
            xmp = foto.info.get("xmp")
            if isinstance(xmp, str):
                xmp = xmp.encode()
            img = recorte_16x9(foto.convert("RGB"))
        print(f"{arquivo} ({img.size[0]}x{img.size[1]}) -> {modelo}")
        for w, h in TAMANHOS:
            gravar(redimensionar(img, (w, h)), DESTINO / f"Kalango_{modelo}_{w}x{h}.jpg", xmp)
        gravar(img.resize(MINIATURA, Image.Resampling.LANCZOS), DESTINO / f"Kalango_{modelo}_miniatura.jpg", xmp)


if __name__ == "__main__":
    main()
