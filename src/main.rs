//! Ponto de entrada do shell do Kalango OS.
//!
//! Toda a lógica mora na biblioteca (`src/lib.rs`); aqui só abrimos a janela.
//! Retorna erro se nenhum backend gráfico puder ser inicializado
//! (ex.: sem permissão em /dev/dri ao rodar com a feature `kms`).

fn main() -> Result<(), slint::PlatformError> {
    linux_os::run()
}
