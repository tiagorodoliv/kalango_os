# Vídeo de apresentação

Gera `saida/kalango-os.mp4` (1920×1080, ~3 min, narrado em inglês) e
`saida/kalango-os.srt`, gravando a ISO de verdade numa VM controlada por script.

## Peças

- `narracao.py`: textos de cada cena e síntese da voz com o
  [Piper](https://github.com/rhasspy/piper) (MIT, roda offline).
- `vm.py`: mouse/teclado pelo QMP do QEMU e captura da tela pelo VNC.
- `vm.sh`: liga a VM sem janela (sockets em `.cache/`).
- `gravar.py`: o roteiro (cliques e teclas de cada cena, com as posições da
  tela de 1920×1080) e a montagem com o ffmpeg: abertura, gravação,
  encerramento, narração, legendas e cortes das esperas (a Loja atualiza o
  catálogo a cada login).
- `ctl.py`: comandos avulsos (foto, clique, tecla) para ajustar o roteiro.

## Como gravar

```sh
# Uma vez: Piper + voz (≈140 MB) em video/.cache
mkdir -p video/.cache && cd video/.cache
curl -fsSL https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz | tar xz
for f in en_US-lessac-high.onnx en_US-lessac-high.onnx.json; do
  curl -fsSLO "https://huggingface.co/rhasspy/piper-voices/resolve/v1.0.0/en/en_US/lessac/high/$f"
done
cd ../..

video/vm.sh            # ISO mais nova de distro/out (+ shell de testar-vm.sh, se houver)
# Esperar a área de trabalho (~80 s), abrir a Loja uma vez e sair da sessão
# (menu → sair): a gravação começa na tela de login, senha "live".
python3 video/gravar.py           # grava e monta
python3 video/gravar.py --montar  # só remonta (ex.: mudou um texto da narração)
video/vm.sh --desligar
```

Com uma ISO anterior a 2026-11-13 (que iniciava em português), use
`video/vm.sh --ingles` e, na VM, `sudo chfn -f User user` e gere o locale
`en_US.UTF-8`.

## Vídeo curto do site (`rapido.py`)

`python3 video/rapido.py` grava ~70 s sem narração (login, menu de apps, ícones
Mandacaru, cores, aviso do firewall, Atualizações com as notas) e monta em
`saida/`: `kalango-rapido.mp4` (H.264, ~1,6 MB), a capa `.jpg`, legendas
`.pt.vtt`/`.en.vtt` e os capítulos em `.capitulos.json`. As esperas (abrir as
Configurações, o aviso do firewall) saem por corte seco. Vai para o site em
`public/assets/video/` (seção "Tour rápido", capítulos clicáveis).

Antes de gravar, na sessão da VM: `sudo apt-get update`, `sudo apt-get install
ufw`, `sudo systemctl start kalango-firewall`, abrir as Configurações uma vez e
sair da sessão (a gravação começa na tela de login). Para gravar de novo na
mesma VM, desfaça a porta liberada: `sudo kalango-firewall remover 8000/tcp`.
Sem `ffmpeg` no computador, o do Debian extraído sem root serve
(`~/.local/kalango-build/ffmpeg/bin`).
