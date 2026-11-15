#!/usr/bin/env python3
"""Gera o ícone do KalangOS: o K do logotipo como tronco, o lagarto do
logotipo subindo nele e um rabo desenhado para o ícone (fino, passando por baixo
do K e terminando em fita, como as fitas de luz dos papéis de parede).

Escolhido pelo Tiago em 2026-11-15 (versão "R1", fundo noite). Precisa de numpy
e Pillow. Uso:
    python3 assets/logo/gerar_icone.py [pasta_de_saída]
Saída: Kalango_Icone_<fundo>.png (1024 px) para noite, claro e azul.
"""

import numpy as np
from collections import deque
from PIL import Image, ImageDraw, ImageFilter

import sys
from pathlib import Path
RAIZ = Path(__file__).resolve().parents[2]
SRC = RAIZ / 'assets/logo/Kalango_Logo_transparente.png'
S = 2048  # desenha grande e reduz no fim (bordas lisas)
src = Image.open(SRC).convert('RGBA')
arr = np.asarray(src).astype(float)
r, g, b, a = arr[..., 0], arr[..., 1], arr[..., 2], arr[..., 3]
H, W = a.shape
lum = np.minimum(np.minimum(r, g), b)

def flood(mask, seed):
    seen = np.zeros_like(mask); q = deque([seed]); seen[seed] = True
    while q:
        y, x = q.popleft()
        for dy, dx in ((1, 0), (-1, 0), (0, 1), (0, -1)):
            ny, nx = y + dy, x + dx
            if 0 <= ny < mask.shape[0] and 0 <= nx < mask.shape[1] and mask[ny, nx] and not seen[ny, nx]:
                seen[ny, nx] = True; q.append((ny, nx))
    return seen

# --- K ---
soft = np.clip((lum - 120) / 120, 0, 1) * (a / 255); hard = soft > 0.5
ys, xs = np.where(hard[:, : W // 10]); kseen = flood(hard, (int(ys[len(ys)//2]), int(xs[len(xs)//2])))
ky, kx = np.where(kseen)
comp = np.asarray(Image.fromarray((kseen * 255).astype('uint8')).filter(ImageFilter.MaxFilter(5))) / 255
K = Image.fromarray(((comp * soft)[ky.min()-6:ky.max()+7, kx.min()-6:kx.max()+7] * 255).astype('uint8'), 'L')

# --- Lagarto: corpo até logo abaixo das patas de trás, cortado em diagonal
# (perpendicular ao corpo, que ali já curva para virar rabo); a borda do corte
# esmaece e o rabo desenhado começa por baixo dela.
P = np.array([1610.0, 500.0]); Q = np.array([1750.0, 578.0])   # linha do corte
nrm_out = np.array([-(Q - P)[1], (Q - P)[0]]); nrm_out /= np.linalg.norm(nrm_out)  # para fora (baixo-esquerda)
blue = ((b - r) > 40) & (a > 40)
yy, xx = np.mgrid[0:H, 0:W]
side = (xx - P[0]) * nrm_out[0] + (yy - P[1]) * nrm_out[1]     # > 0 = já é rabo
m = blue & (side < 0) & (xx > 1450)
ys, xs = np.where(m); y0, y1, x0, x1 = ys.min(), ys.max(), xs.min(), xs.max()
sub = m[y0:y1+1, x0:x1+1]
outside = flood(~sub, (0, sub.shape[1] - 1))
holes = ~sub & ~outside
fill = sub.copy(); seen = np.zeros_like(holes)
for (hy, hx) in zip(*np.where(holes)):
    if seen[hy, hx]: continue
    reg = flood(holes, (hy, hx)); seen |= reg
    if reg.sum() < 4000: fill |= reg
fade = np.clip(-side[y0:y1+1, x0:x1+1] / 22.0, 0, 1)            # esmaece 22 px antes do corte
liz = src.crop((x0, y0, x1 + 1, y1 + 1))
alpha = (np.where(fill, a[y0:y1+1, x0:x1+1], 0) * fade).astype('uint8')
liz.putalpha(Image.fromarray(alpha, 'L'))
LW, LH = liz.size
# seção do corpo no corte: onde a linha cruza o corpo
tline = np.linspace(-0.3, 1.3, 400)
cross = [P + t * (Q - P) for t in tline]
inside = [c for c in cross if blue[int(c[1]), int(c[0])]]
c0, c1 = inside[0], inside[-1]
root_local = (c0 + c1) / 2 - [x0, y0]
root_w = float(np.linalg.norm(c1 - c0))
up_local = root_local - nrm_out * 60
body_rgb = np.array(src.getpixel((int((c0[0]+c1[0])/2 - nrm_out[0]*30), int((c0[1]+c1[1])/2 - nrm_out[1]*30)))[:3], float)

def smooth_alpha(mask, size):
    big = mask.resize(size, Image.LANCZOS)
    x = np.asarray(big).astype(float) / 255
    x = np.clip((x - 0.5) * 3 + 0.5, 0, 1); x = x * x * (3 - 2 * x)
    return Image.fromarray((x * 255).astype('uint8'), 'L')

def squircle(size, frac=0.22):
    mk = Image.new('L', (size, size), 0)
    ImageDraw.Draw(mk).rounded_rectangle((0, 0, size - 1, size - 1), radius=int(size * frac), fill=255)
    return mk

def background(top, bot, glow):
    yy = np.linspace(0, 1, S)[:, None, None]
    bg = (np.array(top, float) * (1 - yy) + np.array(bot, float) * yy) * np.ones((S, S, 1))
    yv, xv = np.mgrid[0:S, 0:S] / S
    d = np.sqrt((xv - 0.62) ** 2 + (yv - 0.70) ** 2)
    bg += (np.clip(1 - d / 0.6, 0, 1) ** 2)[..., None] * np.array(glow, float)
    return Image.fromarray(np.clip(bg, 0, 255).astype('uint8'), 'RGB').convert('RGBA')

def light_lines(light):
    layer = Image.new('RGBA', (S, S), (0, 0, 0, 0)); d = ImageDraw.Draw(layer)
    t = np.linspace(0, 1, 400)
    for k, col, w in ((0.0, (47, 139, 255), 16), (0.12, (61, 214, 245), 12), (0.06, (201, 210, 222), 8)):
        pts = np.stack([t * S * 1.1 - 80, S * (0.82 + k * 0.5 - 0.15 * np.sin(np.pi * (t * 1.05 + 0.12 + k)))], 1)
        fade = np.clip(np.minimum(t * 4, (1 - t) * 4), 0, 1)
        for i in range(len(pts) - 1):
            d.line([tuple(pts[i]), tuple(pts[i + 1])], fill=col + (int((140 if light else 255) * fade[i]),), width=w)
    out = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    if not light:
        out.alpha_composite(layer.filter(ImageFilter.GaussianBlur(44)))
        out.alpha_composite(layer.filter(ImageFilter.GaussianBlur(14)))
    out.alpha_composite(layer)
    return out

def bez(p0, p1, p2, p3, n):
    t = np.linspace(0, 1, n)[:, None]
    return (1-t)**3*p0 + 3*(1-t)**2*t*p1 + 3*(1-t)*t*t*p2 + t**3*p3

def tail(pts, w0, glow_on, light):
    """Rabo afunilado com sombreado lateral e ponta que se torce (fita)."""
    n = len(pts)
    d = np.gradient(pts, axis=0); d /= np.linalg.norm(d, axis=1, keepdims=True) + 1e-9
    nrm = np.stack([-d[:, 1], d[:, 0]], 1)
    t = np.linspace(0, 1, n)
    half = w0 / 2 * (1 - t) ** 1.25 + 2.5
    twist = np.where(t > 0.62, np.abs(np.cos((t - 0.62) / 0.38 * np.pi * 1.5)) * 0.85 + 0.15, 1.0)
    half *= twist
    tip = np.array([61, 214, 245], float); deep = np.array([24, 110, 230], float)
    layer = Image.new('RGBA', (S, S), (0, 0, 0, 0)); dl = ImageDraw.Draw(layer)
    # cilindro: faixas do lado da sombra (-1) ao da luz (+1)
    edges = np.linspace(-1, 1, 13)
    bands = [(edges[j], edges[j + 1], 0.55 + 0.75 * ((edges[j] + edges[j + 1]) / 4 + 0.5) ** 1.2) for j in range(12)]
    for i in range(n - 1):
        c = body_rgb * (1 - t[i]) + tip * t[i]
        for lo, hi, k in bands:
            col = np.clip(c * k + max(0, k - 1.05) * 90, 0, 255)
            q = [tuple(pts[i] + nrm[i] * half[i] * lo), tuple(pts[i+1] + nrm[i+1] * half[i+1] * lo),
                 tuple(pts[i+1] + nrm[i+1] * half[i+1] * hi), tuple(pts[i] + nrm[i] * half[i] * hi)]
            dl.polygon(q, fill=tuple(int(v) for v in col) + (255,))
    layer = layer.filter(ImageFilter.GaussianBlur(3))
    out = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    if glow_on:
        gl = Image.new('RGBA', (S, S), (0, 0, 0, 0)); gl.paste(Image.new('RGBA', (S, S), (47, 139, 255, 255)), (0, 0), layer.split()[3])
        out.alpha_composite(Image.blend(Image.new('RGBA', (S, S), (0, 0, 0, 0)), gl.filter(ImageFilter.GaussianBlur(40)), 0.7))
    out.alpha_composite(layer)
    return out

def compose(name, top, bot, glow, k_color, light=False, lines=True, rot=10, kscale=0.62, lscale=0.86, kxf=0.37, snout_x=0.52, snout_y=0.10):
    canvas = background(top, bot, glow)
    if lines:
        canvas.alpha_composite(light_lines(light))
    kh = int(S * kscale); kw = int(K.width * kh / K.height)
    kmask = smooth_alpha(K, (kw, kh)); kx, ky = int(S * kxf), int((S - kh) / 2)
    # lagarto do tamanho de antes (altura do lagarto inteiro ≈ 1,18 × K × lscale)
    full_h = int(kh * 1.18 * lscale); scale = full_h / 664.0
    lw, lh = int(LW * scale), int(LH * scale)
    L = liz.resize((lw, lh), Image.LANCZOS)
    # marcadores (raiz do rabo, um ponto acima dela e o focinho), girados junto
    def marker(p):
        mk = Image.new('L', (lw, lh), 0); x, y = p * scale
        ImageDraw.Draw(mk).ellipse((x - 4, y - 4, x + 4, y + 4), fill=255)
        return mk.rotate(rot, resample=Image.BICUBIC, expand=True)
    snout_local = np.array([LW * 0.985, LH * 0.115])
    mr, mu, ms = marker(root_local), marker(up_local), marker(snout_local)
    L = L.rotate(rot, resample=Image.BICUBIC, expand=True)
    def pos(img):
        yy, xx = np.where(np.asarray(img) > 60); return np.array([xx.mean(), yy.mean()])
    # focinho logo à direita da haste, perto do topo do K
    target = np.array([kx + kw * snout_x, ky + kh * snout_y])
    off = target - pos(ms)
    lx, ly = int(off[0]), int(off[1])
    root = pos(mr) + off; up = pos(mu) + off
    dir0 = (root - up) / np.linalg.norm(root - up)
    w0 = root_w * scale * 0.98
    root = root - dir0 * 26 * scale
    # caminho: desce pela haste, enrola na base do K e sai em fita para a direita
    # desce junto da haste, passa por baixo dela e sobe afinando para a direita
    p_b = np.array([kx + kw * 0.30, ky + kh * 1.06])
    p_d = np.array([kx + kw * 1.05, ky + kh * 0.86])
    a1 = bez(root, root + dir0 * kh * 0.22, p_b + [-kw * 0.30, kh * 0.02], p_b, 260)
    a2 = bez(p_b, p_b + [kw * 0.30, -kh * 0.02], p_d + [-kw * 0.30, kh * 0.20], p_d, 300)
    pts = np.vstack([a1, a2[1:]])
    # sombras
    sh = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    sh.paste(Image.new('RGBA', (kw, kh), (0, 0, 0, 150 if not light else 45)), (kx + 20, ky + 32), kmask)
    canvas.alpha_composite(sh.filter(ImageFilter.GaussianBlur(44)))
    canvas.paste(Image.new('RGBA', (kw, kh), k_color + (255,)), (kx, ky), kmask)
    canvas.alpha_composite(tail(pts, w0, not light, light))
    lsh = Image.new('RGBA', (S, S), (0, 0, 0, 0))
    lsh.paste(Image.new('RGBA', L.size, (0, 0, 0, 130 if not light else 40)), (lx + 16, ly + 24), L.split()[3])
    canvas.alpha_composite(lsh.filter(ImageFilter.GaussianBlur(28)))
    canvas.alpha_composite(L, (lx, ly))
    out = Image.new('RGBA', (S, S), (0, 0, 0, 0)); out.paste(canvas, (0, 0), squircle(S))
    out.resize((1024, 1024), Image.LANCZOS).save(SAIDA / f'{name}.png')

if __name__ == '__main__':
    SAIDA = Path(sys.argv[1]) if len(sys.argv) > 1 else RAIZ / 'assets/logo'
    SAIDA.mkdir(parents=True, exist_ok=True)
    compose('Kalango_Icone_Noite', (3, 5, 9), (6, 12, 30), (12, 46, 140), (255, 255, 255))
    compose('Kalango_Icone_Claro', (255, 255, 255), (230, 238, 248), (-8, -3, 6), (27, 42, 74), light=True)
    compose('Kalango_Icone_Azul', (16, 44, 120), (5, 10, 26), (10, 40, 120), (255, 255, 255))
    print('gerado em', SAIDA)
