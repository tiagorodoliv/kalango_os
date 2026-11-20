#!/usr/bin/env python3
"""Gera um PDF único com a documentação de ``docs/``.

Junta os arquivos Markdown na ordem do índice, monta uma página HTML com capa,
sumário e estilo de impressão (fontes Inter e JetBrains Mono, cores do
KalangOS), desenha os diagramas ``mermaid`` e imprime com o Chrome sem janela.

Uso::

    python3 docs/gerar-pdf.py [ARQUIVO.pdf]

Sem argumento, grava ``~/Documents/KalangOS-documentacao.pdf``. Precisa do
módulo ``markdown`` do Python e do ``google-chrome`` (ou ``chromium``). Os
diagramas usam o mermaid baixado do jsDelivr (versão fixa); sem internet eles
saem como texto.
"""

import datetime
import html
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import markdown
from markdown.extensions.toc import slugify_unicode

DOCS = Path(__file__).resolve().parent
RAIZ = DOCS.parent
ORDEM = [
    "README.md",
    "arquitetura.md",
    "rust-e-slint.md",
    "mapa-do-codigo.md",
    "sistema-e-hardware.md",
    "pacotes-e-atualizacoes.md",
    "iso-e-testes.md",
    "seguranca.md",
    "manutencao.md",
]
MERMAID = "https://cdn.jsdelivr.net/npm/mermaid@11.4.1/dist/mermaid.min.js"

CSS = """
@page { size: A4; margin: 18mm 16mm 20mm 16mm;
  @bottom-right { content: counter(page); font: 9pt 'JetBrains Mono', monospace; color: #8b98a5; }
  @bottom-left { content: "KalangOS · Documentação do projeto"; font: 9pt Inter, sans-serif; color: #8b98a5; } }
@page :first { @bottom-right { content: none; } @bottom-left { content: none; } }
* { box-sizing: border-box; }
body { font: 10.5pt/1.55 Inter, 'Noto Sans', sans-serif; color: #1b2430; margin: 0; }
h1, h2, h3 { color: #0d1218; line-height: 1.25; break-after: avoid; }
h1 { font-size: 22pt; margin: 0 0 6mm; padding-bottom: 3mm; border-bottom: 2px solid #3dd6f5; }
h2 { font-size: 14pt; margin: 8mm 0 3mm; }
h3 { font-size: 11.5pt; margin: 6mm 0 2mm; }
p, li { orphans: 3; widows: 3; }
a { color: #0e7490; text-decoration: none; }
code { font: 9pt 'JetBrains Mono', monospace; background: #eef3f7; padding: 0.5px 4px; border-radius: 3px; }
pre { background: #0d1218; color: #e6edf3; padding: 10px 12px; border-radius: 6px; overflow: hidden;
  white-space: pre-wrap; word-break: break-word; break-inside: avoid; }
pre code { background: none; color: inherit; padding: 0; font-size: 8.5pt; }
table { width: 100%; border-collapse: collapse; margin: 3mm 0 5mm; font-size: 9.2pt; break-inside: auto; }
tr { break-inside: avoid; }
th { text-align: left; background: #0d1218; color: #e6edf3; padding: 5px 7px; font-weight: 600; }
td { padding: 5px 7px; border-bottom: 1px solid #d5dde5; vertical-align: top; }
tr:nth-child(even) td { background: #f5f8fa; }
blockquote { margin: 3mm 0; padding: 2mm 4mm; border-left: 3px solid #3dd6f5; background: #f2fbfd; }
ul.task li { list-style: none; }
.capitulo { break-before: page; }
.mermaid { background: #f7fafc; border: 1px solid #d5dde5; border-radius: 6px; padding: 6px; text-align: center;
  color: #1b2430; break-inside: avoid; }
.capa { height: 250mm; display: flex; flex-direction: column; justify-content: center; }
.capa img { width: 34mm; height: 34mm; border-radius: 8mm; margin-bottom: 10mm; }
.capa h1 { border: 0; font-size: 34pt; margin: 0; }
.capa .sub { font-size: 15pt; color: #4b5867; margin: 3mm 0 14mm; }
.capa .meta { font: 10pt 'JetBrains Mono', monospace; color: #4b5867; line-height: 1.9; }
.capa .faixa { height: 3px; width: 60mm; background: #3dd6f5; margin-bottom: 10mm; }
.sumario { break-before: page; }
.sumario ol { font-size: 12pt; line-height: 2; padding-left: 8mm; }
.sumario li span { color: #4b5867; font-size: 10pt; }
"""


def versao() -> str:
    """Versão do sistema, do Cargo.toml (``0.1.1``)."""
    texto = (RAIZ / "Cargo.toml").read_text(encoding="utf-8")
    m = re.search(r'^version\s*=\s*"([^"]+)"', texto, re.M)
    return m.group(1) if m else "?"


def ajustar_links(fonte: str) -> str:
    """Links entre arquivos viram links dentro do PDF (``x.md#y`` → ``#y``)."""
    def troca(m: re.Match) -> str:
        texto, alvo = m.group(1), m.group(2)
        arquivo, _, ancora = alvo.partition("#")
        if arquivo.endswith(".md") and not arquivo.startswith("http"):
            nome = Path(arquivo).name
            if nome in ORDEM:
                return f"[{texto}](#{ancora or 'cap-' + Path(nome).stem})"
            return texto  # arquivo fora de docs/: só o texto
        return m.group(0)
    return re.sub(r"\[([^\]]+)\]\(([^)\s]+)\)", troca, fonte)


def mermaid_em_html(fonte: str) -> str:
    """Blocos ```mermaid viram <pre class="mermaid"> (desenhados pelo mermaid.js)."""
    def troca(m: re.Match) -> str:
        return f'\n<pre class="mermaid">{html.escape(m.group(1))}</pre>\n'
    return re.sub(r"```mermaid\n(.*?)```", troca, fonte, flags=re.S)


def capitulo(nome: str) -> tuple[str, str]:
    """HTML de um arquivo e o título dele."""
    fonte = (DOCS / nome).read_text(encoding="utf-8")
    titulo = fonte.splitlines()[0].lstrip("# ").strip()
    fonte = mermaid_em_html(ajustar_links(fonte))
    corpo = markdown.markdown(
        fonte,
        extensions=["tables", "fenced_code", "sane_lists", "toc"],
        extension_configs={"toc": {"slugify": slugify_unicode}},
    )
    # Listas de tarefas "- [ ]" viram caixinhas.
    corpo = corpo.replace("<li>[ ] ", "<li>☐ ").replace("<li>[x] ", "<li>☑ ")
    return f'<section class="capitulo" id="cap-{Path(nome).stem}">{corpo}</section>', titulo


def main() -> None:
    saida = Path(sys.argv[1]) if len(sys.argv) > 1 else Path.home() / "Documents" / "KalangOS-documentacao.pdf"
    chrome = shutil.which("google-chrome") or shutil.which("chromium")
    if not chrome:
        sys.exit("Precisa do google-chrome ou do chromium.")
    partes, titulos = [], []
    for nome in ORDEM:
        h, t = capitulo(nome)
        partes.append(h)
        titulos.append((nome, t))
    hoje = datetime.date.today().strftime("%d/%m/%Y")
    icone = (RAIZ / "ui/brand/kalango-icon.png").as_uri()
    capa = f"""<div class="capa">
  <img src="{icone}" alt="">
  <div class="faixa"></div>
  <h1>KalangOS</h1>
  <p class="sub">Documentação do projeto: arquitetura, código, sistema, pacotes e segurança</p>
  <p class="meta">Versão {versao()} · Mandacaru<br>Base: Debian 13 "trixie" · Interface em Rust + Slint<br>Gerado em {hoje}</p>
</div>"""
    sumario = '<div class="sumario"><h1>Sumário</h1><ol>' + "".join(
        f'<li><a href="#cap-{Path(n).stem}">{html.escape(t)}</a> <span>({n})</span></li>' for n, t in titulos
    ) + "</ol></div>"
    pagina = f"""<!doctype html><html lang="pt-BR"><head><meta charset="utf-8">
<title>KalangOS · Documentação</title><style>{CSS}</style>
<script src="{MERMAID}"></script>
<script>
  window.addEventListener('load', () => {{
    if (window.mermaid) {{
      mermaid.initialize({{ startOnLoad: false, theme: 'neutral', fontFamily: 'Inter' }});
      mermaid.run({{ querySelector: '.mermaid' }});
    }}
  }});
</script></head><body>{capa}{sumario}{''.join(partes)}</body></html>"""
    with tempfile.TemporaryDirectory() as tmp:
        pagina_html = Path(tmp) / "documentacao.html"
        pagina_html.write_text(pagina, encoding="utf-8")
        saida.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [chrome, "--headless=new", "--disable-gpu", "--no-pdf-header-footer",
             "--allow-file-access-from-files", "--virtual-time-budget=20000",
             f"--print-to-pdf={saida}", pagina_html.as_uri()],
            check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=180,
        )
    print(saida)


if __name__ == "__main__":
    main()
