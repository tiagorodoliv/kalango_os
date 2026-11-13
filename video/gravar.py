"""Grava o vídeo de apresentação do Kalango OS (inglês, narrado).

Pré-requisitos:
  1. VM ligada com `video/vm.sh` (e `--ingles` numa ISO antiga), parada na
     tela de login, com a Loja já aberta uma vez na sessão (a primeira
     abertura baixa o catálogo do Flathub e demora).
  2. Piper e a voz em video/.cache (ver video/README.md).

  python3 video/gravar.py            grava e monta video/saida/kalango-os.mp4
  python3 video/gravar.py --montar   só remonta a partir da última gravação

As posições (x, y) são da tela de 1920×1080 da VM.
"""

import contextlib
import json
import os
import subprocess
import sys
import time

VIDEO = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, VIDEO)
from narracao import CENAS, sintetizar  # noqa: E402
from vm import Gravador, Qmp, Vnc  # noqa: E402

ROOT = os.path.dirname(VIDEO)
CACHE = os.path.join(VIDEO, ".cache")
SAIDA = os.path.join(VIDEO, "saida")
W, H = 1920, 1080
FOLGA = 0.7  # silêncio entre uma fala e a próxima

# Pontos da tela (1920×1080).
DOCK = {"files": (800, 1026), "terminal": (860, 1026), "browser": (920, 1026),
        "code": (980, 1026), "store": (1040, 1026), "settings": (1118, 1026)}
LADO = {"network": 216, "appearance": 300, "taskbar": 342, "displays": 384, "sound": 426,
        "power": 468, "region": 552, "updates": 594, "about": 636}


class Diretor:
    def __init__(self, q, rec, narr, vnc):
        self.q, self.rec, self.narr, self.vnc = q, rec, narr, vnc
        self.marcas = []
        self.cortes = []

    @contextlib.contextmanager
    def corte(self):
        """Trecho de espera que sai do vídeo final (corte seco)."""
        inicio = self.rec.agora()
        yield
        self.cortes.append((inicio, self.rec.agora()))

    def pixel(self, x, y):
        tela, w, _ = self.vnc.copia()
        i = (y * w + x) * 4
        return tela[i + 2], tela[i + 1], tela[i]  # R, G, B

    def esperar(self, condicao, limite=30):
        """Espera até `condicao(self)` ser verdadeira (ou o limite, em segundos)."""
        fim = time.monotonic() + limite
        while time.monotonic() < fim and not condicao(self):
            time.sleep(0.2)

    @contextlib.contextmanager
    def cena(self, cid):
        inicio = self.rec.agora()
        self.marcas.append((cid, inicio))
        print(f"{inicio:6.1f}s  {cid}", flush=True)
        yield
        fim = inicio + self.narr[cid][1] + FOLGA
        while self.rec.agora() < fim:
            time.sleep(0.05)


def p(seg):
    time.sleep(seg)


def roteiro(d):
    q = d.q
    with d.cena("login"):
        p(1.2)
        q.clicar(930, 735, dur=1.0)
        p(0.5)
        q.digitar("live", 0.2)
        p(0.6)
        q.clicar(1092, 735, dur=0.5)
        p(3.5)

    with d.cena("desktop"):
        p(0.8)
        q.mover(70, 16, 1.2)
        p(1.4)
        q.mover(960, 18, 1.2)
        p(1.6)
        q.mover(1790, 18, 1.4)
        p(1.2)
        q.mover(1890, 18, 0.8)
        p(1.2)
        q.mover(*DOCK["files"], 1.4)
        q.mover(*DOCK["settings"], 2.4)
        p(0.5)
        q.mover(1500, 700, 1.0)

    with d.cena("menu"):
        q.teclas("meta_l")
        p(1.2)
        q.mover(664, 310, 0.8)
        q.mover(1258, 310, 1.6)
        q.mover(1258, 422, 0.5)
        q.mover(664, 422, 1.6)
        q.mover(780, 552, 0.7)

    with d.cena("todos"):
        q.clicar(1284, 238, dur=0.8)
        p(1.2)
        q.mover(960, 500, 0.6)
        for _ in range(3):
            q.rolar(2)
            p(0.6)
        p(0.6)
        q.rolar(-6)
        p(0.4)
        q.clicar(1284, 238, dur=0.8)

    with d.cena("busca"):
        p(0.3)
        q.digitar("files", 0.16)
        p(1.2)
        q.teclas("ret")

    with d.cena("arquivos"):
        p(2.0)
        q.mover(884, 300, 0.8)
        p(0.4)
        q.mover(1292, 300, 1.0)
        p(0.3)
        q.clicar()
        p(0.08)
        q.clicar()
        p(1.6)
        q.clicar(538, 236, dur=0.8)
        p(1.4)
        q.teclas("alt", "f4")

    with d.cena("terminal"):
        p(0.6)
        q.teclas("meta_l", "ret")
        p(1.6)
        q.digitar("cat /etc/os-release | head -3", 0.06)
        q.teclas("ret")
        p(2.6)
        q.digitar("exit", 0.08)
        q.teclas("ret")

    with d.cena("aparencia"):
        q.clicar(*DOCK["settings"], dur=1.0)
        p(2.4)
        q.clicar(1150, 290, dur=0.9)   # Light
        p(1.6)
        q.clicar(646, 452, dur=0.9)    # roxo
        p(1.4)
        q.clicar(762, 452, dur=0.6)    # verde
        p(1.4)
        q.clicar(914, 290, dur=0.8)    # OLED
        p(1.4)
        q.clicar(678, 290, dur=0.6)    # vidro
        p(0.6)
        q.clicar(588, 452, dur=0.7)    # ciano
        p(0.8)

    with d.cena("icones"):
        q.clicar(876, 778, dur=0.9)    # Azul
        p(1.6)
        q.clicar(1056, 778, dur=0.7)   # Roxo
        p(1.6)
        q.clicar(696, 778, dur=0.9)    # Preto e azul
        p(0.8)
        q.clicar(928, 910, dur=0.8)    # Colloid
        p(2.6)
        q.clicar(1174, 910, dur=0.7)   # Fluent
        p(2.6)
        q.clicar(682, 910, dur=0.9)    # Flat Remix
        p(1.0)

    with d.cena("dock"):
        q.clicar(340, LADO["taskbar"], dur=0.9)
        p(1.0)
        q.clicar(1140, 554, dur=0.9)   # Left
        p(2.6)
        q.clicar(1224, 621, dur=0.7)   # Bar
        p(2.6)
        q.clicar(1054, 554, dur=0.7)   # Bottom
        p(2.2)
        q.clicar(1138, 621, dur=0.6)   # Dock
        p(1.0)

    with d.cena("sistema"):
        for item in ("network", "sound", "displays", "power", "updates", "about"):
            q.clicar(340, LADO[item], dur=0.6)
            p(1.4)

    with d.cena("idioma"):
        q.clicar(340, LADO["region"], dur=0.8)
        p(1.0)
        q.clicar(1142, 219, dur=0.8)   # idioma
        p(1.8)
        q.clicar(900, 700, dur=0.6)    # fecha a lista
        p(0.6)
        q.clicar(1142, 286, dur=0.6)   # teclado
        p(1.2)
        q.clicar(1124, 433, dur=0.7)   # Portuguese (Brazil)
        p(2.4)
        q.clicar(1142, 286, dur=0.6)
        p(0.8)
        q.clicar(1124, 331, dur=0.6)   # English (US)
        p(0.8)

    with d.cena("direito"):
        q.teclas("alt", "f4")
        p(0.8)
        q.clicar(1300, 600, botao="right", dur=0.9)
        p(0.8)
        for y in (620, 656, 690, 732, 766):
            q.mover(1380, y, 0.45)
            p(0.5)
        q.clicar(600, 800, dur=0.9)

    with d.cena("loja"):
        q.clicar(*DOCK["store"], dur=1.0)
        # Espera o catálogo: o botão verde "Learn" aparece no lugar do
        # "Refreshing Data" (a Loja atualiza os dados a cada login).
        p(1.2)
        with d.corte():
            d.esperar(lambda d: (lambda r, g, b: g > r + 50)(*d.pixel(960, 680)), 40)
        p(1.5)
        q.mover(1100, 600, 1.2)
        q.rolar(4)
        p(2.0)
        q.rolar(-4)
        p(1.5)
        q.teclas("alt", "f4")

    with d.cena("bloqueio"):
        p(0.4)
        q.teclas("meta_l", "l")
        p(2.6)
        q.digitar("live", 0.18)
        q.teclas("ret")
        p(1.8)


def gravar(narr):
    os.makedirs(SAIDA, exist_ok=True)
    vnc = Vnc(os.path.join(CACHE, "vnc.sock"))
    time.sleep(1.0)
    q = Qmp(os.path.join(CACHE, "qmp.sock"), W, H)
    q.mover(1500, 300, 0.2)
    time.sleep(0.5)
    rec = Gravador(vnc, os.path.join(CACHE, "captura.mkv"))
    d = Diretor(q, rec, narr, vnc)
    try:
        p(0.8)
        roteiro(d)
        p(1.0)
    finally:
        total = rec.agora()
        rec.parar()
        q.fechar()
        vnc.fechar()
    json.dump({"marcas": d.marcas, "cortes": d.cortes, "total": total}, open(os.path.join(CACHE, "captura.json"), "w"), indent=1)


# --- Montagem ---------------------------------------------------------------

def cartao(caminho, linhas):
    """Quadro de abertura/encerramento: logotipo e linhas de texto."""
    from PIL import Image, ImageDraw, ImageFont

    img = Image.new("RGB", (W, H), "#07090d")
    dr = ImageDraw.Draw(img)
    for x in range(0, W, 48):
        dr.line([(x, 0), (x, H)], fill="#0c1117")
    for y in range(0, H, 48):
        dr.line([(0, y), (W, y)], fill="#0c1117")
    logo = Image.open(os.path.join(ROOT, "ui/brand/kalango-logo.png")).convert("RGBA")
    lw = 860
    logo = logo.resize((lw, int(logo.height * lw / logo.width)), Image.LANCZOS)
    y0 = 330
    img.paste(logo, ((W - lw) // 2, y0), logo)
    y = y0 + logo.height + 60
    for texto, tamanho, cor, fonte in linhas:
        f = ImageFont.truetype(os.path.join(ROOT, "ui/fonts", fonte), tamanho)
        larg = dr.textlength(texto, font=f)
        dr.text(((W - larg) / 2, y), texto, font=f, fill=cor)
        y += int(tamanho * 1.7)
    img.save(caminho)


def legendas(eventos, caminho):
    """SRT: cada fala dividida em frases, com o tempo proporcional ao tamanho."""
    def ts(t):
        ms = int(round(t * 1000))
        return f"{ms // 3600000:02}:{ms // 60000 % 60:02}:{ms // 1000 % 60:02},{ms % 1000:03}"

    textos = dict(CENAS)
    n, linhas = 1, []
    for cid, inicio, dur in eventos:
        frases = [f.strip() for f in textos[cid].replace(": ", ". ").split(". ") if f.strip()]
        total = sum(len(f) for f in frases)
        t = inicio
        for f in frases:
            dt = dur * len(f) / total
            fim_ = f if f.endswith((".", "!", "?")) else f + "."
            fim_ = fim_[0].upper() + fim_[1:]
            linhas += [str(n), f"{ts(t)} --> {ts(t + dt)}", fim_, ""]
            n += 1
            t += dt
    open(caminho, "w").write("\n".join(linhas))


def montar(narr):
    info = json.load(open(os.path.join(CACHE, "captura.json")))
    intro_d = narr["intro"][1] + 2.0
    fim_d = narr["fim"][1] + 2.5
    cartao(os.path.join(CACHE, "intro.png"), [
        ("A Linux desktop built on Debian 13", 46, "#e6edf3", "Inter-Medium.ttf"),
        ("Shell written in Rust + Slint", 30, "#8b98a5", "JetBrainsMono-Regular.ttf"),
    ])
    cartao(os.path.join(CACHE, "fim.png"), [
        ("Free and open source · GPL-3.0-or-later", 44, "#e6edf3", "Inter-Medium.ttf"),
        ("Built on Debian 13 “trixie”", 30, "#3dd6f5", "JetBrainsMono-Regular.ttf"),
    ])
    # Cortes (esperas): o vídeo pula esses trechos e os tempos depois deles
    # andam para trás.
    cortes = sorted(info.get("cortes", []))

    def mapear(t):
        return t - sum(min(b, max(t, a)) - a for a, b in cortes)

    cap_d = mapear(info["total"])
    trechos, ini = [], 0.0
    for a, b in cortes:
        trechos.append((ini, a))
        ini = b
    trechos.append((ini, info["total"]))

    eventos = [("intro", 0.9, narr["intro"][1])]
    for cid, t in info["marcas"]:
        eventos.append((cid, intro_d + mapear(t) + 0.25, narr[cid][1]))
    eventos.append(("fim", intro_d + cap_d + 0.9, narr["fim"][1]))
    total = intro_d + cap_d + fim_d

    entradas = ["-loop", "1", "-framerate", "30", "-t", f"{intro_d:.3f}", "-i", os.path.join(CACHE, "intro.png"),
                "-i", os.path.join(CACHE, "captura.mkv"),
                "-loop", "1", "-framerate", "30", "-t", f"{fim_d:.3f}", "-i", os.path.join(CACHE, "fim.png")]
    filtros = [
        f"[0:v]fps=30,format=yuv444p,fade=t=in:st=0:d=0.8,fade=t=out:st={intro_d - 0.6:.3f}:d=0.6[v0]",
        f"[1:v]split={len(trechos)}" + "".join(f"[c{i}]" for i in range(len(trechos))),
        *[f"[c{i}]trim={a:.3f}:{b:.3f},setpts=PTS-STARTPTS[t{i}]" for i, (a, b) in enumerate(trechos)],
        "".join(f"[t{i}]" for i in range(len(trechos))) + f"concat=n={len(trechos)}:v=1:a=0,"
        f"fps=30,format=yuv444p,fade=t=in:st=0:d=0.6,fade=t=out:st={cap_d - 0.6:.3f}:d=0.6[v1]",
        f"[2:v]fps=30,format=yuv444p,fade=t=in:st=0:d=0.6,fade=t=out:st={fim_d - 1.0:.3f}:d=1.0[v2]",
        "[v0][v1][v2]concat=n=3:v=1:a=0,format=yuv420p[v]",
    ]
    rotulos = []
    for i, (cid, inicio, _) in enumerate(eventos):
        entradas += ["-i", narr[cid][0]]
        ms = int(inicio * 1000)
        filtros.append(f"[{3 + i}:a]aresample=48000,adelay={ms}|{ms}[a{i}]")
        rotulos.append(f"[a{i}]")
    filtros.append(f"{''.join(rotulos)}amix=inputs={len(rotulos)}:normalize=0,"
                   f"apad,atrim=0:{total:.3f},loudnorm=I=-16:TP=-1.5,aresample=48000,"
                   "aformat=channel_layouts=stereo[a]")
    srt = os.path.join(SAIDA, "kalango-os.srt")
    legendas(eventos, srt)
    saida = os.path.join(SAIDA, "kalango-os.mp4")
    subprocess.run(
        ["ffmpeg", "-loglevel", "error", "-y", *entradas, "-i", srt,
         "-filter_complex", ";".join(filtros), "-map", "[v]", "-map", "[a]",
         "-map", f"{3 + len(eventos)}:s",
         "-c:v", "libx264", "-preset", "slow", "-crf", "18", "-profile:v", "high",
         "-c:a", "aac", "-b:a", "192k", "-c:s", "mov_text", "-metadata:s:s:0", "language=eng", "-disposition:s:0", "0",
         "-movflags", "+faststart", saida],
        check=True,
    )
    print(f">> {saida} ({total:.0f} s) e {srt}")


if __name__ == "__main__":
    narr = sintetizar()
    if "--montar" not in sys.argv:
        gravar(narr)
    montar(narr)
