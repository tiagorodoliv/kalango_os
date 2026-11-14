//! A "casca" da sessão: os processos do shell e o que fica sempre na tela.
//!
//! - [`session`]: modos do shell (um processo cada), abrir apps, desligar;
//! - [`greeter`]: tela de login de verdade, como greeter do greetd;
//! - [`dock`]: a dock em janela própria, sempre por cima das janelas;
//! - `taskbar` (Linux): apps abertos na dock e o menu do botão direito dela;
//! - `switcher` (Linux): Alt+Tab com os ícones das janelas;
//! - `toplevels` (Linux): janelas abertas (wlr-foreign-toplevel);
//! - `workspaces` (Linux): áreas de trabalho na barra (ext-workspace do labwc);
//! - [`outputs`]: vários monitores (barra no principal, papel de parede em todos);
//! - [`wallpaper`]: papel de parede da área de trabalho;
//! - [`labwc`]: ajustes do compositor feitos pelo shell (margens, dock, menus);
//! - [`usb`]: grava logo nos pendrives e avisa na barra enquanto grava.

pub mod dock;
pub mod greeter;
pub mod labwc;
pub mod outputs;
pub mod session;
#[cfg(target_os = "linux")]
pub mod switcher;
#[cfg(target_os = "linux")]
pub mod taskbar;
#[cfg(target_os = "linux")]
pub mod toplevels;
pub mod usb;
pub mod wallpaper;
#[cfg(target_os = "linux")]
pub mod workspaces;
