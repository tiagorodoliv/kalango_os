"""Controle da VM do Kalango OS para gravar o vídeo de apresentação.

- `Qmp`: mouse (tablet USB, posição absoluta) e teclado pelo QMP do QEMU.
- `Vnc`: cliente VNC mínimo (RFB 3.8, sem senha, Raw/CopyRect) que mantém
  uma cópia da tela; serve para tirar fotos e para gravar.
- `Gravador`: manda a tela para o ffmpeg em taxa fixa (30 fps), repetindo o
  quadro atual quando nada muda.

Só biblioteca padrão + Pillow (fotos). O teclado da VM precisa estar no
layout us (os códigos de tecla são os de um teclado americano).
"""

import json
import os
import socket
import struct
import subprocess
import threading
import time

# Caractere -> tecla do QEMU (qcode), teclado us. Maiúsculas viram shift+letra.
TECLAS = {
    " ": "spc", "\n": "ret", "-": "minus", "=": "equal", ".": "dot", "/": "slash",
    ",": "comma", ";": "semicolon", "'": "apostrophe", "`": "grave_accent",
    "\\": "backslash", "[": "bracket_left", "]": "bracket_right", "\t": "tab",
}
COM_SHIFT = {
    "_": "minus", "+": "equal", ">": "dot", "?": "slash", "<": "comma", ":": "semicolon",
    '"': "apostrophe", "~": "grave_accent", "|": "backslash", "{": "bracket_left",
    "}": "bracket_right", "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6",
    "&": "7", "*": "8", "(": "9", ")": "0",
}


class Qmp:
    """Conexão QMP (um cliente por vez: abrir, usar e fechar)."""

    def __init__(self, caminho, largura, altura):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(caminho)
        self.arq = self.sock.makefile("rwb")
        self._linha()  # saudação
        self.cmd("qmp_capabilities")
        self.largura, self.altura = largura, altura
        self.x, self.y = largura // 2, altura // 2

    def _linha(self):
        return json.loads(self.arq.readline())

    def cmd(self, nome, **args):
        msg = {"execute": nome}
        if args:
            msg["arguments"] = args
        self.arq.write(json.dumps(msg).encode() + b"\n")
        self.arq.flush()
        while True:
            r = self._linha()
            if "return" in r:
                return r["return"]
            if "error" in r:
                raise RuntimeError(f"{nome}: {r['error']}")

    def fechar(self):
        self.sock.close()

    # --- mouse ---
    def _abs(self, x, y):
        self.x, self.y = x, y
        ev = [
            {"type": "abs", "data": {"axis": "x", "value": int(x * 0x7FFF / (self.largura - 1))}},
            {"type": "abs", "data": {"axis": "y", "value": int(y * 0x7FFF / (self.altura - 1))}},
        ]
        self.cmd("input-send-event", events=ev)

    def mover(self, x, y, dur=0.6):
        """Leva o ponteiro até (x, y) em `dur` segundos, com aceleração suave."""
        x0, y0 = self.x, self.y
        passos = max(1, int(dur * 60))
        for i in range(1, passos + 1):
            t = i / passos
            t = t * t * (3 - 2 * t)  # smoothstep
            self._abs(round(x0 + (x - x0) * t), round(y0 + (y - y0) * t))
            time.sleep(dur / passos)

    def botao(self, botao="left", baixo=True):
        self.cmd("input-send-event", events=[{"type": "btn", "data": {"down": baixo, "button": botao}}])

    def clicar(self, x=None, y=None, botao="left", dur=0.6):
        if x is not None:
            self.mover(x, y, dur)
            time.sleep(0.12)
        self.botao(botao, True)
        time.sleep(0.07)
        self.botao(botao, False)

    def rolar(self, quantos, x=None, y=None):
        """Roda a rodinha: negativo = para cima."""
        if x is not None:
            self.mover(x, y)
        botao = "wheel-down" if quantos > 0 else "wheel-up"
        for _ in range(abs(quantos)):
            self.botao(botao, True)
            self.botao(botao, False)
            time.sleep(0.05)

    # --- teclado ---
    def _tecla(self, qcode, baixo):
        self.cmd("input-send-event", events=[{"type": "key", "data": {"down": baixo, "key": {"type": "qcode", "data": qcode}}}])

    def teclas(self, *qcodes, segurar=0.06):
        """Combinação: teclas("meta_l", "ret") = Super+Enter."""
        for q in qcodes:
            self._tecla(q, True)
            time.sleep(0.02)
        time.sleep(segurar)
        for q in reversed(qcodes):
            self._tecla(q, False)
            time.sleep(0.02)

    def digitar(self, texto, intervalo=0.07):
        for c in texto:
            if c in TECLAS:
                self.teclas(TECLAS[c])
            elif c in COM_SHIFT:
                self.teclas("shift", COM_SHIFT[c])
            elif c.isupper():
                self.teclas("shift", c.lower())
            else:
                self.teclas(c)
            time.sleep(intervalo)


class Vnc:
    """Cliente VNC que mantém a tela em `self.tela` (bytes BGRX)."""

    def __init__(self, caminho):
        self.sock = socket.socket(socket.AF_UNIX)
        self.sock.connect(caminho)
        versao = self._ler(12)
        assert versao.startswith(b"RFB "), versao
        self.sock.sendall(b"RFB 003.008\n")
        n = self._ler(1)[0]
        tipos = self._ler(n)
        assert 1 in tipos, "a VM pede senha no VNC"
        self.sock.sendall(b"\x01")
        assert struct.unpack(">I", self._ler(4))[0] == 0, "VNC recusou"
        self.sock.sendall(b"\x01")  # compartilhado
        self.largura, self.altura = struct.unpack(">HH", self._ler(4))
        self._ler(16)
        self._ler(struct.unpack(">I", self._ler(4))[0])
        # 32 bits, little-endian, R<<16 G<<8 B -> bytes B,G,R,X na memória.
        formato = struct.pack(">BBBBHHHBBB3x", 32, 24, 0, 1, 255, 255, 255, 16, 8, 0)
        self.sock.sendall(b"\x00\x00\x00\x00" + formato)
        codificacoes = [0, 1, -223]  # Raw, CopyRect, DesktopSize
        self.sock.sendall(struct.pack(">BxH", 2, len(codificacoes)) + b"".join(struct.pack(">i", c) for c in codificacoes))
        self.tela = bytearray(self.largura * self.altura * 4)
        self.trava = threading.Lock()
        self.quadros = 0
        self._pedir(incremental=False)
        self.vivo = True
        self.thread = threading.Thread(target=self._laco, daemon=True)
        self.thread.start()

    def _ler(self, n):
        partes, falta = [], n
        while falta:
            p = self.sock.recv(min(falta, 1 << 20))
            if not p:
                raise ConnectionError("VNC fechou")
            partes.append(p)
            falta -= len(p)
        return b"".join(partes)

    def _pedir(self, incremental=True):
        self.sock.sendall(struct.pack(">BBHHHH", 3, 1 if incremental else 0, 0, 0, self.largura, self.altura))

    def _laco(self):
        try:
            while self.vivo:
                tipo = self._ler(1)[0]
                if tipo == 0:
                    self._atualizacao()
                    self._pedir()
                elif tipo == 2:
                    pass  # campainha
                elif tipo == 3:
                    self._ler(3)
                    self._ler(struct.unpack(">I", self._ler(4))[0])
                elif tipo == 1:
                    self._ler(1)
                    _, n = struct.unpack(">HH", self._ler(4))
                    self._ler(n * 6)
                else:
                    raise RuntimeError(f"mensagem VNC desconhecida {tipo}")
        except (ConnectionError, OSError):
            pass

    def _atualizacao(self):
        self._ler(1)
        (n,) = struct.unpack(">H", self._ler(2))
        for _ in range(n):
            x, y, w, h, cod = struct.unpack(">HHHHi", self._ler(12))
            if cod == 0:
                dados = self._ler(w * h * 4)
                with self.trava:
                    passo = self.largura * 4
                    for r in range(h):
                        i = (y + r) * passo + x * 4
                        self.tela[i : i + w * 4] = dados[r * w * 4 : (r + 1) * w * 4]
            elif cod == 1:
                sx, sy = struct.unpack(">HH", self._ler(4))
                with self.trava:
                    passo = self.largura * 4
                    linhas = [bytes(self.tela[(sy + r) * passo + sx * 4 : (sy + r) * passo + (sx + w) * 4]) for r in range(h)]
                    for r, linha in enumerate(linhas):
                        i = (y + r) * passo + x * 4
                        self.tela[i : i + w * 4] = linha
            elif cod == -223:
                with self.trava:
                    self.largura, self.altura = w, h
                    self.tela = bytearray(w * h * 4)
                self._pedir(incremental=False)
            else:
                raise RuntimeError(f"codificação VNC inesperada {cod}")
        self.quadros += 1

    def copia(self):
        with self.trava:
            return bytes(self.tela), self.largura, self.altura

    def foto(self, caminho, escala=1.0):
        from PIL import Image

        dados, w, h = self.copia()
        img = Image.frombytes("RGB", (w, h), dados, "raw", "BGRX")
        if escala != 1.0:
            img = img.resize((int(w * escala), int(h * escala)))
        img.save(caminho)

    def fechar(self):
        self.vivo = False
        self.sock.close()


class Gravador:
    """Grava a tela do `Vnc` num vídeo (sem áudio) em taxa fixa."""

    def __init__(self, vnc, saida, fps=30):
        self.vnc, self.fps = vnc, fps
        _, w, h = vnc.copia()
        self.ffmpeg = subprocess.Popen(
            ["ffmpeg", "-loglevel", "error", "-y", "-f", "rawvideo", "-pix_fmt", "bgr0",
             "-s", f"{w}x{h}", "-r", str(fps), "-i", "-",
             "-c:v", "libx264", "-preset", "ultrafast", "-crf", "14", "-pix_fmt", "yuv444p", saida],
            stdin=subprocess.PIPE,
        )
        self.inicio = time.monotonic()
        self.vivo = True
        self.thread = threading.Thread(target=self._laco, daemon=True)
        self.thread.start()

    def agora(self):
        """Segundos desde o início da gravação."""
        return time.monotonic() - self.inicio

    def _laco(self):
        n = 0
        while self.vivo:
            alvo = self.inicio + n / self.fps
            espera = alvo - time.monotonic()
            if espera > 0:
                time.sleep(espera)
            quadro, _, _ = self.vnc.copia()
            # Atrasado: repete o quadro para o vídeo seguir o relógio real.
            while self.vivo and time.monotonic() >= self.inicio + n / self.fps:
                self.ffmpeg.stdin.write(quadro)
                n += 1
                if time.monotonic() < self.inicio + n / self.fps:
                    break

    def parar(self):
        self.vivo = False
        self.thread.join()
        self.ffmpeg.stdin.close()
        self.ffmpeg.wait()


def esperar_arquivo(caminho, limite=60):
    fim = time.monotonic() + limite
    while not os.path.exists(caminho):
        if time.monotonic() > fim:
            raise TimeoutError(caminho)
        time.sleep(0.2)
