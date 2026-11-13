"""Textos da narração do vídeo de apresentação (inglês) e síntese com o Piper.

Cada cena tem um id e um texto. `sintetizar()` gera video/.cache/narr/<id>.wav
(só refaz quando o texto muda) e devolve a duração de cada um em segundos.
"""

import hashlib
import json
import os
import subprocess
import wave

VIDEO = os.path.dirname(os.path.abspath(__file__))
CACHE = os.path.join(VIDEO, ".cache")
PIPER = os.path.join(CACHE, "piper", "piper")
VOZ = os.path.join(CACHE, "en_US-lessac-high.onnx")

CENAS = [
    ("intro", "This is Kalango OS: a Linux desktop built on Debian 13, with its own shell, "
              "written in Rust. Let's take a quick tour."),
    ("login", "Everything starts at the login screen. Type your password, and you're in."),
    ("desktop", "The desktop is clean and focused. At the top: the Kalango button, the clock, "
                "and the system status: processor, network, sound and power. "
                "At the bottom, the dock, with your favorite apps."),
    ("menu", "Press the Super key to open the app menu, with your pinned apps and your recent files."),
    ("todos", "All apps lists every program installed on the system, "
              "including the ones you add later from the Store."),
    ("busca", "Or just start typing to search, and press Enter to launch."),
    ("arquivos", "Files is fast and lightweight, and it is themed to match the rest of the system."),
    ("terminal", "Super plus Enter opens the terminal, in the same colors, with the JetBrains Mono font."),
    ("aparencia", "Settings is where you make Kalango yours. In Appearance, pick a theme and an accent color. "
                  "Every change is applied live, to the whole system."),
    ("icones", "Choose a wallpaper, and an icon theme: Flat Remix, Colloid, or Fluent. "
               "The dock and your apps follow along instantly."),
    ("dock", "In Taskbar, move the dock to the left or to the right, or turn it into a full width panel."),
    ("sistema", "Wi-Fi, sound, displays, power, and updates are real settings, "
                "built on standard Linux tools like Network Manager and PipeWire."),
    ("idioma", "Kalango OS speaks English by default, with Portuguese built in. "
               "The keyboard layout switches instantly, and the language follows on your next sign in."),
    ("direito", "A right click on the desktop gives you quick shortcuts: wallpaper, dock, displays, "
                "and the terminal."),
    ("loja", "The Store brings thousands of apps from Flathub. Browse by category, "
             "and install any app with a single click."),
    ("bloqueio", "And Super plus L locks the screen, whenever you step away."),
    ("fim", "Kalango OS is free and open source software, licensed under the GPL. "
            "Thanks for watching."),
]


def duracao(caminho):
    with wave.open(caminho) as w:
        return w.getnframes() / w.getframerate()


def sintetizar():
    """Gera os .wav que faltam; devolve {id: (caminho, duração)}."""
    pasta = os.path.join(CACHE, "narr")
    os.makedirs(pasta, exist_ok=True)
    indice_arq = os.path.join(pasta, "indice.json")
    indice = json.load(open(indice_arq)) if os.path.exists(indice_arq) else {}
    out = {}
    for cid, texto in CENAS:
        arq = os.path.join(pasta, f"{cid}.wav")
        h = hashlib.sha256(texto.encode()).hexdigest()
        if indice.get(cid) != h or not os.path.exists(arq):
            subprocess.run(
                [PIPER, "--model", VOZ, "--output_file", arq, "--sentence_silence", "0.35",
                 "--length_scale", "1.05"],
                input=texto.encode(), check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
            )
            indice[cid] = h
        out[cid] = (arq, duracao(arq))
    json.dump(indice, open(indice_arq, "w"), indent=1)
    return out


if __name__ == "__main__":
    total = 0
    for cid, (arq, d) in sintetizar().items():
        total += d
        print(f"{cid:10} {d:5.1f}s")
    print(f"total      {total:5.1f}s")
