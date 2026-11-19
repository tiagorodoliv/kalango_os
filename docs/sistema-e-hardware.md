# Sistema e hardware

O que o KalangOS muda no Debian, o que roda como root e como o hardware é
tratado.

## Onde ficam as mudanças no sistema

| Caminho no repositório | Vai para | Quando |
|---|---|---|
| `distro/config/includes.chroot/etc/…` | `/etc/…` | **só na ISO** (o apt não sobrescreve `/etc` de quem já instalou) |
| `distro/config/includes.chroot/usr/local/{bin,sbin,lib/kalango}` | `/usr/local/…` | na ISO **e** no pacote `kalango-shell` (chega por atualização) |
| `distro/config/hooks/live/*.hook.chroot` | rodam uma vez | só ao montar a ISO |
| `distro/config/package-lists/kalango.list.chroot` | pacotes instalados | só na ISO |

**Consequência importante:** uma mudança em `/etc` ou um pacote novo na lista
**não chega** a quem já tem o KalangOS instalado. Para chegar por atualização:

- pacote novo → `Depends:` ou `Recommends:` no `distro/pacotes/gerar-deb.sh`;
- arquivo de configuração → o shell grava a versão do usuário (como faz com o
  `rc.xml` do labwc em `labwc::ensure_capture`), ou o `postinst` do pacote
  ajusta, com cuidado para não apagar mudanças da pessoa.

## Scripts que rodam como root

A interface **nunca** roda como root. Quando precisa mudar o sistema, chama um
script pelo `pkexec`, que pede a senha de administrador (polkit). Cada script tem
uma política em `distro/config/includes.chroot/usr/share/polkit-1/actions/`.

| Script (`/usr/local/sbin/`) | Faz | Política |
|---|---|---|
| `kalango-atualizar` | `apt-get update` / `full-upgrade` | `org.kalango.atualizar` (`auth_admin_keep`) |
| `kalango-usuarios` | criar, remover, mudar tipo/nome/senha de contas | `org.kalango.usuarios` |
| `kalango-compartilhar` | ligar o Samba e escolher pastas | `org.kalango.compartilhar` |
| `kalango-firewall` | regras do ufw | `org.kalango.firewall` |
| `kalango-impressoras` | `lpadmin` para quem não é administrador | `org.kalango.impressoras` |
| `kalango-energia` | tampa e botão de ligar (`logind.conf.d`) | `org.kalango.energia` |
| `kalango-swap` | arquivo `/swapfile` | `org.kalango.swap` |
| `kalango-integridade` | aceitar ou restaurar arquivos mudados | `org.kalango.integridade` (`auth_admin`, sem lembrar) |
| `kalango-pin` | conferir o PIN no login (pelo PAM) | — (chamado pelo PAM, não pela interface) |
| `kalango-dev-aplicar` | modo de desenvolvimento | só age na ISO live com a pasta 9p da VM |

`auth_admin_keep` lembra a senha por alguns minutos; `auth_admin` pede toda vez.
A regra `etc/polkit-1/rules.d/50-kalango-trocar.rules` libera, **sem senha**, só
o necessário para "Trocar de usuário" (abrir uma tela de login extra).

As regras de como escrever esses scripts com segurança estão em
[Segurança](seguranca.md#scripts-que-rodam-como-root).

## Scripts da sessão (`/usr/local/bin/`, como a própria pessoa)

| Script | Faz |
|---|---|
| `kalango-session` | prepara o ambiente e abre o labwc (chamado pelo greetd) |
| `kalango-greeter` | labwc da tela de login (como `_greetd`) |
| `kalango-shell-sessao` | supervisor: reabre a área de trabalho se cair, volta à versão anterior após 3 quedas |
| `kalango-bloquear` | tela de bloqueio (gtklock; swaylock se o gtklock cair 3 vezes) |
| `kalango-trocar-usuario` | bloqueia e abre outra tela de login |
| `kalango-midia` | teclas de volume, brilho e player |
| `kalango-terminal` | abre o Ptyxis com o tema do sistema |
| `kalango-instalar` | abre o instalador (só na ISO live) |
| `kalango-ficha` | ficha do sistema no terminal (fastfetch) |

E em `/usr/local/lib/kalango/`: `pam-kalango.sh` (ajustes do PAM), `regiao.sh`
(idioma e teclado escolhidos), `sem-hibernar.sh` (instalador), `greeter-monitores`.

## Hooks da ISO

Rodam dentro do sistema enquanto a ISO é montada (`distro/config/hooks/live/`):

| Hook | Faz |
|---|---|
| `0050-kalango-os-release` | protege o `/usr/lib/os-release` do KalangOS contra atualização do `base-files` |
| `0100-kalango-schemas` | compila os padrões do GSettings (tema escuro, ícones) |
| `0110-kalango-flathub` | liga o Flathub para a Loja (arquivo oficial com a chave) |
| `0120-kalango-compartilhamento` | Samba desligado por padrão e sem compartilhamentos do Debian |
| `0130-kalango-secureboot` | instala o GRUB e o shim assinados no sistema instalado |
| `0140-kalango-zsh` | zsh como shell padrão |
| `0150-kalango-thunar` | tira a ação de papel de parede do próprio Thunar |
| `0160-kalango-motd` | sem o texto do Debian no login pelo terminal |
| `0170-kalango-pam` | senhas de qualquer tamanho e PIN |

## Login, senhas e PIN

- Senhas passam sempre pelo **PAM**. O KalangOS troca a configuração padrão do
  `pam_unix` (`pam-kalango.sh`) para aceitar senhas curtas, como o Ubuntu.
- O **PIN** é guardado como hash SHA-512 em `/var/lib/kalango/pin/<conta>`.
  Vale no login e no desbloqueio, **não** no `sudo` nem no `pkexec`. Cinco erros
  bloqueiam o PIN até entrar com a senha.
- O contador de erros do PIN é gravado como root numa pasta onde todos escrevem:
  por isso abre com `O_NOFOLLOW` (não segue links). Uma versão antiga seguia
  links e deixava uma conta virar dona do `/etc/shadow`; foi corrigido na
  0.1.1-6 e é o exemplo clássico do que conferir nesses scripts.

## Rede

NetworkManager por `nmcli` (sempre com `LC_ALL=C`, porque os estados viriam
traduzidos). WireGuard é criado por D-Bus (`gdbus`), **nunca** por
`nmcli connection import`, que liga a VPN na hora e pode derrubar a rede.
Compartilhamento por Samba + `wsdd2`, desligado por padrão.

## Firewall

`ufw` ligado por padrão: bloqueia o que chega e libera o que sai. Na primeira
vez, libera o que já estava escutando (para não cortar um SSH, por exemplo). A
área de trabalho avisa quando um programa novo passa a esperar conexões e o
firewall o bloqueia.

## Energia

- Apagar a tela, bloquear e suspender por tempo: `swayidle`, rodado e trocado
  pela área de trabalho (`src/settings/power.rs`).
- **App em tela cheia em foco (jogo, vídeo): sem tempos.** O controle de jogo não
  conta como atividade para o labwc, e jogos pelo Proton não pedem para a tela
  ficar acesa.
- Perfis de desempenho: `powerprofilesctl`. Brilho: `/sys/class/backlight` pelo
  logind (sem root).
- Swap: zram (memória comprimida) sempre; arquivo `/swapfile` opcional.

## Hardware

### Kernel e edições

| Edição | Kernel | Vídeo | Para |
|---|---|---|---|
| Padrão | 6.12 LTS do Debian 13 | Mesa do Debian 13 | a maioria dos computadores, inclusive antigos |
| HWE | 7.2 (trixie-backports) | Mesa 26.1 | processadores e placas de 2024 em diante |

A HWE usa o mesmo sistema; só `linux-*`, `firmware-*` e `mesa-*` vêm dos
backports, pela prioridade em `distro/edicoes/hwe.pref`, que também vai para
`/etc/apt/preferences.d` do instalado (continua recebendo kernels novos).

### Renderização da interface

`lib.rs::renderer_for`: com placa de vídeo de verdade (i915, xe, amdgpu, radeon,
nouveau, nvidia), tudo em OpenGL. Sem placa (VM, máquina muito antiga), o
renderizador de software, que gasta muito menos memória (32 MB contra 192 MB
numa VM de 2 GB).

### Casos tratados

| Situação | Solução | Onde |
|---|---|---|
| Wi-Fi/Bluetooth somem ao voltar da suspensão (Intel 3160) | recarrega o driver; se a BIOS bloquear, usa `s2idle` | `usr/lib/systemd/system-sleep/kalango-radios` |
| Cópia para pendrive "termina" antes de gravar | limita o cache de escrita em disco USB e avisa na barra | `etc/udev/rules.d/90-kalango-pendrive.rules`, `src/shell/usb.rs` |
| Controle de jogo aparece no Steam e não funciona | regras `steam-devices` (`/dev/uinput` e hidraw para a sessão) | pacote `steam-devices` |
| Vários monitores | barra e dock no principal, papel de parede nos outros | `src/shell/outputs.rs` |
| Bloqueio de tela em VM com blocos pretos | `WLR_RENDERER=pixman` em VM | `kalango-session` |
| Secure Boot | shim e GRUB assinados pelo Debian | hook `0130` |

### Firmware

A ISO usa o repositório `non-free-firmware` do Debian (Wi-Fi, placas de vídeo,
microcódigo de CPU). Firmware é binário fechado dos fabricantes: vem assinado pelo
Debian e é atualizado pelo apt.
