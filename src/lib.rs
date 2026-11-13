//! Shell gráfico do Kalango OS (protótipo navegável em Rust + Slint).
//!
//! A UI declarativa fica em `ui/*.slint` (compilada pelo `build.rs`); este
//! crate liga a UI aos serviços do sistema:
//! - [`clock`]: data/hora local em pt-BR;
//! - [`device`]: rede, som, energia e ficha do sistema nas Configurações;
//! - [`displays`]: monitores (resolução, escala, rotação) pelo `wlr-randr`;
//! - [`system`]: uso de CPU e bateria;
//! - [`apps`]: busca do launcher e a lista "Todos os apps";
//! - [`desktop`]: apps instalados, lidos dos arquivos `.desktop`;
//! - [`session`]: sessão real (abrir apps, menu e Configurações como janelas
//!   próprias, desligar);
//! - [`appearance`]: aparência salva em disco e compartilhada entre processos;
//! - [`labwc`]: ajustes do compositor feitos pelo shell (margem da dock);
//! - [`greeter`]: tela de login de verdade, como greeter do greetd;
//! - [`wallpaper`]: papel de parede da área de trabalho;
//! - [`recent`]: arquivos recentes de verdade no menu de apps.
//!
//! A lógica fica na biblioteca (e não no `main.rs`) para poder ser reusada
//! por testes e por ferramentas como um gerador de capturas das telas.

pub mod appearance;
pub mod apps;
pub mod clock;
pub mod desktop;
pub mod device;
pub mod displays;
pub mod greeter;
pub mod i18n;
pub mod labwc;
pub mod recent;
pub mod region;
pub mod session;
pub mod sharing;
pub mod system;
pub mod wallpaper;

pub use session::Mode;

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
    slint::set_xdg_app_id(mode.app_id())?;
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
        native && !matches!(mode, Mode::Greeter | Mode::QuickSettings),
    );
    let mut timers = vec![clock::start(&ui), system::start(&ui)];
    timers.extend(session::install(&ui, mode));
    timers.extend(appearance::install(&ui, mode));
    timers.extend(device::install(&ui, mode));
    displays::install(&ui, mode);
    region::install(&ui, mode);
    sharing::install(&ui, mode);
    timers.extend(wallpaper::install(&ui, mode));
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
/// `KALANGO_RENDERER=gl` ou `=software` força um dos dois em tudo. Com
/// `$SLINT_BACKEND` definido, vale a escolha dele (`None`).
pub fn renderer_for(
    mode: Mode,
    slint_backend: Option<&str>,
    kalango_renderer: Option<&str>,
) -> Option<&'static str> {
    if slint_backend.is_some_and(|b| !b.is_empty()) {
        return None;
    }
    match kalango_renderer {
        Some("gl") | Some("femtovg") => Some("femtovg"),
        Some("software") => Some("software"),
        _ if matches!(mode, Mode::Launcher | Mode::QuickSettings) => Some("femtovg"),
        _ => Some("software"),
    }
}

/// Aplica a escolha de [`renderer_for`] (só no backend winit; o `kms` desenha
/// direto na tela e escolhe sozinho).
fn select_renderer(mode: Mode) {
    if cfg!(feature = "kms") {
        return;
    }
    let backend = std::env::var("SLINT_BACKEND").ok();
    let wanted = std::env::var("KALANGO_RENDERER").ok();
    let Some(renderer) = renderer_for(mode, backend.as_deref(), wanted.as_deref()) else {
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
    let mode = Mode::from_args(std::env::args());
    select_renderer(mode);
    let page = session::page_from_args(std::env::args());
    // Super com o menu aberto: o novo processo fecha o antigo e sai.
    // Configurações já abertas: só pede a página a elas.
    if !session::claim_instance(mode) {
        if let (Mode::Settings, Some(page)) = (mode, page) {
            session::request_page(page);
        }
        return Ok(());
    }
    let (ui, mut timers) = build_mode(mode)?;
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
        assert_eq!(renderer_for(Mode::Desktop, None, None), Some("software"));
        assert_eq!(renderer_for(Mode::Settings, Some(""), Some("outro")), Some("software"));
        assert_eq!(renderer_for(Mode::Desktop, None, Some("gl")), Some("femtovg"));
        assert_eq!(renderer_for(Mode::Desktop, Some("winit-femtovg"), None), None);
    }

    #[test]
    fn menu_transparente_usa_opengl() {
        assert_eq!(renderer_for(Mode::Launcher, None, None), Some("femtovg"));
        assert_eq!(renderer_for(Mode::QuickSettings, None, None), Some("femtovg"));
        assert_eq!(renderer_for(Mode::Launcher, None, Some("software")), Some("software"));
    }
}
