//! Shell gráfico do Kalango OS (protótipo navegável em Rust + Slint).
//!
//! A UI declarativa fica em `ui/*.slint` (compilada pelo `build.rs`); este
//! crate liga a UI aos serviços do sistema, em quatro pastas:
//! - [`common`]: relógio, idioma e uso de CPU/bateria;
//! - [`launcher`]: menu de apps, apps instalados e arquivos recentes;
//! - [`shell`]: a sessão (processos do shell, login, dock, barra, Alt+Tab,
//!   monitores, papel de parede e ajustes do labwc);
//! - [`settings`]: o que fica por trás das páginas das Configurações.
//!
//! A lógica fica na biblioteca (e não no `main.rs`) para poder ser reusada
//! por testes e por ferramentas como um gerador de capturas das telas.

pub mod common;
pub mod launcher;
pub mod settings;
pub mod shell;

use common::{clock, i18n, system};
use launcher::apps;
use settings::{appearance, bluetooth, device, displays, netconfig, power, printers, region, sharing, terminal, users};
#[cfg(target_os = "linux")]
use shell::{switcher, taskbar, workspaces};
use shell::{dock, outputs, session, usb, wallpaper};

pub use shell::session::Mode;

// Tipos gerados pelo build.rs (AppWindow, Shell, Theme, Apps...).
slint::include_modules!();

/// Identificador da janela do shell no Wayland (`app_id` do xdg-shell).
///
/// Deve bater com o `identifier` das regras em
/// `distro/config/includes.chroot/etc/xdg/labwc/rc.xml`.
pub const APP_ID: &str = "kalango-shell";

/// Cria a janela da área de trabalho já ligada aos serviços.
///
/// Os timers devolvidos precisam ficar vivos enquanto a janela roda.
pub fn build() -> Result<(AppWindow, Vec<slint::Timer>), slint::PlatformError> {
    build_mode(Mode::Desktop)
}

/// Cria a janela de um modo do shell (área de trabalho, menu ou Configurações).
///
/// Os timers devolvidos precisam ficar vivos enquanto a janela roda.
pub fn build_mode(mode: Mode) -> Result<(AppWindow, Vec<slint::Timer>), slint::PlatformError> {
    // app_id Wayland: o labwc usa esse nome nas regras de janela da ISO.
    // Precisa vir depois de criar a janela (que inicializa a plataforma do
    // Slint) e antes de mostrá-la (quando a janela nativa é criada).
    let ui = AppWindow::new()?;
    // Janela de fundo: um app_id por monitor (as regras do labwc são por saída).
    let fundo = session::fundo_from_args(std::env::args()).filter(|_| mode == Mode::Fundo);
    match &fundo {
        Some((name, _)) => slint::set_xdg_app_id(session::fundo_app_id(name))?,
        None => slint::set_xdg_app_id(mode.app_id())?,
    }
    // Idioma da interface (inglês ou português, pelo LANG do ambiente).
    i18n::apply();
    let shell = ui.global::<Shell>();

    // Protótipo: qualquer senha não vazia entra. No sistema real a
    // autenticação será via PAM, pelo greetd.
    shell.on_try_login(|password| !password.is_empty());

    let native = session::is_native();
    apps::install(
        &ui,
        mode == Mode::Launcher && native,
        native && !matches!(mode, Mode::Greeter | Mode::QuickSettings | Mode::Fundo),
    );
    let mut timers = vec![clock::start(&ui), system::start(&ui)];
    timers.extend(session::install(&ui, mode));
    timers.extend(appearance::install(&ui, mode));
    timers.extend(device::install(&ui, mode));
    timers.extend(bluetooth::install(&ui, mode));
    timers.extend(netconfig::install(&ui, mode));
    displays::install(&ui, mode);
    region::install(&ui, mode);
    sharing::install(&ui, mode);
    terminal::install(&ui, mode);
    usb::install(&ui, mode);
    #[cfg(target_os = "linux")]
    workspaces::install(&ui, mode);
    timers.extend(wallpaper::install(&ui, mode));
    // Monitores antes da dock: ela se posiciona no monitor principal.
    timers.extend(outputs::install(&ui, mode));
    if let Some((name, number)) = &fundo {
        timers.extend(outputs::install_background(&ui, name, *number));
    }
    timers.extend(dock::install(&ui, mode));
    #[cfg(target_os = "linux")]
    {
        timers.extend(taskbar::install(&ui, mode));
        // O Alt+Tab fica aberto a sessão toda (guarda a ordem das janelas).
        if mode == Mode::Desktop && native {
            switcher::spawn_background();
        }
    }
    timers.extend(power::install(&ui, mode));
    if mode == Mode::Desktop && native {
        shell::thunar::ensure_wallpaper_action();
    }
    users::install(&ui, mode);
    shell::calendar::install(&ui, mode);
    settings::updates::install_page(&ui, mode);
    timers.extend(printers::install(&ui, mode));
    Ok((ui, timers))
}

/// Renderer do Slint a usar, a partir de `$SLINT_BACKEND` e `$KALANGO_RENDERER`.
///
/// O padrão é o de software: o shell é quase todo parado, e ele redesenha só
/// o que mudou, sem carregar o Mesa. Medido numa VM de 2 GB, a área de
/// trabalho foi de 192 MB (OpenGL pelo llvmpipe, que traz o LLVM inteiro) para
/// 31 MB, e de 1,4 s para 0,08 s de CPU a cada 30 s parada.
/// O menu de apps e o painel rápido são a exceção: são janelas transparentes
/// por cima dos apps, e o renderer de software no Wayland não tem canal alfa
/// (o fundo ficaria preto); como ficam abertos pouco tempo, usam o OpenGL.
/// Com placa de vídeo de verdade ([`dock::hardware_gl`]: Intel, AMD,
/// NVIDIA), tudo usa o OpenGL, que nela é barato e rola sem engasgar.
/// `KALANGO_RENDERER=gl` ou `=software` força um dos dois em tudo. Com
/// `$SLINT_BACKEND` definido, vale a escolha dele (`None`).
pub fn renderer_for(
    mode: Mode,
    slint_backend: Option<&str>,
    kalango_renderer: Option<&str>,
    hardware_gl: bool,
) -> Option<&'static str> {
    if slint_backend.is_some_and(|b| !b.is_empty()) {
        return None;
    }
    match kalango_renderer {
        Some("gl") | Some("femtovg") => Some("femtovg"),
        Some("software") => Some("software"),
        // Com placa de vídeo de verdade o OpenGL é barato e rola liso (a
        // página Aparência, alta, travava no renderer de software).
        _ if hardware_gl => Some("femtovg"),
        // A dock em janela própria só abre com placa de vídeo (src/shell/dock.rs).
        _ if matches!(mode, Mode::Launcher | Mode::QuickSettings | Mode::Calendar | Mode::Dock | Mode::Switcher | Mode::DockMenu) => {
            Some("femtovg")
        }
        _ => Some("software"),
    }
}

/// Aplica a escolha de [`renderer_for`] (só no backend winit; o `kms` desenha
/// direto na tela e escolhe sozinho).
pub(crate) fn select_renderer(mode: Mode) {
    if cfg!(feature = "kms") {
        return;
    }
    let backend = std::env::var("SLINT_BACKEND").ok();
    let wanted = std::env::var("KALANGO_RENDERER").ok();
    let Some(renderer) = renderer_for(mode, backend.as_deref(), wanted.as_deref(), dock::hardware_gl()) else {
        return;
    };
    if let Err(err) = slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.into())
        .select()
    {
        eprintln!("kalango-shell: renderer {renderer} indisponível ({err}); usando o padrão do Slint");
    }
}

/// Abre a janela do modo pedido nos argumentos e roda até ela ser fechada.
pub fn run() -> Result<(), slint::PlatformError> {
    // "Definir como papel de parede" (Abrir com, no Thunar): sem janela.
    if let Some(file) = wallpaper::file_from_args(std::env::args()) {
        if let Err(err) = wallpaper::run_cli(&file) {
            eprintln!("kalango-shell: papel de parede {file}: {err}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let mode = Mode::from_args(std::env::args());
    // Alt+Tab: o atalho só manda o pedido ao processo aberto (rápido, sem
    // abrir janela); ver src/shell/switcher.rs.
    #[cfg(target_os = "linux")]
    if mode == Mode::Switcher {
        return switcher::run();
    }
    select_renderer(mode);
    let page = session::page_from_args(std::env::args());
    // Super com o menu aberto: o novo processo fecha o antigo e sai.
    // Configurações já abertas: pede a página a elas e as traz para a frente.
    if !session::claim_instance(mode) {
        if let (Mode::Settings, Some(page)) = (mode, page) {
            session::request_page(page);
        }
        if mode == Mode::Settings {
            session::raise(mode);
        }
        return Ok(());
    }
    let (ui, mut timers) = build_mode(mode)?;
    if let (Mode::Dock, Some((w, h))) = (mode, session::strip_from_args(std::env::args())) {
        let shell = ui.global::<Shell>();
        shell.set_dock_strip_width(w as f32);
        shell.set_dock_strip_height(h as f32);
        // Diagnóstico (KALANGO_DEBUG_DOCK=1): tamanho que a janela recebeu.
        if std::env::var_os("KALANGO_DEBUG_DOCK").is_some() {
            let weak = ui.as_weak();
            let t = slint::Timer::default();
            t.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(1), move || {
                if let Some(ui) = weak.upgrade() {
                    let s = ui.window().size();
                    eprintln!("dock: janela {}x{} escala {}", s.width, s.height, ui.window().scale_factor());
                }
            });
            timers.push(t);
        }
    }
    // Menu de apps e painel rápido: já no tamanho da tela (ver
    // `outputs::primary_size`).
    if matches!(mode, Mode::Launcher | Mode::QuickSettings | Mode::Calendar | Mode::DockMenu) && session::is_native() {
        if let Some((w, h)) = outputs::primary_size() {
            ui.window().set_size(slint::LogicalSize::new(w as f32, h as f32));
        }
    }
    if mode == Mode::Settings {
        if let Some(page) = page {
            ui.set_settings_page(page);
        }
        timers.extend(session::watch_page_requests(&ui));
    }
    ui.run()
}

#[cfg(test)]
mod tests {
    use super::{renderer_for, Mode};

    #[test]
    fn renderer_de_software_por_padrao() {
        assert_eq!(renderer_for(Mode::Desktop, None, None, false), Some("software"));
        assert_eq!(renderer_for(Mode::Settings, Some(""), Some("outro"), false), Some("software"));
        assert_eq!(renderer_for(Mode::Desktop, None, Some("gl"), false), Some("femtovg"));
        assert_eq!(renderer_for(Mode::Desktop, Some("winit-femtovg"), None, false), None);
    }

    #[test]
    fn com_placa_de_video_tudo_usa_opengl() {
        assert_eq!(renderer_for(Mode::Settings, None, None, true), Some("femtovg"));
        assert_eq!(renderer_for(Mode::Desktop, None, Some("software"), true), Some("software"));
    }

    #[test]
    fn menu_transparente_usa_opengl() {
        assert_eq!(renderer_for(Mode::Launcher, None, None, false), Some("femtovg"));
        assert_eq!(renderer_for(Mode::QuickSettings, None, None, false), Some("femtovg"));
        assert_eq!(renderer_for(Mode::Launcher, None, Some("software"), false), Some("software"));
    }
}
