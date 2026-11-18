//! Menu de apps (`--launcher`) e de onde vêm os apps.
//!
//! - [`apps`]: catálogo fixo, busca e a lista "Todos os apps";
//! - [`desktop`]: apps instalados, lidos dos arquivos `.desktop`, e ícones do tema;
//! - [`recent`]: arquivos recentes de verdade no menu.

pub mod apps;
pub mod desktop;
pub mod icon_style;
pub mod recent;
