#!/usr/bin/env python3
"""Gera o tema de ícones do Kalango OS (pastas, lugares e dispositivos).

Pastas planas na cor de destaque com os mesmos ícones de traço do shell
(`ui/icons/`) por cima; computador, discos e lixeira em quadrados escuros com
o desenho na cor de destaque; arquivos como folhas escuras com a dobra e o
desenho do tipo (texto, código, imagem...) na cor de destaque. Uma variante por cor de destaque (Kalango-Ciano,
Kalango-Roxo...), que o shell escolhe junto com o destaque. Os demais ícones
(apps, tipos de arquivo) vêm do Adwaita (`Inherits`).

Uso: python3 distro/icones/gerar.py
Saída: distro/config/includes.chroot/usr/share/icons/Kalango-<Cor>/
"""

import re
import shutil
from pathlib import Path

RAIZ = Path(__file__).resolve().parents[2]
ICONES_UI = RAIZ / "ui/icons"
SAIDA = RAIZ / "distro/config/includes.chroot/usr/share/icons"

# Mesma ordem de Theme.accents (ui/theme.slint) e de ACCENTS (src/appearance.rs).
CORES = [
    ("Ciano", "#3dd6f5"),
    ("Roxo", "#a78bfa"),
    ("Azul", "#7ea6f7"),
    ("Verde", "#5fd38d"),
    ("Ambar", "#e8b04b"),
    ("Rosa", "#f28b95"),
]
FUNDO = "#0d1218"
TILE = "#131920"
BORDA = "#1e2a33"
TINTA = "#07090d"

# Desenhos extras no mesmo estilo dos ícones de ui/icons (24×24, traço).
EXTRAS = {
    "home": '<path d="m3 10.5 9-7 9 7V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z"/>',
    "video": '<rect x="3" y="5" width="18" height="14" rx="2"/><path d="m10 9 5 3-5 3z"/>',
    "share": '<circle cx="18" cy="5" r="2.5"/><circle cx="6" cy="12" r="2.5"/>'
    '<circle cx="18" cy="19" r="2.5"/><path d="m8.2 13.2 7.6 4.6M15.8 6.2l-7.6 4.6"/>',
    "drive": '<rect x="3" y="6" width="18" height="12" rx="2"/><path d="M3 13h18"/>'
    '<circle cx="17" cy="15.5" r=".6" fill="currentColor"/>',
    "usb": '<rect x="7" y="9" width="10" height="12" rx="1.5"/><path d="M9 9V3h6v6M10.5 5.5h0M13.5 5.5h0"/>',
    "disc": '<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="2.5"/>',
    "trash": '<path d="M4 7h16M9 7V4h6v3M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13"/>',
    "trash-full": '<path d="M4 7h16M9 7V4h6v3M6 7l1 13a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1l1-13M10 11v6M14 11v6"/>',
    "pdf": '<path d="M7 17V9h2.5a2 2 0 0 1 0 4H7M13 9v8h1.5a2.5 2.5 0 0 0 2.5-2.5v-3A2.5 2.5 0 0 0 14.5 9z"/>',
    "box": '<path d="m3 7 9-4 9 4v10l-9 4-9-4zM3 7l9 4 9-4M12 11v10"/>',
    "gear": '<circle cx="12" cy="12" r="3"/><path d="M12 2v3M12 19v3M4.9 4.9 7 7M17 17l2.1 2.1M2 12h3M19 12h3M4.9 19.1 7 17M17 7l2.1-2.1"/>',
    "star": '<path d="m12 3 2.7 5.6 6.1.9-4.4 4.3 1 6.1L12 17l-5.4 2.9 1-6.1-4.4-4.3 6.1-.9z"/>',
}

# Pastas: nome do ícone -> desenho (None = pasta lisa).
PASTAS = {
    "folder": None,
    "folder-open": None,
    "folder-drag-accept": None,
    "folder-visiting": None,
    "inode-directory": None,
    "user-home": "home",
    "folder-home": "home",
    "user-desktop": "monitor",
    "folder-desktop": "monitor",
    "folder-documents": "file-text",
    "folder-download": "download",
    "folder-downloads": "download",
    "folder-pictures": "image",
    "folder-images": "image",
    "folder-music": "music",
    "folder-videos": "video",
    "folder-templates": "file",
    "folder-publicshare": "share",
    "folder-remote": "globe",
    "user-bookmarks": "star",
}

# Lugares e dispositivos em quadrado escuro: nome -> (contexto, desenho).
TILES = {
    "computer": ("devices", "monitor"),
    "drive-harddisk": ("devices", "drive"),
    "drive-harddisk-system": ("devices", "drive"),
    "drive-harddisk-usb": ("devices", "usb"),
    "drive-removable-media": ("devices", "usb"),
    "drive-removable-media-usb": ("devices", "usb"),
    "media-removable": ("devices", "usb"),
    "drive-optical": ("devices", "disc"),
    "media-optical": ("devices", "disc"),
    "network-workgroup": ("places", "globe"),
    "user-trash": ("places", "trash"),
    "user-trash-full": ("places", "trash-full"),
}


# Arquivos: nome do ícone (tipo MIME) -> desenho.
ARQUIVOS = {
    "text-x-generic": "file-text",
    "text-plain": "file-text",
    "text-x-markdown": "file-text",
    "text-x-readme": "file-text",
    "x-office-document": "file-text",
    "application-x-shellscript": "terminal",
    "text-x-script": "code",
    "text-x-csrc": "code",
    "text-x-chdr": "code",
    "text-x-c++src": "code",
    "text-x-python": "code",
    "text-x-rust": "code",
    "text-x-java": "code",
    "text-x-javascript": "code",
    "application-javascript": "code",
    "application-json": "code",
    "application-xml": "code",
    "text-html": "globe",
    "text-css": "code",
    "image-x-generic": "image",
    "audio-x-generic": "music",
    "video-x-generic": "video",
    "application-pdf": "pdf",
    "package-x-generic": "box",
    "application-x-archive": "box",
    "application-zip": "box",
    "application-x-compressed-tar": "box",
    "application-vnd.debian.binary-package": "box",
    "application-x-executable": "gear",
    "application-x-sharedlib": "gear",
    "unknown": "file",
}


def desenho(nome):
    """Conteúdo interno (traços) de um ícone de ui/icons ou de EXTRAS."""
    if nome in EXTRAS:
        return EXTRAS[nome]
    texto = (ICONES_UI / f"{nome}.svg").read_text()
    return re.search(r"<svg[^>]*>(.*)</svg>", texto, re.S).group(1)


def misturar(a, b, t):
    """Mistura duas cores '#rrggbb' (t = 0 -> a, 1 -> b)."""
    ca = [int(a[i : i + 2], 16) for i in (1, 3, 5)]
    cb = [int(b[i : i + 2], 16) for i in (1, 3, 5)]
    return "#" + "".join(f"{round(x + (y - x) * t):02x}" for x, y in zip(ca, cb))


def tracos(conteudo, cor, x, y, escala, largura):
    return (
        f'<g transform="translate({x} {y}) scale({escala})" fill="none" stroke="{cor}" '
        f'stroke-width="{largura}" stroke-linecap="round" stroke-linejoin="round" color="{cor}">'
        f"{conteudo}</g>"
    )


def pasta(cor, glifo):
    """Pasta plana: aba e fundo mais escuros, frente na cor de destaque."""
    atras = misturar(cor, FUNDO, 0.45)
    frente_alto = misturar(cor, "#ffffff", 0.12)
    frente_baixo = misturar(cor, FUNDO, 0.12)
    corpo = (
        '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">'
        f'<defs><linearGradient id="f" x1="0" y1="0" x2="0" y2="1">'
        f'<stop offset="0" stop-color="{frente_alto}"/><stop offset="1" stop-color="{frente_baixo}"/>'
        "</linearGradient></defs>"
        f'<path d="M6 13a4 4 0 0 1 4-4h13.5a3 3 0 0 1 2.1.9L30 14h24a4 4 0 0 1 4 4v32a4 4 0 0 1-4 4H10a4 4 0 0 1-4-4z" fill="{atras}"/>'
        '<rect x="6" y="20" width="52" height="34" rx="4" fill="url(#f)"/>'
        f'<rect x="6" y="20" width="52" height="1.4" rx=".7" fill="#ffffff" opacity=".35"/>'
    )
    if glifo:
        corpo += tracos(desenho(glifo), TINTA, 22, 27, 0.84, 2.3)
    return corpo + "</svg>\n"


def tile(cor, glifo):
    """Quadrado escuro com borda e o desenho na cor de destaque."""
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">'
        f'<rect x="5" y="5" width="54" height="54" rx="12" fill="{TILE}" stroke="{BORDA}" stroke-width="1.5"/>'
        f'<rect x="5" y="5" width="54" height="54" rx="12" fill="{cor}" opacity=".10"/>'
        + tracos(desenho(glifo), cor, 14, 14, 1.5, 1.8)
        + "</svg>\n"
    )


def arquivo(cor, glifo):
    """Folha escura com a dobra no canto e o desenho do tipo na cor de destaque."""
    return (
        '<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">'
        f'<path d="M14 6h25l13 13v35a4 4 0 0 1-4 4H14a4 4 0 0 1-4-4V10a4 4 0 0 1 4-4z" fill="{TILE}" stroke="{BORDA}" stroke-width="1.5"/>'
        f'<path d="M14 6h25l13 13v35a4 4 0 0 1-4 4H14a4 4 0 0 1-4-4V10a4 4 0 0 1 4-4z" fill="{cor}" opacity=".08"/>'
        f'<path d="M39 6v9a4 4 0 0 0 4 4h9z" fill="{misturar(cor, TILE, 0.35)}"/>'
        + tracos(desenho(glifo), cor, 19, 26, 1.1, 2.0)
        + "</svg>\n"
    )


INDEX = """[Icon Theme]
Name=Kalango {nome}
Comment=Pastas e dispositivos do Kalango OS, na cor de destaque {nome}
Inherits=Adwaita,hicolor
Example=folder
Directories=places/scalable,devices/scalable,mimetypes/scalable

[places/scalable]
Context=Places
Size=64
MinSize=8
MaxSize=512
Type=Scalable

[devices/scalable]
Context=Devices
Size=64
MinSize=8
MaxSize=512
Type=Scalable

[mimetypes/scalable]
Context=MimeTypes
Size=64
MinSize=8
MaxSize=512
Type=Scalable
"""


def main():
    for nome, cor in CORES:
        tema = SAIDA / f"Kalango-{nome}"
        if tema.exists():
            shutil.rmtree(tema)
        for sub in ("places", "devices", "mimetypes"):
            (tema / sub / "scalable").mkdir(parents=True)
        (tema / "index.theme").write_text(INDEX.format(nome=nome))
        for icone, glifo in PASTAS.items():
            contexto = "mimetypes" if icone == "inode-directory" else "places"
            (tema / contexto / "scalable" / f"{icone}.svg").write_text(pasta(cor, glifo))
        for icone, glifo in ARQUIVOS.items():
            (tema / "mimetypes" / "scalable" / f"{icone}.svg").write_text(arquivo(cor, glifo))
        for icone, (contexto, glifo) in TILES.items():
            (tema / contexto / "scalable" / f"{icone}.svg").write_text(tile(cor, glifo))
        print("gerado", tema.relative_to(RAIZ))


if __name__ == "__main__":
    main()
