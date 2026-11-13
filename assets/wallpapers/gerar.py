#!/usr/bin/env python3
"""Gera os papéis de parede 16:9 do Kalango OS.

Sete modelos (preto, branco, azul, verde, roxo, laranja e preto com azul),
cada um com fundo escuro (ou claro) em gradiente, fitas de luz translúcidas
que se torcem (miolo quase transparente, bordas finas e brilhantes com halo),
hexágonos nos cantos com algumas arestas acesas e o logotipo real da marca no
centro. Tudo é desenhado em 4K e reduzido para 2K e Full HD.

Uso (precisa de numpy e Pillow):
    python3 assets/wallpapers/gerar.py [pasta_de_saída]
    python3 assets/wallpapers/gerar.py --previa [pasta]   (só 1920x1080, rápido)
Saída padrão: assets/wallpapers/16x9/Kalango_<Modelo>_<largura>x<altura>.png
(+ Kalango_<Modelo>_miniatura.png, 384x216, para as Configurações)
"""

import math
import sys
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

RAIZ = Path(__file__).resolve().parents[2]
LOGO = RAIZ / "assets/logo/Kalango_Logo_transparente.png"
PREVIA = "--previa" in sys.argv
W, H = (1920, 1080) if PREVIA else (3840, 2160)
# Medidas em pixels pensadas para 4K; E reduz na prévia.
E = W / 3840
TAMANHOS = [(1920, 1080)] if PREVIA else [(3840, 2160), (2560, 1440), (1920, 1080)]


def cor(hexa):
    """'#rrggbb' -> vetor float 0..1."""
    hexa = hexa.lstrip("#")
    return np.array([int(hexa[i : i + 2], 16) / 255 for i in (0, 2, 4)], np.float32)


# Cada modelo: fundo (topo, base), cor do brilho de fundo, cor das bordas das
# fitas, cor do véu (miolo translúcido) e se o fundo é claro.
MODELOS = {
    "Preto": dict(topo="#000000", base="#08080a", brilho="#1c1c1f", borda="#f2f2f2",
                  borda2="#9a9a9a", veu="#d8d8d8", claro=False),
    "Branco": dict(topo="#ffffff", base="#eef1f5", brilho="#ffffff", borda="#7f8ea3",
                   borda2="#b3bfcd", veu="#9aa7b8", claro=True),
    "Azul": dict(topo="#00030f", base="#010a2c", brilho="#0a3fd0", borda="#4a95ff",
                 borda2="#2f6dff", veu="#2a6cff", claro=False),
    "Verde": dict(topo="#000400", base="#010d02", brilho="#0d7a16", borda="#4be357",
                  borda2="#23b432", veu="#2fc73d", claro=False),
    "Roxo": dict(topo="#030109", base="#0c0420", brilho="#4d14a8", borda="#b066ff",
                 borda2="#8b3dff", veu="#9a4dff", claro=False),
    "Laranja": dict(topo="#050200", base="#100500", brilho="#a63c08", borda="#ff9330",
                    borda2="#ff6a10", veu="#ff7a1f", claro=False),
    # Modelo da segunda referência: fundo preto, fitas acinzentadas com bordas azuis.
    "PretoAzul": dict(topo="#000000", base="#050608", brilho="#0a2a80", borda="#2f8bff",
                      borda2="#c9d2de", veu="#aab4c2", claro=False),
}


def desfoque(img, sigma):
    """Desfoque quase gaussiano (3 passadas de caixa com soma acumulada)."""
    raio = max(1, int(round(math.sqrt(12 * sigma * sigma / 3 + 1) / 2)))
    out = img.astype(np.float32)
    for eixo in (0, 1):
        for _ in range(3):
            pad = [(0, 0)] * out.ndim
            pad[eixo] = (raio + 1, raio)
            c = np.cumsum(np.pad(out, pad, mode="edge"), axis=eixo)
            fim = np.take(c, np.arange(2 * raio + 1, c.shape[eixo]), axis=eixo)
            ini = np.take(c, np.arange(0, c.shape[eixo] - 2 * raio - 1), axis=eixo)
            out = (fim - ini) / (2 * raio + 1)
    return out


YY = np.arange(H, dtype=np.float32)[:, None]
T = np.linspace(0, 1, W, dtype=np.float32)


def fundo(m):
    """Gradiente vertical + brilhos radiais suaves."""
    y = np.linspace(0, 1, H, dtype=np.float32)[:, None, None]
    img = cor(m["topo"]) * (1 - y) + cor(m["base"]) * y
    img = np.broadcast_to(img, (H, W, 3)).copy()
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    for cx, cy, raio, forca in ((0.85, 0.75, 0.70, 0.55), (0.10, 0.85, 0.55, 0.35), (0.55, 0.62, 0.50, 0.25)):
        d = np.sqrt(((xx - cx * W) / W) ** 2 + ((yy - cy * H) / W) ** 2) / raio
        img += (np.clip(1 - d, 0, 1) ** 2.4)[..., None] * cor(m["brilho"]) * forca
    if m["claro"]:
        d = np.sqrt(((xx - W / 2) / W) ** 2 + ((yy - H * 0.45) / H) ** 2)
        img -= (np.clip(d - 0.35, 0, 1) * 0.10)[..., None] * np.array([1.0, 0.9, 0.75], np.float32)
    return img


def linha(y, sigma):
    """Perfil gaussiano de uma curva y(x) (distância perpendicular aproximada)."""
    inclin = np.gradient(y) if y.ndim == 1 else 0
    d = (YY - y[None, :]) / np.sqrt(1 + inclin**2)[None, :]
    return np.exp(-0.5 * (d / sigma) ** 2)


def fita(centro, meia, fase, fade):
    """Uma fita que se torce: bordas = centro ± meia·cos(fase).

    Devolve (borda_a, borda_b, véu): as duas bordas como linhas finas e o miolo
    translúcido, mais claro perto das bordas (como seda vista de lado).
    """
    c, a = centro(T) * H, meia(T) * H * np.cos(fase(T))
    y1, y2 = c - a, c + a
    alto, baixo = np.minimum(y1, y2), np.maximum(y1, y2)
    larg = np.maximum(baixo - alto, 1)
    dentro = np.clip((YY - alto) / (3 * E), 0, 1) * np.clip((baixo - YY) / (3 * E), 0, 1)
    pos = np.clip((YY - alto) / larg, 0, 1)
    perto_borda = 0.35 + 0.65 * (np.abs(pos - 0.5) * 2) ** 3
    f = fade(T)[None, :]
    veu = dentro * perto_borda * f
    s = 1.6 * E
    return linha(y1, s) * f, linha(y2, s) * f, veu


# Fitas: (centro, meia largura, fase da torção, esmaecer nas pontas, peso, usa borda2).
def _pontas(t, ini=0.0, fim=1.0):
    return np.clip(np.minimum(t - ini, fim - t) * 6, 0, 1)


FITAS = [
    # Principal: passa logo abaixo do logo, sobe e desce em curva.
    (lambda t: 0.64 - 0.10 * np.sin(np.pi * (1.2 * t + 0.1)) + 0.05 * t,
     lambda t: 0.035 + 0.075 * np.abs(np.sin(np.pi * (1.3 * t + 0.25))),
     lambda t: 2.4 * np.pi * t + 0.4, lambda t: _pontas(t, -0.2, 1.2), 1.0, False),
    # Segunda, logo abaixo, defasada (cruza a principal).
    (lambda t: 0.70 - 0.13 * np.sin(np.pi * (1.0 * t + 0.35)) + 0.04 * t,
     lambda t: 0.03 + 0.06 * np.abs(np.sin(np.pi * (1.6 * t + 0.7))),
     lambda t: 2.0 * np.pi * t + 1.9, lambda t: _pontas(t, -0.2, 1.2), 0.8, True),
    # Embaixo: nasce no canto inferior esquerdo e sobe para a direita.
    (lambda t: 0.95 - 0.22 * t - 0.05 * np.sin(2 * np.pi * t),
     lambda t: 0.025 + 0.06 * np.abs(np.sin(np.pi * (1.1 * t + 0.4))),
     lambda t: 1.8 * np.pi * t + 0.8, lambda t: _pontas(t, -0.2, 1.2), 0.85, False),
    # Canto superior direito: arco que sobe para fora da tela.
    (lambda t: 0.02 + 0.55 * (1 - t) ** 2.2 + 0.05 * np.sin(3 * t),
     lambda t: 0.03 + 0.05 * t,
     lambda t: 1.5 * np.pi * t + 0.2, lambda t: np.clip((t - 0.45) * 3, 0, 1), 0.75, False),
    # Atrás do logo, fraca.
    (lambda t: 0.45 + 0.08 * np.sin(np.pi * (1.3 * t - 0.35)),
     lambda t: 0.02 + 0.04 * np.abs(np.sin(np.pi * (1.1 * t + 0.6))),
     lambda t: 1.6 * np.pi * t + 1.3, lambda t: _pontas(t, -0.1, 1.1), 0.35, True),
]


def ondas():
    """Camadas das fitas: bordas (cor principal e secundária) e véu."""
    borda = np.zeros((H, W), np.float32)
    borda2 = np.zeros((H, W), np.float32)
    veu = np.zeros((H, W), np.float32)
    for centro, meia, fase, fade, peso, segunda in FITAS:
        a, b, v = fita(centro, meia, fase, fade)
        alvo = borda2 if segunda else borda
        alvo += (a + b * 0.8) * peso
        veu += v * peso
    return borda, borda2, veu


def hexagonos():
    """Grade de hexágonos nos cantos superior esquerdo e inferior direito.

    Devolve (contorno apagado, arestas acesas): algumas arestas brilham na
    cor da borda das fitas, como na referência.
    """
    base = Image.new("L", (W, H), 0)
    acesa = Image.new("L", (W, H), 0)
    db, da = ImageDraw.Draw(base), ImageDraw.Draw(acesa)
    r = W * 0.042
    dx, dy = r * math.sqrt(3), r * 1.5
    rng = np.random.default_rng(11)
    larg = max(1, round(3 * E))
    for lin in range(-1, int(H / dy) + 2):
        for col in range(-1, int(W / dx) + 2):
            cx = col * dx + (dx / 2 if lin % 2 else 0)
            cy = lin * dy
            pts = [(cx + r * math.cos(math.radians(60 * k + 30)), cy + r * math.sin(math.radians(60 * k + 30))) for k in range(7)]
            db.line(pts, fill=255, width=larg)
            for k in range(6):
                if rng.random() < 0.10:
                    da.line([pts[k], pts[k + 1]], fill=255, width=larg + 1)
    yy, xx = np.mgrid[0:H, 0:W].astype(np.float32)
    canto1 = np.clip(1 - np.sqrt((xx / (W * 0.27)) ** 2 + (yy / (H * 0.62)) ** 2), 0, 1) ** 1.3
    canto2 = np.clip(1 - np.sqrt(((W - xx) / (W * 0.22)) ** 2 + ((H - yy) / (H * 0.42)) ** 2), 0, 1) ** 1.3
    mascara = np.maximum(canto1, canto2)
    b = desfoque(np.asarray(base, np.float32) / 255, 0.7 * E) * mascara
    a = np.asarray(acesa, np.float32) / 255 * mascara
    a = desfoque(a, 0.9 * E) + desfoque(a, 10 * E) * 1.5
    return b, a


def logo(claro):
    """Logotipo da marca; no fundo claro as letras brancas viram azul-marinho."""
    img = Image.open(LOGO).convert("RGBA")
    if claro:
        a = np.asarray(img).astype(np.float32)
        rgb = a[..., :3]
        sat = rgb.max(-1) - rgb.min(-1)
        h, w = sat.shape
        yy, xx = np.mgrid[0:h, 0:w]
        # Letras: brancas e sem cor. Fora a cabeça do calango (o branco do olho).
        letra = (sat < 45) & (rgb.min(-1) > 90) & ~((xx > w * 0.86) & (yy < h * 0.30))
        a[letra, :3] = np.array([12, 27, 51], np.float32)
        img = Image.fromarray(a.astype(np.uint8), "RGBA")
    largura = int(W * 0.44)
    return img.resize((largura, round(img.height * largura / img.width)), Image.LANCZOS)


def gerar(m, logo_img, camadas, hexa):
    img = fundo(m)
    borda, borda2, veu = camadas
    hex_base, hex_acesa = hexa
    c1, c2, cv = cor(m["borda"]), cor(m["borda2"]), cor(m["veu"])
    linhas = borda[..., None] * c1 + borda2[..., None] * c2
    if m["claro"]:
        # Fundo claro: bordas cinza-azuladas, véu acinzentado e sombra suave.
        img = np.clip(img, 0, 1)
        k = np.clip(borda * 0.55 + borda2 * 0.45 + veu * 0.16
                    + desfoque(borda + borda2, 14 * E) * 0.35, 0, 0.9)[..., None]
        img = img * (1 - k) + c1 * k
        img -= hex_base[..., None] * 0.12
    else:
        img += veu[..., None] * cv * 0.30
        img += linhas * 0.95
        brilho = desfoque(borda, 10 * E)[..., None] * c1 + desfoque(borda2, 10 * E)[..., None] * c2
        halo = desfoque(borda, 60 * E)[..., None] * c1 + desfoque(borda2, 60 * E)[..., None] * c2
        img += brilho * 1.4 + halo * 2.2
        img += hex_base[..., None] * 0.20 + hex_acesa[..., None] * c1 * 1.1
    # Núcleo branco onde as bordas mais brilham (as linhas parecem de luz).
    if not m["claro"]:
        img += (np.clip(borda - 0.6, 0, 1) * 0.6)[..., None]
    # Pontilhado leve contra faixas no gradiente.
    img += (np.random.default_rng(7).random((H, W, 1), np.float32) - 0.5) / 255
    arr = np.clip(img, 0, 1)

    # Sombra suave atrás do logo e o logo no centro, um pouco acima do meio.
    x = (W - logo_img.width) // 2
    y = int(H * 0.43 - logo_img.height / 2)
    alfa = np.asarray(logo_img.split()[3], np.float32) / 255
    sombra = np.zeros((H, W), np.float32)
    sombra[y : y + logo_img.height, x : x + logo_img.width] = alfa
    sombra = desfoque(sombra, 30 * E) * (0.25 if m["claro"] else 0.65)
    arr *= (1 - sombra)[..., None]
    base = Image.fromarray((arr * 255 + 0.5).astype(np.uint8), "RGB")
    base.paste(logo_img, (x, y), logo_img)
    return base


def main():
    args = [a for a in sys.argv[1:] if a != "--previa"]
    saida = Path(args[0]) if args else RAIZ / "assets/wallpapers/16x9"
    saida.mkdir(parents=True, exist_ok=True)
    camadas = ondas()
    hexa = hexagonos()
    logos = {False: logo(False), True: logo(True)}
    for nome, m in MODELOS.items():
        img = gerar(m, logos[m["claro"]], camadas, hexa)
        for w, h in TAMANHOS:
            final = img if (w, h) == (W, H) else img.resize((w, h), Image.LANCZOS)
            arq = saida / f"Kalango_{nome}_{w}x{h}.png"
            final.save(arq, optimize=True)
            print("gerado", arq)
        # Miniatura para a escolha em Configurações → Aparência.
        mini = saida / f"Kalango_{nome}_miniatura.png"
        img.resize((384, 216), Image.LANCZOS).save(mini, optimize=True)


if __name__ == "__main__":
    main()
