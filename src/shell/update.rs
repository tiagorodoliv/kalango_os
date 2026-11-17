//! Interface atualizada sem reiniciar: recarregar a área de trabalho.
//!
//! O `kalango-shell` roda separado dos apps (cada app é um processo do
//! labwc), então trocar a versão da interface não precisa reiniciar o
//! computador nem sair da sessão. Quando o apt troca o binário, o arquivo
//! que este processo executa é apagado (`/proc/self/exe` termina em
//! " (deleted)"). A área de trabalho percebe isso, pergunta se pode
//! recarregar e, com o "sim", sai com [`RELOAD_CODE`]: o supervisor
//! (`/usr/local/bin/kalango-shell-sessao`) abre a versão nova no lugar. A
//! dock, os fundos e a faixa da dock escondida novos fecham os antigos
//! sozinhos (`session::claim_instance`); o Alt+Tab antigo é fechado aqui.
//! O menu, o painel e as Configurações abrem cada vez como processo novo e
//! já pegam a versão nova.
//!
//! Sem o supervisor (sessão iniciada por um autostart antigo), a área de
//! trabalho se troca pela versão nova com `exec` (mesmo PID).
//!
//! O supervisor também volta para a versão anterior se a nova cair ao
//! iniciar; nesse caso ele passa `KALANGO_SHELL_ANTERIOR=1` e [`start`]
//! avisa a pessoa.

use std::process::{Command, Stdio};
use std::time::Duration;

use crate::common::i18n::tr;

/// Código de saída que pede ao supervisor para abrir a versão instalada agora.
pub const RELOAD_CODE: i32 = 75;

/// Binário instalado pelo pacote.
const INSTALLED: &str = "/usr/local/bin/kalango-shell";

/// A área de trabalho roda sob o `kalango-shell-sessao`.
pub fn supervised() -> bool {
    std::env::var_os("KALANGO_SUPERVISIONADO").is_some()
}

/// O binário deste processo foi trocado (atualização instalada).
pub fn exe_replaced() -> bool {
    std::fs::read_link("/proc/self/exe").is_ok_and(|p| p.to_string_lossy().ends_with(" (deleted)"))
}

/// Versão instalada do pacote, para o aviso ("0.1.1-3").
fn installed_version() -> Option<String> {
    let out = Command::new("dpkg-query")
        .args(["-W", "-f=${Version}", "kalango-shell"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!v.is_empty()).then_some(v)
}

/// Processos do Alt+Tab (`--alternar=iniciar`) desta pessoa: o novo só abre
/// se o antigo não estiver ouvindo, então o antigo sai na recarga.
fn switcher_pids() -> Vec<u32> {
    let me = std::process::id();
    let Ok(dir) = std::fs::read_dir("/proc") else { return Vec::new() };
    dir.flatten()
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter(|pid| *pid != me)
        .filter(|pid| {
            let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else { return false };
            let args: Vec<&[u8]> = raw.split(|b| *b == 0).collect();
            let exe = args.first().map(|a| String::from_utf8_lossy(a).into_owned()).unwrap_or_default();
            crate::shell::session::is_shell_binary(std::path::Path::new(&exe))
                && args.get(1).is_some_and(|a| *a == b"--alternar=iniciar")
        })
        .collect()
}

/// Troca a área de trabalho pela versão instalada agora, sem fechar os apps.
pub fn reload() {
    for pid in switcher_pids() {
        let _ = Command::new("kill").arg(pid.to_string()).status();
    }
    if supervised() {
        std::process::exit(RELOAD_CODE);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let exe = if std::path::Path::new(INSTALLED).exists() { INSTALLED.into() } else { crate::shell::session::own_exe() };
        // Só volta se o exec falhar.
        let err = Command::new(exe).exec();
        eprintln!("kalango-shell: não foi possível recarregar: {err}");
    }
}

/// Pergunta se pode recarregar agora. `true` = sim.
fn ask(version: Option<&str>) -> bool {
    let title = tr("KalangOS interface updated", "Interface do KalangOS atualizada");
    let what = match version {
        Some(v) => format!("{} {v}.", tr("A new version of the interface was installed:", "Uma versão nova da interface foi instalada:")),
        None => tr("A new version of the interface was installed.", "Uma versão nova da interface foi instalada.").to_string(),
    };
    let text = format!(
        "{what}\n\n{}",
        tr(
            "Reload now? The bar and the dock disappear for a second; your open apps stay as they are. \
             No need to restart the computer.",
            "Recarregar agora? A barra e a dock somem por um segundo; os apps abertos continuam como estão. \
             Não é preciso reiniciar o computador.",
        )
    );
    Command::new("zenity")
        .args([
            "--question",
            "--no-markup",
            "--width=460",
            "--icon=system-software-update",
            &format!("--title={title}"),
            &format!("--text={text}"),
            &format!("--ok-label={}", tr("Reload now", "Recarregar agora")),
            &format!("--cancel-label={}", tr("Later", "Depois")),
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Aviso de que a versão nova não abriu e a anterior está em uso.
fn warn_previous() {
    let text = tr(
        "The new version of the interface failed to start, so KalangOS went back to the previous one. \
         Your system is working normally. Please report it at github.com/tiagorodoliv/kalango_os/issues.",
        "A versão nova da interface não abriu, então o KalangOS voltou para a anterior. \
         O sistema está funcionando normalmente. Se puder, relate em github.com/tiagorodoliv/kalango_os/issues.",
    );
    let _ = Command::new("zenity")
        .args(["--warning", "--no-markup", "--width=460", &format!("--text={text}")])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Área de trabalho da sessão real: avisa se está na versão anterior e
/// confere a cada 5 s se uma atualização trocou o binário.
pub fn start() {
    std::thread::spawn(|| {
        if std::env::var_os("KALANGO_SHELL_ANTERIOR").is_some() {
            std::thread::sleep(Duration::from_secs(5));
            warn_previous();
        }
        loop {
            std::thread::sleep(Duration::from_secs(5));
            if !exe_replaced() {
                continue;
            }
            // Espera o apt terminar de configurar o pacote.
            std::thread::sleep(Duration::from_secs(3));
            if ask(installed_version().as_deref()) {
                reload();
            }
            // "Depois": não pergunta de novo nesta sessão; a versão nova
            // vale na próxima vez que a pessoa entrar.
            return;
        }
    });
}
