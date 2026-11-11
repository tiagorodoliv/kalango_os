//! Script de build: compila a interface declarada em `ui/` para código Rust.
//!
//! O Slint gera os tipos (ex.: `AppWindow`) em tempo de compilação, então
//! erros de layout aparecem no `cargo build` e não em tempo de execução.

fn main() {
    slint_build::compile("ui/app_window.slint").expect("falha ao compilar a UI .slint");
}
