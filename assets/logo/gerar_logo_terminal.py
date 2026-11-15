#!/usr/bin/env python3
"""Logotipo do KalangOS para o terminal (fastfetch), em blocos coloridos.

Cada caractere "▀" desenha dois pixels: o de cima na cor do texto e o de
baixo na cor do fundo (cores de 24 bits). Assim o ícone sai com pixels
quadrados em qualquer terminal moderno, inclusive pela SSH, sem precisar de
suporte a imagens (sixel/kitty). Os cantos transparentes do ícone ficam com o
fundo do próprio terminal.

Uso: python3 assets/logo/gerar_logo_terminal.py
Gera distro/config/includes.chroot/usr/share/kalango/fastfetch/logo.txt
(28 colunas × 14 linhas; mude LADO para outro tamanho e ajuste o
config.jsonc do fastfetch).
"""

from pathlib import Path

from PIL import Image

LADO = 28  # pixels = colunas; linhas = LADO / 2
RAIZ = Path(__file__).resolve().parents[2]
ICONE = RAIZ / "assets/logo/Kalango_Icone_Noite.png"
SAIDA = RAIZ / "distro/config/includes.chroot/usr/share/kalango/fastfetch/logo.txt"


def reduzir(img: Image.Image, lado: int) -> Image.Image:
    """Reduz em duas etapas (o Pillow falha ao reduzir 1024 → 28 de uma vez)."""
    while img.width >= lado * 4:
        img = img.reduce(2)
    return img.resize((lado, lado), Image.LANCZOS)


def main() -> None:
    img = reduzir(Image.open(ICONE).convert("RGBA"), LADO)
    px = img.load()

    def cor(x: int, y: int):
        r, g, b, a = px[x, y]
        return None if a < 128 else (r, g, b)

    linhas = []
    for y in range(0, LADO, 2):
        partes = []
        for x in range(LADO):
            cima, baixo = cor(x, y), cor(x, y + 1)
            if cima is None and baixo is None:
                partes.append("\x1b[0m ")
            elif cima is None:
                partes.append("\x1b[0m\x1b[38;2;%d;%d;%dm▄" % baixo)
            elif baixo is None:
                partes.append("\x1b[0m\x1b[38;2;%d;%d;%dm▀" % cima)
            else:
                partes.append("\x1b[38;2;%d;%d;%dm\x1b[48;2;%d;%d;%dm▀" % (cima + baixo))
        linhas.append("".join(partes) + "\x1b[0m")

    SAIDA.parent.mkdir(parents=True, exist_ok=True)
    SAIDA.write_text("\n".join(linhas) + "\n", encoding="utf-8")
    print(f"gerado {SAIDA.relative_to(RAIZ)} ({LADO}×{LADO // 2})")


if __name__ == "__main__":
    main()
