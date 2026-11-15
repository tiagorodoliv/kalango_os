#!/usr/bin/env python3
"""Gera o tema gráfico do GRUB do Kalango OS.

Fundo do papel de parede Preto e azul (sem o logotipo no meio, onde fica o
menu), o logotipo pequeno no alto, menu centralizado com um ícone por tipo
de entrada (classe do GRUB: kalango, windows, gnu-linux, efi, memtest...),
caixa de seleção arredondada com borda ciano e as fontes Inter e JetBrains
Mono convertidas para o formato do GRUB (.pf2, pelo grub-mkfont).

Uso (precisa de numpy, Pillow e grub-mkfont):
    python3 distro/grub/gerar-tema.py
Saída: distro/config/includes.chroot/boot/grub/themes/kalango/ (o sistema
instalado copia junto com o resto do /boot) e
distro/config/bootloaders/grub-pc/live-theme/ (o menu da ISO).
"""

import math
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter

RAIZ = Path(__file__).resolve().parents[2]
SAIDA = RAIZ / "distro/config/includes.chroot/boot/grub/themes/kalango"
LIVE = RAIZ / "distro/config/bootloaders/grub-pc/live-theme"
FONTES = RAIZ / "ui/fonts"

# Cores do shell (ui/theme.slint).
TEXTO = (230, 237, 243)
MUDO = (139, 152, 165)
DESTAQUE = (61, 214, 245)
PAINEL = (13, 18, 24)
TILE = (19, 25, 32)
BORDA = (30, 42, 51)


def fundo():
    """Papel de parede Preto e azul em 1920x1080, sem o logotipo no centro."""
    sys.argv = [sys.argv[0], "--previa"]
    sys.path.insert(0, str(RAIZ / "assets/wallpapers"))
    import gerar  # noqa: E402  (o módulo lê --previa ao importar)

    vazio = Image.new("RGBA", (1, 1), (0, 0, 0, 0))
    img = gerar.gerar(gerar.MODELOS["PretoAzul"], vazio, gerar.ondas(), gerar.hexagonos())
    # Escurece o miolo para o menu ficar legível.
    w, h = img.size
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32)
    d = np.sqrt(((xx - w / 2) / (w * 0.32)) ** 2 + ((yy - h * 0.55) / (h * 0.42)) ** 2)
    k = np.clip(1 - d, 0, 1) ** 1.5 * 0.45
    arr = np.asarray(img.convert("RGB")).astype(np.float32) * (1 - k[..., None])
    return Image.fromarray(arr.astype(np.uint8), "RGB")


def logotipo():
    """Logotipo (letras brancas) com 480 px de largura."""
    img = Image.open(RAIZ / "assets/logo/Kalango_Logo_transparente.png").convert("RGBA")
    largura = 480
    return img.resize((largura, round(img.height * largura / img.width)), Image.LANCZOS)


def ladrilho(tamanho=128):
    """Fundo dos ícones: quadrado arredondado escuro com borda (como o R1)."""
    img = Image.new("RGBA", (tamanho, tamanho), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    r = int(tamanho * 0.22)
    d.rounded_rectangle((2, 2, tamanho - 3, tamanho - 3), radius=r, fill=TILE + (255,), outline=BORDA + (255,), width=3)
    return img, d


def icone_windows():
    img, d = ladrilho()
    # Quatro painéis em perspectiva leve (ícone genérico de janela).
    c, g, s = 64, 6, 26
    cor = (126, 166, 247, 255)
    for i in (-1, 1):
        for j in (-1, 1):
            x0 = c + (g // 2 if i > 0 else -g // 2 - s)
            y0 = c + (g // 2 if j > 0 else -g // 2 - s)
            d.rounded_rectangle((x0, y0, x0 + s, y0 + s), radius=3, fill=cor)
    return img


def icone_terminal():
    img, d = ladrilho()
    d.line([(38, 46), (58, 64), (38, 82)], fill=DESTAQUE + (255,), width=8, joint="curve")
    d.line([(66, 84), (92, 84)], fill=TEXTO + (255,), width=8)
    return img


def icone_engrenagem():
    img, d = ladrilho()
    c, r1, r2 = 64, 26, 36
    pts = []
    for k in range(16):
        ang = math.radians(k * 22.5)
        r = r2 if k % 2 == 0 else r1
        for da in (-7, 7):
            a = ang + math.radians(da)
            pts.append((c + r * math.cos(a), c + r * math.sin(a)))
    d.polygon(pts, fill=TEXTO + (255,))
    d.ellipse((c - 12, c - 12, c + 12, c + 12), fill=TILE + (255,))
    return img


def icone_memoria():
    img, d = ladrilho()
    d.rounded_rectangle((36, 40, 92, 88), radius=6, outline=TEXTO + (255,), width=6)
    for x in range(44, 92, 12):
        d.line([(x, 30), (x, 40)], fill=DESTAQUE + (255,), width=5)
        d.line([(x, 88), (x, 98)], fill=DESTAQUE + (255,), width=5)
    return img


def icone_kalango():
    """O ícone escolhido (R1), do gerador do ícone."""
    arq = RAIZ / "assets/logo/Kalango_Icone_Noite.png"
    return Image.open(arq).convert("RGBA").resize((128, 128), Image.LANCZOS)


def selecao(altura=60, raio=14):
    """Caixa da entrada escolhida: só as pontas (w/e) e o meio (c), na altura
    exata da entrada. Com cantos em cima e embaixo (n/s), o GRUB desloca o
    texto da entrada escolhida pela altura deles e ela encavala na seguinte."""
    largura = 2 * raio + 1
    img = Image.new("RGBA", (largura, altura), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    d.rounded_rectangle((0, 0, largura - 1, altura - 1), radius=raio, fill=DESTAQUE + (36,), outline=DESTAQUE + (230,), width=2)
    return {
        "w": img.crop((0, 0, raio, altura)),
        "c": img.crop((raio, 0, raio + 1, altura)),
        "e": img.crop((raio + 1, 0, largura, altura)),
    }


def fontes(destino):
    """Converte as fontes do shell para .pf2 (grub-mkfont)."""
    faixa = "0x20-0x7E,0xA0-0x17F,0x2022-0x2022,0x2190-0x2193,0x21B5-0x21B5"
    lista = [
        ("Inter-Regular.ttf", 22, "inter-22.pf2"),
        ("Inter-Bold.ttf", 22, "inter-bold-22.pf2"),
        ("Inter-Regular.ttf", 16, "inter-16.pf2"),
        ("JetBrainsMono-Regular.ttf", 18, "jetbrains-mono-18.pf2"),
    ]
    for fonte, tam, saida in lista:
        subprocess.run(
            ["grub-mkfont", "-s", str(tam), "-r", faixa, "-o", str(destino / saida), str(FONTES / fonte)],
            check=True,
            stderr=subprocess.DEVNULL,  # avisos de recursos da fonte que o GRUB não usa
        )


TEMA = """# Tema do GRUB do Kalango OS (gerado por distro/grub/gerar-tema.py).
# Ícones por classe da entrada em icons/ (kalango = o próprio sistema).
title-text: ""
desktop-image: "background.png"
desktop-color: "#07090d"
terminal-font: "JetBrains Mono Regular 18"
terminal-box: "terminal_box_*.png"
terminal-left: "10%"
terminal-top: "12%"
terminal-width: "80%"
terminal-height: "76%"

+ image {
    left = 50%-240
    top = 9%
    width = 480
    height = 160
    file = "logo.png"
}

+ boot_menu {
    left = 50%-380
    top = 34%
    width = 760
    height = 44%
    item_font = "Inter Regular 22"
    selected_item_font = "Inter Bold 22"
    item_color = "#c9d2de"
    selected_item_color = "#ffffff"
    icon_width = 44
    icon_height = 44
    item_icon_space = 18
    item_height = 60
    item_padding = 0
    item_spacing = 12
    selected_item_pixmap_style = "select_*.png"
    scrollbar = true
    scrollbar_width = 6
    scrollbar_thumb = "barra_*.png"
}

# Contagem até iniciar a entrada escolhida (some ao apertar uma tecla).
# Sem barra: o GRUB ignora a altura da progress_bar e ela saía grossa.
+ label {
    id = "__timeout__"
    left = 50%-200
    top = 100%-96
    width = 400
    height = 24
    align = "center"
    font = "Inter Regular 16"
    color = "#3dd6f5"
    text = "%d s"
}

+ label {
    left = 0
    top = 100%-48
    width = 100%
    align = "center"
    font = "Inter Regular 16"
    color = "#8b98a5"
    text = "↑ ↓   •   Enter   •   E: editar / edit   •   C: comandos / command line"
}
"""


def main():
    SAIDA.mkdir(parents=True, exist_ok=True)
    (SAIDA / "icons").mkdir(exist_ok=True)
    fundo().save(SAIDA / "background.png", optimize=True)
    logotipo().save(SAIDA / "logo.png", optimize=True)
    icones = {
        "kalango": icone_kalango(),
        "windows": icone_windows(),
        "gnu-linux": icone_terminal(),
        "os": icone_terminal(),
        "efi": icone_engrenagem(),
        "memtest86": icone_memoria(),
        "memtest": icone_memoria(),
    }
    for nome, img in icones.items():
        img.save(SAIDA / "icons" / f"{nome}.png", optimize=True)
    for velho in SAIDA.glob("select_*.png"):
        velho.unlink()
    for nome, img in selecao().items():
        img.save(SAIDA / f"select_{nome}.png")
    # Caixa do terminal (linha de comando e edição): painel escuro liso.
    for nome in ("nw", "n", "ne", "w", "c", "e", "sw", "s", "se"):
        Image.new("RGBA", (8, 8), PAINEL + (235,)).save(SAIDA / f"terminal_box_{nome}.png")
    # Barra de rolagem fina.
    for nome in ("n", "c", "s"):
        Image.new("RGBA", (6, 6), BORDA + (255,)).save(SAIDA / f"barra_{nome}.png")
    fontes(SAIDA)
    (SAIDA / "theme.txt").write_text(TEMA)
    # Menu da ISO: mesma pasta.
    if LIVE.exists():
        shutil.rmtree(LIVE)
    shutil.copytree(SAIDA, LIVE)
    print("tema em", SAIDA, "e", LIVE)


if __name__ == "__main__":
    main()
