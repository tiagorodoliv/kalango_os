# Documentação do KalangOS

Guia para entender, manter e proteger este repositório. Escrito para quem vai
mexer no código: o mantenedor e quem contribuir.

O KalangOS é uma distribuição Linux baseada no **Debian 13 "trixie"**. O que é
próprio do projeto é a **interface** (barra, dock, menu, Configurações, tela de
login…), escrita em **Rust** com a biblioteca de interface **Slint**, mais os
**scripts e configurações** que transformam um Debian comum no KalangOS. Todo o
resto (kernel, drivers, programas) vem do Debian.

## Por onde começar

| Quero… | Leia |
|---|---|
| Entender como as peças se encaixam | [Arquitetura](arquitetura.md) |
| Aprender a linguagem e a ferramenta da interface | [Rust e Slint](rust-e-slint.md) |
| Achar o arquivo que faz uma coisa | [Mapa do código](mapa-do-codigo.md) |
| Entender o que roda como root, hardware, energia, rede | [Sistema e hardware](sistema-e-hardware.md) |
| Publicar uma atualização ou entender o apt do projeto | [Pacotes e atualizações](pacotes-e-atualizacoes.md) |
| Gerar a ISO ou testar numa máquina virtual | [ISO e testes](iso-e-testes.md) |
| Manter o projeto sem vulnerabilidades | [Segurança](seguranca.md) |
| Saber o que fazer toda semana, mês e versão | [Manutenção](manutencao.md) |

Além destes, há guias curtos ao lado do que explicam:

- `distro/repo/LEIAME.md`: repositório apt (chave, publicar, servidor).
- `distro/migracao/LEIAME.md`: backup e restauração ao reinstalar.
- `CONTRIBUTING.md`, `SECURITY.md`, `TRADEMARKS.md`: regras de contribuição,
  como relatar falhas de segurança e uso do nome e do logo.
- `CLAUDE.md`: o diário técnico das decisões, com o porquê de cada detalhe.
  É longo e denso; esta pasta é a versão organizada para leitura.

## Estrutura do repositório

```
kalango_os/
├── src/            código Rust da interface (o binário kalango-shell)
│   ├── common/     relógio, idioma, CPU/bateria/rede da barra
│   ├── launcher/   menu de apps, apps instalados, arquivos recentes
│   ├── shell/      processos da sessão: dock, login, Alt+Tab, monitores…
│   ├── settings/   o que fica por trás de cada página das Configurações
│   └── tools/      apps próprios (Calculadora)
├── ui/             telas em linguagem Slint (.slint), compiladas junto
├── lang/           tradução pt_BR (.po); o texto original é inglês
├── assets/         logo, papéis de parede e os scripts que os geram
├── distro/         tudo que vira sistema: ISO, pacotes, scripts de root,
│   ├── config/     configurações e scripts copiados para o sistema
│   ├── pacotes/    geradores dos pacotes .deb
│   ├── repo/       repositório apt (publicar, chave, servidor)
│   └── build.sh    gera a ISO
├── examples/       snapshot.rs: gera imagens das telas (usado no CI)
├── build.rs        compila os .slint e embute as traduções
└── Cargo.toml      dependências e perfis de compilação do Rust
```

## Glossário rápido

| Termo | O que é |
|---|---|
| **shell** | A interface gráfica do KalangOS (não confundir com o terminal `bash`/`zsh`). O binário se chama `kalango-shell`. |
| **labwc** | O *compositor* Wayland: desenha as janelas na tela, cuida de foco, atalhos e bordas. O shell roda dentro dele. |
| **Wayland / X11** | Protocolos de janelas. O KalangOS usa Wayland; programas antigos rodam pelo **Xwayland**. |
| **greetd** | O serviço da tela de login. |
| **polkit / pkexec** | O sistema que pede a senha de administrador antes de um programa rodar como root. |
| **live-build** | Ferramenta do Debian que monta a ISO. |
| **apt / .deb** | Gerenciador de pacotes do Debian e o formato dos pacotes. |
| **ISO live** | O sistema rodando pelo pendrive, sem instalar. |
| **HWE** | *Hardware Enablement*: a edição da ISO com kernel e drivers de vídeo mais novos. |
| **Calamares** | O instalador gráfico. |
