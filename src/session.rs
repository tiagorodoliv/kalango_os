//! Integração com a sessão real do Kalango OS (labwc).
//!
//! O mesmo binário roda em três processos, escolhidos pelo argumento:
//! - sem argumento: a área de trabalho (barra, dock, fundo), sempre atrás dos apps;
//! - `--launcher`: o menu de apps, em tela cheia e transparente, por cima de tudo;
//! - `--settings`: as Configurações, numa janela comum;
//! - `--quick-settings`: o painel rápido do canto da barra (volume, brilho,
//!   Wi-Fi, desligar...), em tela cheia e transparente, por cima de tudo;
//! - `--greeter`: a tela de login do greetd (ver [`crate::greeter`]).
//!
//! O menu e as Configurações são processos próprios porque, sem layer-shell,
//! uma janela "sempre atrás" não consegue mostrar nada por cima dos apps.
//! A variável `KALANGO_SESSION=1` (exportada pelo `kalango-session`)
//! liga o modo real: abrir apps de verdade, desligar pelo systemd etc.

use crate::i18n::tr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

use crate::{AppWindow, Screen, Shell, ShellMode};

/// O que este processo mostra.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Desktop,
    Launcher,
    Settings,
    Greeter,
    QuickSettings,
    /// Dock em janela própria, sempre por cima das janelas (`src/dock.rs`).
    Dock,
    /// Papel de parede de um monitor extra (`--fundo=<saída>`, `src/outputs.rs`).
    Fundo,
    /// Alt+Tab (`--alternar`, `src/switcher.rs`): um processo que fica
    /// aberto (escondido) e mostra as janelas a cada Alt+Tab.
    Switcher,
    /// Menu do botão direito de um app da dock (`--menu-dock=<id>`,
    /// `src/taskbar.rs`).
    DockMenu,
}

impl Mode {
    /// Lê o modo dos argumentos de linha de comando (`--launcher`, `--settings`).
    pub fn from_args<I: IntoIterator<Item = String>>(args: I) -> Mode {
        let mut mode = Mode::Desktop;
        for arg in args {
            match arg.as_str() {
                "--launcher" => mode = Mode::Launcher,
                "--settings" => mode = Mode::Settings,
                "--greeter" => mode = Mode::Greeter,
                "--quick-settings" => mode = Mode::QuickSettings,
                "--dock" => mode = Mode::Dock,
                a if a.starts_with("--fundo=") => mode = Mode::Fundo,
                a if a == "--alternar" || a.starts_with("--alternar=") => mode = Mode::Switcher,
                a if a.starts_with("--menu-dock=") => mode = Mode::DockMenu,
                _ => {}
            }
        }
        mode
    }

    /// `app_id` Wayland da janela; as regras do labwc usam esses nomes
    /// (`distro/config/includes.chroot/etc/xdg/labwc/rc.xml`).
    pub fn app_id(self) -> &'static str {
        match self {
            Mode::Desktop => crate::APP_ID,
            Mode::Launcher => "kalango-launcher",
            Mode::Settings => "kalango-settings",
            Mode::Greeter => "kalango-greeter",
            Mode::QuickSettings => "kalango-quick-settings",
            Mode::Dock => "kalango-dock",
            // O de verdade leva a saída: ver `fundo_app_id`.
            Mode::Fundo => "kalango-fundo",
            Mode::Switcher => "kalango-alternar",
            Mode::DockMenu => "kalango-dock-menu",
        }
    }

    /// Argumento que abre este modo.
    fn arg(self) -> Option<&'static str> {
        match self {
            Mode::Desktop => None,
            Mode::Launcher => Some("--launcher"),
            Mode::Settings => Some("--settings"),
            Mode::Greeter => Some("--greeter"),
            Mode::QuickSettings => Some("--quick-settings"),
            Mode::Dock => Some("--dock"),
            Mode::Fundo | Mode::Switcher | Mode::DockMenu => None,
        }
    }

    fn to_ui(self) -> ShellMode {
        match self {
            Mode::Desktop => ShellMode::Desktop,
            Mode::Launcher => ShellMode::Launcher,
            Mode::Settings => ShellMode::Settings,
            Mode::Greeter => ShellMode::Greeter,
            Mode::QuickSettings => ShellMode::QuickSettings,
            Mode::Dock => ShellMode::Dock,
            Mode::Fundo => ShellMode::Fundo,
            Mode::Switcher => ShellMode::Switcher,
            Mode::DockMenu => ShellMode::DockMenu,
        }
    }
}

/// Diz se estamos na sessão real do Kalango OS (`KALANGO_SESSION=1`).
pub fn is_native() -> bool {
    std::env::var("KALANGO_SESSION").is_ok_and(|v| v == "1")
}

/// Comando que abre um app do catálogo (`Apps.pinned` em `ui/globals.slint`).
///
/// `None` para apps que ainda não existem no sistema. "launcher" e "settings"
/// não estão aqui: são janelas do próprio shell (ver [`open`]).
pub fn command_for(id: &str) -> Option<Vec<String>> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/".into());
    let cmd: Vec<String> = match id {
        "files" => vec!["thunar".into()],
        "terminal" => vec!["foot".into()],
        "browser" => vec!["firefox-esr".into()],
        "store" => vec!["gnome-software".into()],
        "calendar" => vec!["gnome-calendar".into()],
        // Só na ISO live: instalador Calamares (ver kalango-instalar).
        "installer" => vec!["kalango-instalar".into()],
        // Editor de código (Geany com o tema Dark Modern); sem ele, o Mousepad.
        "code" => vec![if find_in_path("geany").is_some() { "geany" } else { "mousepad" }.into()],
        "documents" => vec!["thunar".into(), documents_dir(&home)],
        "images" => vec!["ristretto".into(), pictures_dir(&home)],
        "music" => vec!["parole".into()],
        "monitor" => vec!["foot".into(), "--title=Monitor".into(), "htop".into()],
        // Ícones de Wi-Fi e volume da barra superior.
        "network" => vec!["foot".into(), format!("--title={}", tr("Network & Wi-Fi", "Rede e Wi-Fi")), "nmtui".into()],
        "sound" => vec!["pavucontrol".into()],
        // Rodapé do menu de apps. O kalango-bloquear (gtklock) usa o protocolo
        // de bloqueio do Wayland (ext-session-lock): nenhuma janela aparece por
        // cima dele.
        "lock" => vec!["kalango-bloquear".into()],
        // Encerra o labwc; o greetd volta para a tela de login.
        "logout" => vec!["labwc".into(), "--exit".into()],
        "updates" => vec![
            "foot".into(),
            format!("--title={}", tr("Updates", "Atualizações")),
            "sh".into(),
            "-c".into(),
            "sudo apt-get update && sudo apt-get full-upgrade; \
             echo; read -p '"
                .to_string()
                + tr("Press Enter to close ", "Enter para fechar ")
                + "' _",
        ],
        _ => return None,
    };
    Some(cmd)
}

/// Pasta de documentos do usuário (`XDG_DOCUMENTS_DIR`).
fn documents_dir(home: &str) -> String {
    xdg_dir(home, "XDG_DOCUMENTS_DIR", tr("Documents", "Documentos"))
}

/// Pasta de imagens do usuário (`XDG_PICTURES_DIR`).
fn pictures_dir(home: &str) -> String {
    xdg_dir(home, "XDG_PICTURES_DIR", tr("Pictures", "Imagens"))
}

/// Lê uma pasta de `~/.config/user-dirs.dirs`; se não houver, usa `home/padrão`.
pub(crate) fn xdg_dir(home: &str, key: &str, fallback: &str) -> String {
    let file = Path::new(home).join(".config/user-dirs.dirs");
    fs::read_to_string(file)
        .ok()
        .and_then(|content| parse_user_dir(&content, key, home))
        .unwrap_or_else(|| format!("{home}/{fallback}"))
}

/// Extrai `KEY="$HOME/Pasta"` do conteúdo de `user-dirs.dirs`.
pub fn parse_user_dir(content: &str, key: &str, home: &str) -> Option<String> {
    let line = content
        .lines()
        .find(|l| l.trim_start().starts_with(&format!("{key}=")))?;
    let value = line.split_once('=')?.1.trim().trim_matches('"');
    Some(value.replace("$HOME", home))
}

/// Procura um programa no `PATH` (como o `which`).
fn find_in_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

/// Abre um arquivo no app padrão do tipo dele (`xdg-open`).
pub fn open_file(path: &str) -> bool {
    if !Path::new(path).exists() || find_in_path("xdg-open").is_none() {
        return false;
    }
    let mut cmd = Command::new("xdg-open");
    cmd.arg(path);
    spawn(cmd, path)
}

/// Abre um app ou uma janela do shell. Devolve `false` se não deu para abrir
/// (app inexistente no catálogo ou não instalado).
pub fn open(id: &str) -> bool {
    // "settings:3": Configurações já numa página (menu da área de trabalho).
    let (id, page) = match id.split_once(':') {
        Some(("settings", page)) => ("settings", page.parse::<i32>().ok()),
        _ => (id, None),
    };
    let cmd = match id {
        "launcher" | "settings" | "quick-settings" => {
            let mode = match id {
                "launcher" => Mode::Launcher,
                "settings" => Mode::Settings,
                _ => Mode::QuickSettings,
            };
            let mut c = Command::new(own_exe());
            c.args(mode.arg());
            if let Some(page) = page {
                c.arg(format!("--pagina={page}"));
            }
            c
        }
        _ => {
            // Apps do sistema ("Todos os apps"): comando do `.desktop`.
            let argv = if id.starts_with(crate::desktop::ID_PREFIX) {
                crate::desktop::from_id(id).map(|app| app.command())
            } else {
                command_for(id)
            };
            let Some(argv) = argv else {
                return false;
            };
            if find_in_path(&argv[0]).is_none() {
                eprintln!("kalango-shell: {} não está instalado", argv[0]);
                return false;
            }
            let mut c = Command::new(&argv[0]);
            c.args(&argv[1..]);
            c
        }
    };
    spawn(cmd, id)
}

/// Inicia o processo sem esperar por ele (`what` só aparece nos erros).
fn spawn(mut cmd: Command, what: &str) -> bool {
    // Começa na pasta pessoal e sem herdar a entrada do shell.
    if let Some(home) = std::env::var_os("HOME") {
        cmd.current_dir(home);
    }
    cmd.stdin(Stdio::null());
    match cmd.spawn() {
        Ok(mut child) => {
            // Espera o filho numa thread para ele não virar zumbi ao sair.
            std::thread::spawn(move || child.wait());
            true
        }
        Err(err) => {
            eprintln!("kalango-shell: falha ao abrir {what}: {err}");
            false
        }
    }
}

/// Página pedida para as Configurações (`--pagina=3`).
pub fn page_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<i32> {
    args.into_iter()
        .find_map(|a| a.strip_prefix("--pagina=").and_then(|p| p.parse().ok()))
}

/// Saída e número do monitor de uma janela de fundo
/// (`--fundo=HDMI-A-1 --numero=2`).
pub fn fundo_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<(String, i32)> {
    let mut name = None;
    let mut number = 0;
    for a in args {
        if let Some(n) = a.strip_prefix("--fundo=") {
            name = Some(n.to_string());
        } else if let Some(n) = a.strip_prefix("--numero=") {
            number = n.parse().unwrap_or(0);
        }
    }
    name.map(|n| (n, number))
}

/// `app_id` da janela de fundo de um monitor (as regras do labwc são por saída).
pub fn fundo_app_id(output: &str) -> String {
    format!("kalango-fundo-{output}")
}

/// Tamanho da janela da dock (`--faixa=1366x54`), em px lógicos.
pub fn strip_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<(u32, u32)> {
    args.into_iter().find_map(|a| {
        let (w, h) = a.strip_prefix("--faixa=")?.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    })
}

/// Arquivo com a página pedida às Configurações que já estão abertas.
fn page_request_file() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| Path::new(&d).join("kalango-settings.pagina"))
}

/// Pede às Configurações já abertas que mudem de página (elas conferem o
/// pedido de tempos em tempos, ver [`watch_page_requests`]).
pub fn request_page(page: i32) {
    if let Some(file) = page_request_file() {
        let _ = fs::write(file, page.to_string());
    }
}

/// O executável do próprio shell, para abrir o menu, o painel e as
/// Configurações como processos novos.
///
/// Se o binário foi trocado com o shell rodando (atualização), o caminho
/// antigo vira "... (deleted)" e não existe mais: usa o `kalango-shell` do
/// PATH, que é a versão nova.
pub(crate) fn own_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("kalango-shell"))
}

/// Traz para a frente a janela de outra instância já aberta (Configurações
/// abertas de novo pelo menu, pela dock ou pelo painel).
///
/// No Wayland um app não pode se pôr na frente sozinho; o `wlrctl` usa o
/// protocolo de controle de janelas (wlr-foreign-toplevel), que o labwc tem.
pub fn raise(mode: Mode) {
    let _ = Command::new("wlrctl")
        .args(["toplevel", "focus", &format!("app_id:{}", mode.app_id())])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// Nas Configurações: confere os pedidos de página e troca a página aberta.
/// O timer devolvido precisa ficar vivo.
pub fn watch_page_requests(ui: &AppWindow) -> Option<Timer> {
    let file = page_request_file()?;
    let _ = fs::remove_file(&file);
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_millis(400), move || {
        let Ok(text) = fs::read_to_string(&file) else { return };
        let _ = fs::remove_file(&file);
        if let (Some(ui), Ok(page)) = (weak.upgrade(), text.trim().parse::<i32>()) {
            ui.set_settings_page(page);
        }
    });
    Some(timer)
}

/// Garante uma instância só do launcher, do painel rápido e das Configurações.
///
/// Usa um arquivo com o PID em `$XDG_RUNTIME_DIR`. Se já existe outra
/// instância viva: o launcher e o painel a fecham (o mesmo clique abre e
/// fecha) e as Configurações não abrem outra janela. Devolve `false` quando este processo
/// deve sair sem mostrar nada.
pub fn claim_instance(mode: Mode) -> bool {
    if matches!(mode, Mode::Desktop | Mode::Greeter | Mode::Switcher) {
        return true;
    }
    let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") else {
        return true;
    };
    // Fundo: uma instância por monitor.
    let id = match (mode, fundo_from_args(std::env::args())) {
        (Mode::Fundo, Some((name, _))) => fundo_app_id(&name),
        _ => mode.app_id().to_string(),
    };
    let pidfile = Path::new(&dir).join(format!("{id}.pid"));
    let me = std::process::id();
    if let Some(pid) = fs::read_to_string(&pidfile)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        if pid != me && is_shell_process(pid) {
            if mode == Mode::Dock {
                // A dock nova (outro tamanho, mais um app aberto) só tira a
                // antiga depois de aparecer: sem piscar.
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_millis(900));
                    if is_shell_process(pid) {
                        let _ = Command::new("kill").arg(pid.to_string()).status();
                    }
                });
            } else if matches!(mode, Mode::Launcher | Mode::QuickSettings | Mode::Fundo | Mode::DockMenu) {
                let _ = Command::new("kill").arg(pid.to_string()).status();
                let _ = fs::remove_file(&pidfile);
            }
            // A dock, o fundo e o menu da dock novos substituem os antigos.
            if !matches!(mode, Mode::Dock | Mode::Fundo | Mode::DockMenu) {
                return false;
            }
        }
    }
    let _ = fs::write(&pidfile, me.to_string());
    true
}

/// Diz se o PID é um processo vivo do kalango-shell (e não um PID reciclado).
fn is_shell_process(pid: u32) -> bool {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|c| String::from_utf8_lossy(&c).contains("kalango"))
        .unwrap_or(false)
}

/// Desliga o computador (sessão real e greeter) ou só encerra o shell (protótipo).
pub fn power_off(real: bool) {
    if real {
        if let Err(err) = Command::new("systemctl").arg("poweroff").spawn() {
            eprintln!("kalango-shell: falha ao desligar: {err}");
        }
    } else {
        let _ = slint::quit_event_loop();
    }
}

/// Liga a UI à sessão: modo, abrir apps, desligar, sair e indicadores da dock.
///
/// Devolve o timer dos indicadores (só na área de trabalho da sessão real),
/// que precisa ficar vivo enquanto a janela roda.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    let native = is_native();
    ui.set_mode(mode.to_ui());
    let shell = ui.global::<Shell>();
    shell.set_native(native);
    shell.on_open_app(|id| open(&id));
    shell.on_open_file(|path| open_file(&path));
    let real = native || mode == Mode::Greeter;
    shell.on_power_off(move || power_off(real));

    if mode == Mode::Greeter {
        crate::greeter::install(ui);
        return None;
    }
    if native {
        // A sessão já foi autenticada pelo greetd: nada de tela de login falsa.
        ui.set_screen(Screen::Desktop);
        if let Ok(login) = std::env::var("USER") {
            let name = crate::greeter::display_name(&login);
            shell.set_user_initial(crate::greeter::initial(&name).into());
            shell.set_user_name(name.into());
        }
        if mode == Mode::Launcher {
            crate::recent::install(ui);
        }
    }
    shell.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fundo_pelos_argumentos() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(Mode::from_args(args(&["x", "--fundo=HDMI-A-1", "--numero=2"])), Mode::Fundo);
        assert_eq!(fundo_from_args(args(&["x", "--fundo=HDMI-A-1", "--numero=2"])), Some(("HDMI-A-1".into(), 2)));
        assert_eq!(fundo_app_id("HDMI-A-1"), "kalango-fundo-HDMI-A-1");
    }

    #[test]
    fn faixa_da_dock_pelos_argumentos() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(strip_from_args(args(&["x", "--dock", "--faixa=1366x54"])), Some((1366, 54)));
        assert_eq!(strip_from_args(args(&["x", "--dock"])), None);
        assert_eq!(Mode::from_args(args(&["x", "--dock"])), Mode::Dock);
    }

    #[test]
    fn pagina_pelos_argumentos() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(page_from_args(args(&["x", "--settings", "--pagina=3"])), Some(3));
        assert_eq!(page_from_args(args(&["x", "--settings"])), None);
        assert_eq!(page_from_args(args(&["x", "--pagina=abc"])), None);
    }

    #[test]
    fn modo_pelos_argumentos() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(Mode::from_args(args(&["kalango-shell"])), Mode::Desktop);
        assert_eq!(Mode::from_args(args(&["x", "--launcher"])), Mode::Launcher);
        assert_eq!(Mode::from_args(args(&["x", "--quick-settings"])), Mode::QuickSettings);
        assert_eq!(Mode::from_args(args(&["x", "--settings"])), Mode::Settings);
        assert_eq!(Mode::from_args(args(&["x", "--greeter"])), Mode::Greeter);
    }

    #[test]
    fn apps_do_catalogo_tem_comando() {
        assert_eq!(command_for("terminal").unwrap(), ["foot"]);
        let code = command_for("code").unwrap();
        assert!(code == ["geany"] || code == ["mousepad"]);
        assert_eq!(command_for("documents").unwrap()[0], "thunar");
        assert_eq!(command_for("network").unwrap().last().unwrap(), "nmtui");
        assert_eq!(command_for("store").unwrap(), ["gnome-software"]);
        assert_eq!(command_for("installer").unwrap(), ["kalango-instalar"]);
        assert!(command_for("inexistente").is_none());
        assert_eq!(command_for("lock").unwrap(), ["kalango-bloquear"]);
        assert_eq!(command_for("music").unwrap(), ["parole"]);
    }

    #[test]
    fn le_pasta_do_user_dirs() {
        let content = "# comentário\nXDG_DOCUMENTS_DIR=\"$HOME/Documentos\"\n";
        assert_eq!(
            parse_user_dir(content, "XDG_DOCUMENTS_DIR", "/home/user").as_deref(),
            Some("/home/user/Documentos")
        );
        assert_eq!(parse_user_dir(content, "XDG_MUSIC_DIR", "/home/user"), None);
    }

}
