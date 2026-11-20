#!/usr/bin/env python3
"""Converte fotos para modelos de papel de parede do KalangOS.

Cada foto vira ``16x9/Kalango_<Modelo>_<LxA>.jpg`` nas três resoluções que a
área de trabalho usa (1920×1080, 2560×1440 e 3840×2160) mais a miniatura
``Kalango_<Modelo>_miniatura.jpg`` (384×216), como os modelos desenhados por
``gerar.py``. A lista de modelos fica em ``src/shell/wallpaper.rs`` (``MODELS``) e
os nomes na tela em ``ui/theme.slint`` (``wallpaper-names``).

O formato é lido pelo conteúdo, não pela extensão: as fotos do Sertão vieram
como WebP com o nome terminado em ``.jpg``. Fotos que não são 16:9 são cortadas
no meio. Os metadados XMP da foto original são mantidos.

**Fotos menores que 3840 px** (as do Sertão e da Chapada têm ~2048 px e vieram
comprimidas) ficam borradas e com blocos se forem só ampliadas. Antes, amplie
4× com o Real-ESRGAN (código aberto, roda na placa de vídeo pelo Vulkan), que
também limpa a compressão, e passe a pasta com os PNG ampliados:

    realesrgan-ncnn-vulkan -i "foto.png" -o "ampliadas/foto.png" -n realesrgan-x4plus

(versão usada: realesrgan-ncnn-vulkan-20220424-ubuntu.zip, SHA-256
e5aa6eb131234b87c0c51f82b89390f5e3e642b7b70f2b9bbe95b6a285a40c96). O script
procura primeiro ``<nome sem extensão>.png`` na pasta (a ampliada) e depois o
arquivo de origem. Daí para as resoluções finais é só redução (Lanczos), que
fica nítida; a nitidez extra só entra quando ainda for preciso ampliar.

Uso::

    python3 assets/wallpapers/fotos.py PASTA_DAS_FOTOS_OU_AMPLIADAS [PASTA_DOS_ORIGINAIS]

A segunda pasta só serve para copiar os metadados XMP dos originais.
Tabela ``FOTOS`` abaixo: arquivo de origem → nome do modelo. Precisa do Pillow.
Depois de gerar, suba a versão em ``distro/pacotes/gerar-deb-papeis.sh``.
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


def fonte(pasta: Path, arquivo: str) -> Path:
    """A versão ampliada (``<nome>.png``) se existir; senão o arquivo de origem."""
    ampliada = pasta / (Path(arquivo).stem + ".png")
    return ampliada if ampliada.exists() else pasta / arquivo


def xmp_de(caminho: Path) -> bytes | None:
    """Metadados XMP de uma imagem (o Real-ESRGAN não os copia)."""
    if not caminho.exists():
        return None
    with Image.open(caminho) as img:
        xmp = img.info.get("xmp")
    return xmp.encode() if isinstance(xmp, str) else xmp


def main() -> None:
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    origem = Path(sys.argv[1])
    originais = Path(sys.argv[2]) if len(sys.argv) == 3 else origem
    for arquivo, modelo in FOTOS.items():
        xmp = xmp_de(originais / arquivo) or xmp_de(origem / arquivo)
        with Image.open(fonte(origem, arquivo)) as foto:
            img = recorte_16x9(foto.convert("RGB"))
        print(f"{arquivo} ({img.size[0]}x{img.size[1]}) -> {modelo}")
        for w, h in TAMANHOS:
            gravar(redimensionar(img, (w, h)), DESTINO / f"Kalango_{modelo}_{w}x{h}.jpg", xmp)
        gravar(img.resize(MINIATURA, Image.Resampling.LANCZOS), DESTINO / f"Kalango_{modelo}_miniatura.jpg", xmp)


if __name__ == "__main__":
    main()
