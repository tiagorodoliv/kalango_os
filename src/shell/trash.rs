//! Lixeira, na dock e na área de trabalho (como no Ubuntu).
//!
//! A lixeira é a do padrão freedesktop (`~/.local/share/Trash`), a mesma do
//! Thunar e do `gio trash`. Aqui: saber se tem algo dentro (o ícone muda de
//! vazia para cheia), os ícones do tema de ícones e o "Esvaziar lixeira", que
//! pergunta antes (o zenity) e usa o `gio trash --empty`. Abrir é o Thunar em
//! `trash:///` (`session::command_for("trash")`).

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use slint::{ComponentHandle, Image, Timer, TimerMode};

use crate::common::i18n::tr;
use crate::launcher::desktop;
use crate::{AppWindow, Mode, Shell};

/// Pasta dos arquivos na lixeira (`$XDG_DATA_HOME/Trash/files`).
fn files_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))?;
    Some(base.join("Trash/files"))
}

/// A lixeira tem algo dentro.
pub fn is_full() -> bool {
    files_dir().and_then(|d| std::fs::read_dir(d).ok()).is_some_and(|mut it| it.next().is_some())
}

/// Ícone do tema de ícones atual (`user-trash` ou `user-trash-full`).
fn themed(name: &str) -> Image {
    desktop::find_themed_icon(name, &desktop::current_icon_theme(), &desktop::data_dirs())
        .and_then(|p| crate::launcher::apps::load_icon(&p))
        .unwrap_or_default()
}

/// Pergunta e esvazia a lixeira, num processo à parte (quem chamou pode
/// fechar: o menu da dock sai logo depois).
pub fn empty() {
    let question = tr(
        "Empty the trash? The files in it will be deleted for good.",
        "Esvaziar a lixeira? Os arquivos dela serão apagados de vez.",
    );
    let script = format!(
        "zenity --question --icon=user-trash-full --title='{}' --text='{}' --ok-label='{}' --cancel-label='{}' && gio trash --empty",
        tr("Trash", "Lixeira"),
        question,
        tr("Empty", "Esvaziar"),
        tr("Cancel", "Cancelar"),
    );
    let _ = Command::new("sh")
        .args(["-c", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

/// Mantém `Shell.trash-full` e os ícones da lixeira em dia (área de trabalho
/// e dock em janela própria). O timer devolvido precisa ficar vivo.
pub fn install(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if !matches!(mode, Mode::Desktop | Mode::Dock) || !crate::shell::session::is_native() {
        return None;
    }
    let weak = ui.as_weak();
    let mut theme = String::new();
    let mut check = move || {
        let Some(ui) = weak.upgrade() else { return };
        let shell = ui.global::<Shell>();
        let full = is_full();
        if shell.get_trash_full() != full {
            shell.set_trash_full(full);
        }
        // Tema de ícones trocado na Aparência: ícones novos.
        let now = desktop::current_icon_theme();
        if now != theme {
            shell.set_trash_icon(themed("user-trash"));
            shell.set_trash_icon_full(themed("user-trash-full"));
            theme = now;
        }
    };
    check();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(2), check);
    Some(timer)
}
