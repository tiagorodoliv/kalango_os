#!/usr/bin/env python3
"""Papéis de parede dos lançamentos do KalangOS: o sertão nordestino.

Cada série tem o nome de uma planta da caatinga (distro/codinomes) e ganha um
papel de parede com ela: pôr do sol no sertão (céu violeta para laranja), sol
baixo, serras em camadas, chão de barro e a planta em silhueta, com as fitas de
luz dos papéis de parede do KalangOS atravessando o céu.

Só precisa do Pillow (sem numpy).

Uso:
    python3 assets/wallpapers/sertao/gerar.py [modelo] [pasta_de_saída]
    modelo: mandacaru (padrão)
Saída: Kalango_<Modelo>_<largura>x<altura>.png em 3840x2160, 2560x1440 e
1920x1080, e Kalango_<Modelo>_miniatura.png (384x216).
"""

import math
import random
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

W, H = 3840, 2160
TAMANHOS = [(3840, 2160), (2560, 1440), (1920, 1080)]

# Paleta de cada lançamento: céu (de cima para o horizonte), sol, serras (da
# mais longe para a mais perto), chão, silhueta, fitas de luz e destaque (a cor
# de destaque sugerida para a interface).
MODELOS = {
    "mandacaru": {
        "ceu": ["#140f2b", "#3a1f4f", "#8a3b5c", "#d9663a", "#f2a54a"],
        "sol": "#ffd27a",
        "serras": ["#6b3550", "#4a2440", "#2e1830"],
        "chao": ["#3a1c1c", "#24110f"],
        "silhueta": "#120a10",
        "fita": "#ffb35c",
        "destaque": "#5fd38d",
    },
}


def rgb(hexa):
    hexa = hexa.lstrip("#")
    return tuple(int(hexa[i : i + 2], 16) for i in (0, 2, 4))


def mistura(a, b, t):
    return tuple(round(a[i] + (b[i] - a[i]) * t) for i in range(3))


def gradiente(img, cores, y0, y1):
    """Gradiente vertical entre y0 e y1 passando pelas cores dadas."""
    d = ImageDraw.Draw(img)
    cs = [rgb(c) for c in cores]
    for y in range(y0, y1):
        t = (y - y0) / max(1, y1 - y0 - 1) * (len(cs) - 1)
        i = min(int(t), len(cs) - 2)
        d.line([(0, y), (W, y)], fill=mistura(cs[i], cs[i + 1], t - i))


def sol(img, cor, cx, cy, r):
    """Sol baixo com halo."""
    halo = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(halo)
    c = rgb(cor)
    for k, a in ((3.2, 40), (2.2, 60), (1.5, 90)):
        d.ellipse([cx - r * k, cy - r * k, cx + r * k, cy + r * k], fill=c + (a,))
    halo = halo.filter(ImageFilter.GaussianBlur(r * 0.9))
    img.alpha_composite(halo)
    d = ImageDraw.Draw(img)
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=c + (255,))


def serra(img, cor, base, altura, seed, picos):
    """Uma camada de serra: cumes suaves (soma de ondas) até o chão."""
    rnd = random.Random(seed)
    ondas = [(rnd.uniform(0.8, 1.6) * picos / 6, rnd.uniform(0, 6.3), a) for a in (0.45, 0.25, 0.12, 0.06)]
    pts = [(0, H)]
    for x in range(0, W + 20, 20):
        v = sum(a * math.sin(x / W * f * 2 * math.pi + fase) for f, fase, a in ondas)
        pts.append((x, base - altura * (0.55 + v * 0.8)))
    pts.append((W, H))
    ImageDraw.Draw(img).polygon(pts, fill=rgb(cor) + (255,))


def estrelas(img, ate_y, seed):
    """Primeiras estrelas no alto do céu."""
    rnd = random.Random(seed)
    d = ImageDraw.Draw(img)
    for _ in range(140):
        x, y = rnd.uniform(0, W), rnd.uniform(0, ate_y) ** 1.0
        if rnd.random() > (1 - y / ate_y) ** 1.5:
            continue
        r = rnd.choice((1.5, 2, 2, 3))
        a = rnd.randint(90, 220)
        d.ellipse([x - r, y - r, x + r, y + r], fill=(255, 240, 220, a))


def fitas(img, cor):
    """Fitas de luz atravessando o céu (identidade dos papéis do KalangOS)."""
    camada = Image.new("RGBA", img.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(camada)
    c = rgb(cor)
    for i, (amp, fase, y0, larg, alfa) in enumerate(
        [(170, 0.0, 520, 5, 150), (210, 1.3, 600, 3, 110), (140, 2.4, 470, 2, 90)]
    ):
        pts = [(x, y0 + amp * math.sin(x / W * 2.6 * math.pi + fase) + 40 * math.sin(x / W * 9 + i)) for x in range(-40, W + 40, 12)]
        d.line(pts, fill=c + (alfa,), width=larg, joint="curve")
    brilho = camada.filter(ImageFilter.GaussianBlur(14))
    img.alpha_composite(brilho)
    img.alpha_composite(camada)


def braco(d, cor, x, y_base, y_topo, larg):
    """Um gomo vertical de cacto com ponta arredondada."""
    d.rounded_rectangle([x - larg / 2, y_topo, x + larg / 2, y_base], radius=larg / 2, fill=cor)


def mandacaru(img, cor, x, chao, altura, seed):
    """Silhueta do mandacaru: tronco alto e braços curtos que sobem colados a ele."""
    rnd = random.Random(seed)
    d = ImageDraw.Draw(img)
    c = rgb(cor) + (255,)
    larg = altura * 0.085
    topo = chao - altura
    braco(d, c, x, chao + 10, topo, larg)
    for lado in (-1, 1):
        for k in range(rnd.choice((1, 2))):
            y_saida = chao - altura * rnd.uniform(0.32, 0.6)
            lb = larg * rnd.uniform(0.78, 0.92)
            dist = larg * 0.55 + lb * rnd.uniform(0.9, 1.3)
            xb = x + lado * dist
            # Cotovelo curto e redondo (o braço nasce quase encostado ao tronco).
            d.rounded_rectangle(
                [min(x, xb) - lb / 2, y_saida - lb * 0.55, max(x, xb) + lb / 2, y_saida + lb * 0.55], radius=lb / 2, fill=c
            )
            braco(d, c, xb, y_saida, y_saida - altura * rnd.uniform(0.28, 0.42), lb)
    # Costelas: frisos claros finos ao longo do tronco (só se notam de perto).
    friso = tuple(min(255, v + 18) for v in rgb(cor)) + (255,)
    for dx in (-larg * 0.18, larg * 0.18):
        d.line([(x + dx, topo + larg), (x + dx, chao)], fill=friso, width=max(1, int(larg * 0.06)))


def xiquexique(img, cor, x, chao, altura, seed):
    """Moitas baixas de cacto (xique-xique) no chão."""
    rnd = random.Random(seed)
    d = ImageDraw.Draw(img)
    c = rgb(cor) + (255,)
    for _ in range(rnd.randint(4, 7)):
        xx = x + rnd.uniform(-altura * 0.6, altura * 0.6)
        a = altura * rnd.uniform(0.4, 1.0)
        braco(d, c, xx, chao + 6, chao - a, altura * 0.09)


def chao(img, cores, y0, cor_rachadura, seed):
    gradiente(img, cores, y0, H)
    rnd = random.Random(seed)
    d = ImageDraw.Draw(img)
    c = rgb(cor_rachadura) + (90,)
    # Rachaduras do chão seco: linhas quebradas que se abrem para a frente.
    for _ in range(70):
        x, y = rnd.uniform(0, W), rnd.uniform(y0 + 40, H)
        for _ in range(rnd.randint(3, 7)):
            nx = x + rnd.uniform(-90, 90) * (y / H) * 2
            ny = y + rnd.uniform(-25, 25)
            d.line([(x, y), (nx, ny)], fill=c, width=max(1, int(3 * y / H)))
            x, y = nx, ny


def gerar(nome):
    m = MODELOS[nome]
    img = Image.new("RGBA", (W, H))
    horizonte = int(H * 0.68)
    gradiente(img, m["ceu"], 0, horizonte + 200)
    estrelas(img, int(H * 0.28), seed=7)
    sol(img, m["sol"], int(W * 0.66), int(H * 0.53), int(H * 0.075))
    fitas(img, m["fita"])
    alturas = [H * 0.16, H * 0.12, H * 0.09]
    for i, (cor, alt) in enumerate(zip(m["serras"], alturas)):
        serra(img, cor, horizonte + i * 40, alt, seed=10 + i, picos=7 + 3 * i)
    chao(img, m["chao"], horizonte + 110, m["silhueta"], seed=3)
    linha_chao = horizonte + 150
    mandacaru(img, m["silhueta"], int(W * 0.22), H - 120, H * 0.62, seed=1)
    mandacaru(img, m["silhueta"], int(W * 0.86), linha_chao + 40, H * 0.30, seed=2)
    mandacaru(img, m["silhueta"], int(W * 0.47), linha_chao, H * 0.16, seed=5)
    xiquexique(img, m["silhueta"], int(W * 0.34), H - 90, H * 0.10, seed=4)
    xiquexique(img, m["silhueta"], int(W * 0.62), linha_chao + 80, H * 0.06, seed=6)
    xiquexique(img, m["silhueta"], int(W * 0.95), H - 140, H * 0.08, seed=8)
    return img.convert("RGB")


def main():
    nome = sys.argv[1] if len(sys.argv) > 1 else "mandacaru"
    saida = Path(sys.argv[2]) if len(sys.argv) > 2 else Path(__file__).parent
    saida.mkdir(parents=True, exist_ok=True)
    img = gerar(nome)
    titulo = nome.capitalize()
    for w, h in TAMANHOS:
        img.resize((w, h), Image.LANCZOS).save(saida / f"Kalango_{titulo}_{w}x{h}.png", optimize=True)
    img.resize((384, 216), Image.LANCZOS).save(saida / f"Kalango_{titulo}_miniatura.png")
    print(f"gerado: {saida}/Kalango_{titulo}_*.png (destaque sugerido {MODELOS[nome]['destaque']})")


if __name__ == "__main__":
    main()
