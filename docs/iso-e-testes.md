# ISO e testes

## Gerar as ISOs

O jeito simples (pede o sudo uma vez no começo):

```sh
distro/gerar-isos.sh todas            # padrão e HWE
distro/gerar-isos.sh hwe --enviar     # só a HWE, e envia ao site
```

Ele usa o `kalango-shell` mais novo **já publicado no apt**, para a ISO e as
atualizações terem o mesmo arquivo. Gera todas as edições e só depois envia. Uma
ISO leva ~20 a 40 minutos para gerar, e o envio depende da conexão (2,3 GB).

Saída: `distro/out/kalango-os-<versão>-<codinome>[-hwe].iso` e o `.sha256`. O
script **não** muda a página do site: ponha o link e o SHA-256 em
`kalangos-site/public/{,pt/}index.html`.

## Como o `build.sh` funciona

O Ubuntu não serve para montar uma ISO do Debian 13 (o `live-build` dele é de
2012, e a glibc é mais nova). Por isso tudo roda num **chroot** Debian 13 em
`distro/.cache/builder`:

1. Cria o chroot na primeira vez (~1 GB baixado).
2. Compila o shell lá dentro, ou usa o pacote publicado (`KALANGO_DEB=`).
3. Gera o pacote de papéis de parede (reprodutível: sai igual ao publicado).
4. Baixa os temas de ícones e o módulo da tela de bloqueio, cada um com **versão
   e SHA-256 fixos** (`distro/icones/`, `distro/gtklock/`).
5. Monta a ISO com o `live-build` do Debian (`lb config` + `lb build`), usando
   `distro/auto/config`, as listas de pacotes, os `includes` e os hooks.
6. Tenta o `lb build` até 3 vezes (às vezes falha ao desmontar o `/proc`; ele
   retoma de onde parou).

Cuidados:

- **Não edite o `build.sh` (nem o `gerar-isos.sh`) com ele rodando.** O `sh` lê o
  script aos poucos e pode misturar a versão velha com a nova.
- **Nunca apague o `distro/.cache/builder` com o script rodando.** Há `/dev`,
  `/proc` e `/sys` montados lá dentro.
- Cada ISO com pacote compilado na hora precisa de **revisão nova**
  (`KALANGO_REVISAO=N`). A mesma versão com outro conteúdo faz o apt da ISO
  tentar "voltar de versão" e o build falha no fim. O `build.sh` confere isso no
  início.

Variáveis do `build.sh`:

| Variável | Para |
|---|---|
| `KALANGO_EDICAO=hwe` | edição HWE (kernel e vídeo dos backports) |
| `KALANGO_DEB=<.deb>` | usar um pacote pronto em vez de compilar |
| `KALANGO_REVISAO=N` | revisão do pacote compilado |
| `KALANGO_MIRROR=<url>` | espelho do Debian mais perto |

## A ISO por dentro

- Debian 13 amd64, repositórios `main`, `contrib`, `non-free-firmware`.
- Idiomas inglês e português; teclado americano; fuso UTC (a pessoa escolhe no
  instalador).
- Live: usuário `user`, senha `live`, entra direto. Ctrl+Alt+F2 abre um console.
- Instalador **Calamares** com a marca do KalangOS (`etc/calamares/`). Sem
  partição de swap por padrão.
- Boot por BIOS e UEFI, com ou sem Secure Boot.

## Testar sem gerar ISO

```sh
distro/testar-vm.sh                 # compila (perfil vm, ~12 s) e abre a ISO no QEMU com o binário novo
distro/testar-vm.sh --so-preparar   # só compila; na VM: sudo kalango-dev-aplicar --reiniciar
```

O binário novo passa para a VM por uma pasta compartilhada (9p). Na ISO live, o
`kalango-dev.service` o aplica antes da tela de login, e só se existir a pasta
compartilhada.

## Testar uma ISO

```sh
distro/run-qemu.sh                  # BIOS, sem aceleração 3D
distro/run-qemu.sh --uefi           # UEFI
distro/run-qemu.sh --secureboot     # UEFI com Secure Boot
distro/run-qemu.sh --disco          # com um disco de 20 GB para testar o instalador
distro/run-qemu.sh --so-disco       # boot pelo disco já instalado
distro/run-qemu.sh --gl             # com aceleração 3D (virgl)
```

Wi-Fi falso na VM, para testar a página de rede: `mac80211_hwsim` + `hostapd`
(passo a passo no `CLAUDE.md`, seção ISO).

## Testar numa máquina de verdade

- Sempre na VM primeiro, principalmente o que mexe em bloqueio de tela e login: um
  erro ali pode deixar a tela presa.
- Para trocar só o binário numa máquina instalada: copiar para
  `/usr/local/bin/kalango-shell` e matar a área de trabalho (o supervisor a reabre
  com o binário novo). Isso dispara o aviso de integridade: aceite lá.
