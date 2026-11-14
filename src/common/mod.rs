//! Utilidades usadas por todo o shell.
//!
//! - [`clock`]: data/hora local no idioma da interface;
//! - [`i18n`]: idioma (inglês ou português) e textos montados no Rust;
//! - [`system`]: uso de CPU e bateria (barra superior).

pub mod clock;
pub mod i18n;
pub mod system;
