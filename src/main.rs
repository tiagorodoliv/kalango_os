//! Ponto de entrada da interface do sistema operacional.
//!
//! Responsabilidades deste módulo:
//! - instanciar a janela principal gerada a partir de `ui/app_window.slint`;
//! - ligar os callbacks da UI à lógica em Rust;
//! - iniciar o loop de eventos.
//!
//! A lógica de negócio (status do sistema, apps, configurações) deve ficar em
//! módulos próprios em `src/`, mantendo os arquivos `.slint` só com layout.

mod clock;

// Importa os tipos gerados pelo build.rs (AppWindow etc.).
slint::include_modules!();

/// Cria a janela principal, conecta os serviços e roda o loop de eventos.
///
/// Retorna erro se nenhum backend gráfico puder ser inicializado
/// (ex.: sem permissão em /dev/dri ao rodar com a feature `kms`).
fn main() -> Result<(), slint::PlatformError> {
    let ui = AppWindow::new()?;

    // O timer precisa viver enquanto a UI roda; por isso fica preso a esta variável.
    let _clock = clock::start(&ui);

    ui.run()
}
