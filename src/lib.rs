//! Shell gráfico do Kalango OS (protótipo navegável em Rust + Slint).
//!
//! A UI declarativa fica em `ui/*.slint` (compilada pelo `build.rs`); este
//! crate liga a UI aos serviços do sistema:
//! - [`clock`]: data/hora local em pt-BR;
//! - [`system`]: uso de CPU e bateria;
//! - [`apps`]: busca do launcher.
//!
//! A lógica fica na biblioteca (e não no `main.rs`) para poder ser reusada
//! por testes e por ferramentas como um gerador de capturas das telas.

pub mod apps;
pub mod clock;
pub mod system;

// Tipos gerados pelo build.rs (AppWindow, Shell, Theme, Apps...).
slint::include_modules!();

/// Cria a janela principal já ligada aos serviços.
///
/// Os timers devolvidos precisam ficar vivos enquanto a janela roda.
pub fn build() -> Result<(AppWindow, Vec<slint::Timer>), slint::PlatformError> {
    let ui = AppWindow::new()?;
    let shell = ui.global::<Shell>();

    // Protótipo: qualquer senha não vazia entra. No sistema real a
    // autenticação será via PAM, pelo greetd.
    shell.on_try_login(|password| !password.is_empty());

    // Protótipo: desligar = encerrar o programa. No sistema real: logind.
    shell.on_power_off(|| {
        let _ = slint::quit_event_loop();
    });

    apps::install(&ui);
    let timers = vec![clock::start(&ui), system::start(&ui)];
    Ok((ui, timers))
}

/// Abre a janela e roda o loop de eventos até ela ser fechada.
pub fn run() -> Result<(), slint::PlatformError> {
    let (ui, _timers) = build()?;
    ui.run()
}
