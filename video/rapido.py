"""Vídeo curto de apresentação para o site (~1 min, sem narração).

Grava a VM de `video/vm.sh` (1920×1080) passando pelas novidades da versão e
monta, em video/saida/:

  kalango-rapido.mp4        vídeo (H.264, 1920×1080, sem áudio)
  kalango-rapido.jpg        imagem de capa (poster)
  kalango-rapido.en.vtt     legendas em inglês
  kalango-rapido.pt.vtt     legendas em português
  kalango-rapido.capitulos.json  capítulos (início em segundos, pt e en)

Pré-requisitos (ver video/README.md): VM ligada, com o ufw instalado e o
firewall ligado (`sudo apt-get install ufw; sudo systemctl start
kalango-firewall`), a lista do apt atualizada (para Atualizações mostrar as
notas da versão), as Configurações abertas uma vez na sessão e parada na tela
de login (senha "live").

  python3 video/rapido.py            grava e monta
  python3 video/rapido.py --montar   só monta a partir da última gravação

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
from vm import Gravador, Qmp, Vnc  # noqa: E402

CACHE = os.path.join(VIDEO, ".cache")
SAIDA = os.path.join(VIDEO, "saida")
BRUTO = os.path.join(CACHE, "rapido.mkv")
MARCAS = os.path.join(CACHE, "rapido.json")
W, H = 1920, 1080
CAPA_SEG = 2.5  # cartão de abertura
FIM_SEG = 4.0  # cartão final

# Capítulos (id, português, inglês) e legendas de cada um.
CAPITULOS = {
    "login": ("Tela de login", "Login",
              "Tela de login própria, com senha ou PIN",
              "Its own login screen, with a password or a PIN"),
    "menu": ("Menu de apps", "App menu",
             "Tecla Super: todos os apps numa lista só, com rolagem e busca",
             "Super key: every app in a single list, with scrolling and search"),
    "icones": ("Ícones Mandacaru", "Mandacaru icons",
               "Ícones Mandacaru: Barro, Caatinga e Noite do sertão",
               "Mandacaru icons: Clay, Caatinga and Night"),
    "cores": ("Cores ao vivo", "Live colors",
              "Cores e temas mudam o sistema inteiro na hora",
              "Colors and themes change the whole system instantly"),
    "firewall": ("Firewall", "Firewall",
                 "Firewall ligado: avisa quando um programa quer receber conexões",
                 "Firewall on: it asks when a program wants incoming connections"),
    "atualizacoes": ("Atualizações", "Updates",
                     "Atualizações com as novidades da versão, sem reiniciar",
                     "Updates with release notes, no restart needed"),
}
ORDEM = ["login", "menu", "icones", "cores", "firewall", "atualizacoes"]


class Diretor:
    def __init__(self, q, rec):
        self.q, self.rec = q, rec
        self.marcas = []  # (id, início)
        self.cortes = []  # (início, fim) das esperas

    @contextlib.contextmanager
    def corte(self):
        """Espera que sai do vídeo final (corte seco)."""
        inicio = self.rec.agora()
        yield
        self.cortes.append((inicio, self.rec.agora()))

    def cena(self, cid):
        t = self.rec.agora()
        self.marcas.append((cid, t))
        print(f"{t:6.1f}s  {cid}", flush=True)


def p(seg):
    time.sleep(seg)


def roteiro(d):
    q = d.q
    d.cena("login")
    p(1.0)
    q.clicar(930, 735, dur=0.9)
    p(0.3)
    q.digitar("live", 0.18)
    p(0.4)
    q.clicar(1092, 735, dur=0.5)
    with d.corte():
        p(6.5)  # a área de trabalho abre
    p(1.2)

    d.cena("menu")
    q.mover(1150, 700, 0.6)
    q.teclas("meta_l", "spc")
    p(1.6)
    q.mover(664, 290, 0.7)
    q.mover(1258, 402, 1.0)
    q.mover(960, 450, 0.5)
    for _ in range(4):
        q.rolar(3)
        p(0.55)
    p(0.6)
    q.rolar(-14)
    p(0.6)
    q.digitar("calc", 0.17)
    p(1.4)
    q.teclas("esc")
    p(0.5)

    d.cena("icones")
    q.teclas("meta_l", "spc")
    p(0.8)
    q.digitar("settings", 0.1)
    p(0.5)
    q.teclas("ret")
    with d.corte():
        p(4.5)  # as Configurações abrem
    q.clicar(324, 368, dur=0.7)  # Barra de tarefas
    p(0.8)
    q.rolar(6, 930, 700)
    p(0.8)
    for x in (1054, 1140, 1224):  # Barro, Caatinga, Noite
        q.clicar(x, 700, dur=0.6)
        p(1.7)
    q.clicar(1140, 700, dur=0.5)  # fica na Caatinga
    p(1.0)

    d.cena("cores")
    q.clicar(340, 326, dur=0.7)  # Aparência
    p(0.9)
    for x in (820, 762, 588):  # âmbar, verde, ciano
        q.clicar(x, 486, dur=0.6)
        p(1.1)
    p(0.4)

    d.cena("firewall")
    q.clicar(322, 620, dur=0.7)  # Compartilhamento
    p(0.6)
    q.mover(1244, 272, 0.7)
    p(1.2)
    q.teclas("meta_l", "ret")
    p(1.6)
    q.digitar("python3 -m http.server 8000\n", 0.06)
    with d.corte():
        p(12.0)  # o aviso aparece (duas voltas de 5 s)
    p(1.0)
    q.mover(1084, 602, 0.9)
    p(0.5)
    q.clicar()
    p(1.3)

    d.cena("atualizacoes")
    q.clicar(326, 704, dur=0.9)  # Atualizações (barra lateral, à mostra)
    with d.corte():
        p(2.5)  # a lista do apt
    q.mover(700, 380, 0.8)
    q.mover(1100, 440, 1.2)
    p(2.5)


def gravar():
    os.makedirs(CACHE, exist_ok=True)
    vnc = Vnc(os.path.join(CACHE, "vnc.sock"))
    p(1.0)
    q = Qmp(os.path.join(CACHE, "qmp.sock"), W, H)
    rec = Gravador(vnc, BRUTO)
    d = Diretor(q, rec)
    try:
        roteiro(d)
    finally:
        total = rec.agora()
        rec.parar()
        q.fechar()
        vnc.fechar()
    json.dump({"marcas": d.marcas, "cortes": d.cortes, "total": total}, open(MARCAS, "w"), indent=1)


def cartao(caminho, linhas):
    """Cartão de abertura/fim: fundo do tema, ícone e textos."""
    from PIL import Image, ImageDraw, ImageFont

    img = Image.new("RGB", (W, H), (7, 9, 13))
    d = ImageDraw.Draw(img)
    for y in range(H):  # brilho suave no centro
        a = max(0, 1 - abs(y - H * 0.45) / (H * 0.6))
        d.line([(0, y), (W, y)], fill=(int(7 + 10 * a), int(9 + 22 * a), int(13 + 30 * a)))
    icone = Image.open(os.path.join(os.path.dirname(VIDEO), "ui/brand/kalango-icon.png")).convert("RGBA")
    icone = icone.resize((180, 180), Image.LANCZOS)
    img.paste(icone, ((W - 180) // 2, 250), icone)
    fontes = "/usr/share/fonts/opentype/inter"
    y = 470
    for texto, tam, cor, peso in linhas:
        f = ImageFont.truetype(f"{fontes}/Inter-{peso}.otf", tam)
        w = d.textlength(texto, font=f)
        d.text(((W - w) / 2, y), texto, font=f, fill=cor)
        y += int(tam * 1.5)
    img.save(caminho)


def vtt_tempo(t):
    h, r = divmod(t, 3600)
    m, s = divmod(r, 60)
    return f"{int(h):02d}:{int(m):02d}:{s:06.3f}"


def montar():
    dados = json.load(open(MARCAS))
    cortes, total = dados["cortes"], dados["total"]

    def mapear(t):
        """Tempo da gravação → tempo no vídeo final (sem os cortes, com a capa)."""
        tirado = sum(min(t, b) - a for a, b in cortes if t > a)
        return CAPA_SEG + t - tirado

    duracao = mapear(total) + FIM_SEG
    os.makedirs(SAIDA, exist_ok=True)
    capa = os.path.join(CACHE, "rapido-capa.png")
    fim = os.path.join(CACHE, "rapido-fim.png")
    cartao(capa, [("KalangOS 0.1 Mandacaru", 64, (230, 237, 243), "SemiBold"),
                  ("Debian 13 · Rust · GPL", 30, (139, 152, 165), "Regular")])
    cartao(fim, [("KalangOS", 72, (230, 237, 243), "SemiBold"),
                 ("kalangos.org", 40, (61, 214, 245), "Medium")])
    sel = "+".join(f"between(t,{a:.3f},{b:.3f})" for a, b in cortes) or "0"
    saida = os.path.join(SAIDA, "kalango-rapido.mp4")
    subprocess.run([
        "ffmpeg", "-loglevel", "error", "-y",
        "-loop", "1", "-t", str(CAPA_SEG), "-framerate", "30", "-i", capa,
        "-i", BRUTO,
        "-loop", "1", "-t", str(FIM_SEG), "-framerate", "30", "-i", fim,
        "-filter_complex",
        f"[1:v]select='not({sel})',setpts=N/30/TB,format=yuv420p[g];"
        "[0:v]format=yuv420p,fade=t=out:st=2.0:d=0.5[c];"
        "[2:v]format=yuv420p,fade=t=in:st=0:d=0.5[f];"
        "[c][g][f]concat=n=3:v=1:a=0[v]",
        "-map", "[v]", "-r", "30", "-c:v", "libx264", "-preset", "slow", "-crf", "27",
        "-pix_fmt", "yuv420p", "-movflags", "+faststart", saida,
    ], check=True)
    # Capa: um quadro do menu de apps (bonito e explica o sistema).
    menu = next(t for c, t in dados["marcas"] if c == "menu")
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-ss", f"{mapear(menu) + 3.2:.2f}", "-i", saida,
                    "-frames:v", "1", "-vf", "scale=1280:-2", "-q:v", "4",
                    os.path.join(SAIDA, "kalango-rapido.jpg")], check=True)
    # Legendas e capítulos.
    inicios = [(c, mapear(t)) for c, t in dados["marcas"]]
    for lingua, i in (("pt", 2), ("en", 3)):
        linhas = ["WEBVTT", ""]
        for k, (cid, t) in enumerate(inicios):
            ate = inicios[k + 1][1] if k + 1 < len(inicios) else duracao - FIM_SEG
            linhas += [f"{vtt_tempo(t + 0.2)} --> {vtt_tempo(ate - 0.2)}", CAPITULOS[cid][i], ""]
        open(os.path.join(SAIDA, f"kalango-rapido.{lingua}.vtt"), "w").write("\n".join(linhas))
    capitulos = [{"id": c, "inicio": round(t, 2), "pt": CAPITULOS[c][0], "en": CAPITULOS[c][1]} for c, t in inicios]
    json.dump({"duracao": round(duracao, 2), "capitulos": capitulos},
              open(os.path.join(SAIDA, "kalango-rapido.capitulos.json"), "w"), ensure_ascii=False, indent=1)
    print(f"pronto: {saida} ({duracao:.1f} s)")


if __name__ == "__main__":
    if "--montar" not in sys.argv:
        gravar()
    montar()
