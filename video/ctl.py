"""Comandos avulsos na VM do vídeo (para montar o roteiro).

  python3 video/ctl.py foto saida.png [escala]
  python3 video/ctl.py clicar X Y [left|right]
  python3 video/ctl.py mover X Y
  python3 video/ctl.py teclas meta_l ret
  python3 video/ctl.py digitar "texto"
"""
import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from vm import Qmp, Vnc  # noqa: E402

CACHE = os.path.join(os.path.dirname(os.path.abspath(__file__)), ".cache")
cmd, args = sys.argv[1], sys.argv[2:]
if cmd == "foto":
    v = Vnc(os.path.join(CACHE, "vnc.sock"))
    time.sleep(0.8)
    v.foto(args[0], float(args[1]) if len(args) > 1 else 1.0)
    v.fechar()
else:
    q = Qmp(os.path.join(CACHE, "qmp.sock"), 1920, 1080)
    if cmd == "clicar":
        q.x, q.y = int(args[0]), int(args[1])
        q._abs(q.x, q.y)
        time.sleep(0.15)
        q.clicar(botao=args[2] if len(args) > 2 else "left")
    elif cmd == "mover":
        q._abs(int(args[0]), int(args[1]))
    elif cmd == "teclas":
        q.teclas(*args)
    elif cmd == "digitar":
        q.digitar(args[0])
    q.fechar()
