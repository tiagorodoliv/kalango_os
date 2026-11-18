//! O que fica por trás das páginas das Configurações (`--settings`).
//!
//! - [`appearance`]: aparência salva em disco e compartilhada entre processos;
//! - [`device`]: rede, Wi-Fi, som, brilho e ficha do sistema;
//! - [`netconfig`]: IP fixo das conexões e VPN (OpenVPN, WireGuard);
//! - [`bluetooth`]: ligar, procurar, parear e conectar aparelhos (BlueZ);
//! - [`displays`]: monitores (resolução, escala, rotação, arranjo) pelo `wlr-randr`;
//! - [`power`]: energia (modo de desempenho, apagar a tela, suspender, tampa);
//! - [`users`]: contas de usuário (senha, criar, tipo, remover);
//! - [`printers`]: impressoras de rede e USB (CUPS);
//! - [`updates`]: atualizações do sistema e dos apps, sem terminal;
//! - [`sharing`]: pastas na rede local (Samba) e computadores da rede;
//! - [`region`]: idioma e teclado;
//! - [`terminal`]: temas do terminal (foot);
//! - [`window_buttons`]: botões das janelas (fechar, maximizar) por estilo.

pub mod appearance;
pub mod bluetooth;
pub mod device;
pub mod displays;
pub mod firewall;
pub mod netconfig;
pub mod power;
pub mod printers;
pub mod region;
pub mod sharing;
pub mod shortcuts;
pub mod terminal;
pub mod updates;
pub mod users;
pub mod window_buttons;
