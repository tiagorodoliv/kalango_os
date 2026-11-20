//! Utilidades usadas por todo o shell.
//!
//! - [`clock`]: data/hora local no idioma da interface;
//! - [`i18n`]: idioma (inglês ou português) e textos montados no Rust;
//! - [`json`]: leitor de JSON pequeno (saída do `pw-dump`);
//! - [`system`]: uso de CPU e bateria (barra superior).

pub mod clock;
pub mod i18n;
pub mod json;
pub mod system;
