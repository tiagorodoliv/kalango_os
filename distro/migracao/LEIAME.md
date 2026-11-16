# Migrar de outro Linux (Ubuntu) para o KalangOS

Dois scripts para levar a pasta pessoal e as conexões de rede antes de formatar:

```sh
# 1. No sistema antigo, com um disco externo montado (NÃO o disco que será formatado):
distro/migracao/fazer-backup.sh --simular                   # o que entra e o tamanho
distro/migracao/fazer-backup.sh --senha /media/$USER/HD     # cria o backup criptografado

# 2. No KalangOS instalado:
./restaurar-backup.sh /media/$USER/HD/backup-<usuário>-<data>.tar.zst.gpg
```

**Entra:** a pasta pessoal inteira (projetos com o histórico do git, chaves SSH e GPG,
chaveiro de senhas, Claude, navegadores, VS Code, documentos...), as conexões salvas do
NetworkManager (Wi-Fi e VPN) e listas dos programas instalados.

**Fica de fora** (baixa ou recompila de novo): caches, `target/` do Rust, `node_modules`,
Gradle/npm/pub, rustup, e — sem `--tudo` — ESP-IDF e SDK do Android.

**Na restauração:** não sobrescreve o que é do KalangOS (tema, painel, `.zshrc`, configurações
do GNOME do Ubuntu); ajusta as permissões das chaves; recoloca Wi-Fi e VPN; reinstala as
extensões do VS Code. O perfil do Firefox em snap fica guardado (o Firefox ESR não abre perfis
de versões mais novas: use o Firefox Sync ou o Firefox do Flathub).

Com `--senha`, guarde a senha: sem ela não há como restaurar.
