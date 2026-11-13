//! Script de build: compila a interface declarada em `ui/` para código Rust.
//!
//! O Slint gera os tipos (ex.: `AppWindow`) em tempo de compilação, então
//! erros de layout aparecem no `cargo build` e não em tempo de execução.
//! As traduções (`lang/<idioma>/LC_MESSAGES/kalango_os.po`) vão embutidas no
//! binário; os textos originais dos `.slint` (`@tr("...")`) são em inglês.

fn main() {
    let config = slint_build::CompilerConfiguration::new()
        .with_bundled_translations("lang")
        .with_default_translation_context(slint_build::DefaultTranslationContext::None);
    println!("cargo:rerun-if-changed=lang");
    slint_build::compile_with_config("ui/app_window.slint", config).expect("falha ao compilar a UI .slint");
}
