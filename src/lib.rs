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
pub mod labwc;
pub mod recent;
pub mod session;
pub mod system;

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
    let shell = ui.global::<Shell>();

    // Protótipo: qualquer senha não vazia entra. No sistema real a
    // autenticação será via PAM, pelo greetd.
    shell.on_try_login(|password| !password.is_empty());

    apps::install(&ui, mode == Mode::Launcher && session::is_native());
    let mut timers = vec![clock::start(&ui), system::start(&ui)];
    timers.extend(session::install(&ui, mode));
    timers.extend(appearance::install(&ui, mode));
    timers.extend(device::install(&ui, mode));
    displays::install(&ui, mode);
    Ok((ui, timers))
}

/// Abre a janela do modo pedido nos argumentos e roda até ela ser fechada.
pub fn run() -> Result<(), slint::PlatformError> {
    let mode = Mode::from_args(std::env::args());
    // Super com o menu aberto: o novo processo fecha o antigo e sai.
    if !session::claim_instance(mode) {
        return Ok(());
    }
    let (ui, _timers) = build_mode(mode)?;
    ui.run()
}
