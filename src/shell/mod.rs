//! A "casca" da sessão: os processos do shell e o que fica sempre na tela.
//!
//! - [`session`]: modos do shell (um processo cada), abrir apps, desligar;
//! - [`calendar`]: calendário que abre ao clicar no relógio da barra;
//! - `capture` (Linux): captura e gravação da tela (Print);
//! - [`greeter`]: tela de login de verdade, como greeter do greetd;
//! - [`dock`]: a dock em janela própria, sempre por cima das janelas;
//! - `taskbar` (Linux): apps abertos na dock e o menu do botão direito dela;
//! - `switcher` (Linux): Alt+Tab com os ícones das janelas;
//! - `toplevels` (Linux): janelas abertas (wlr-foreign-toplevel);
//! - `workspaces` (Linux): áreas de trabalho na barra (ext-workspace do labwc);
//! - [`outputs`]: vários monitores (barra no principal, papel de parede em todos);
//! - [`wallpaper`]: papel de parede da área de trabalho;
//! - [`labwc`]: ajustes do compositor feitos pelo shell (margens, dock, menus);
//! - [`usb`]: grava logo nos pendrives e avisa na barra enquanto grava;
//! - [`thunar`]: "Definir como papel de parede" no botão direito do Thunar;
//! - [`update`]: recarrega a interface depois de uma atualização, sem reiniciar;
//! - [`integrity`]: avisa (e pede o administrador) quando os arquivos da interface
//!   mudaram fora de uma atualização.

pub mod calendar;
#[cfg(target_os = "linux")]
pub mod capture;
pub mod dock;
pub mod firewall;
pub mod osd;
pub mod greeter;
pub mod integrity;
pub mod labwc;
pub mod outputs;
pub mod session;
#[cfg(target_os = "linux")]
pub mod switcher;
#[cfg(target_os = "linux")]
pub mod taskbar;
pub mod thunar;
#[cfg(target_os = "linux")]
pub mod toplevels;
pub mod update;
pub mod usb;
pub mod wallpaper;
#[cfg(target_os = "linux")]
pub mod workspaces;
