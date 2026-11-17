//! Captura de tela e gravação da tela, como no Ubuntu.
//!
//! - **Print** → `kalango-shell --captura`: a ferramenta (`ui/screens/capture.slint`),
//!   em tela cheia e transparente por cima de tudo (regra `kalango-captura` no
//!   rc.xml). Área (arrastar) ou a tela inteira, foto ou vídeo.
//! - **Shift+Print** → `--captura=tela`: foto da tela inteira na hora, sem janela.
//! - **Ctrl+Shift+Alt+R** → `--captura=video`: liga/desliga a gravação da tela inteira.
//!
//! Foto pelo `grim` (PNG em Imagens/Capturas de tela, copiada para a área de
//! transferência pelo `wl-copy`); vídeo pelo `wf-recorder` (MP4 em
//! Vídeos/Gravações de tela). A gravação fica num processo à parte, anotado em
//! `$XDG_RUNTIME_DIR/kalango-gravacao`; com ela rodando, qualquer um dos atalhos
//! a termina (SIGINT: o `wf-recorder` fecha o arquivo direito).
//!
//! A área de trabalho mostra o aviso "Gravando a tela" no centro da barra e os
//! avisos dos outros processos (foto salva, vídeo salvo), que chegam pelo arquivo
//! `$XDG_RUNTIME_DIR/kalango-aviso` (o processo da captura já terminou quando o
//! aviso aparece).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use slint::{ComponentHandle, Timer, TimerMode};

use crate::common::i18n::tr;
use crate::shell::session::{find_in_path, xdg_dir};
use crate::{AppWindow, Capture, Mode, Shell};

/// O que o argumento `--captura[=...]` pede.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Abre a ferramenta (`--captura`).
    Tool,
    /// Foto da tela inteira na hora (`--captura=tela`).
    Screen,
    /// Liga/desliga a gravação da tela inteira (`--captura=video`).
    Record,
}

/// Lê a ação dos argumentos (`None` = não é a captura).
pub fn action_from_args<I: IntoIterator<Item = String>>(args: I) -> Option<Action> {
    args.into_iter().find_map(|a| match a.as_str() {
        "--captura" => Some(Action::Tool),
        "--captura=tela" => Some(Action::Screen),
        "--captura=video" => Some(Action::Record),
        _ => None,
    })
}

/// Onde a captura acontece: o monitor principal (nome e origem no arranjo).
struct Output {
    name: String,
    x: i32,
    y: i32,
}

fn primary_output() -> Option<Output> {
    let out = Command::new("wlr-randr").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let outputs = crate::settings::displays::parse(&String::from_utf8_lossy(&out.stdout));
    let config = crate::settings::displays::config_path()
        .and_then(|p| fs::read_to_string(p).ok())
        .unwrap_or_default();
    let o = crate::settings::displays::primary(&outputs, &config)?;
    Some(Output { name: o.name.clone(), x: o.x, y: o.y })
}

/// Região no formato do `grim -g` e do `wf-recorder -g`: "x,y LxA" em px
/// lógicos do arranjo dos monitores (a área é relativa ao monitor principal).
pub fn geometry(origin: (i32, i32), x: f32, y: f32, w: f32, h: f32) -> String {
    format!(
        "{},{} {}x{}",
        origin.0 + x.round() as i32,
        origin.1 + y.round() as i32,
        w.round().max(1.0) as i32,
        h.round().max(1.0) as i32
    )
}

/// O que capturar: um monitor inteiro ou uma região.
enum Target {
    Screen(String),
    Region(String),
}

impl Target {
    fn args(&self) -> [&str; 2] {
        match self {
            Target::Screen(name) => ["-o", name],
            Target::Region(geom) => ["-g", geom],
        }
    }
}

/// Nome do arquivo com data e hora: "Captura de tela 2026-11-17 20-15-03.png".
pub fn file_name(prefix: &str, when: chrono::NaiveDateTime, ext: &str) -> String {
    format!("{prefix} {}.{ext}", when.format("%Y-%m-%d %H-%M-%S"))
}

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/tmp".into())
}

/// Pasta das fotos (criada se faltar): Imagens/Capturas de tela.
fn screenshots_dir() -> PathBuf {
    let home = home();
    let dir = Path::new(&xdg_dir(&home, "XDG_PICTURES_DIR", tr("Pictures", "Imagens")))
        .join(tr("Screenshots", "Capturas de tela"));
    let _ = fs::create_dir_all(&dir);
    dir
}

/// Pasta dos vídeos (criada se faltar): Vídeos/Gravações de tela.
fn screencasts_dir() -> PathBuf {
    let home = home();
    let dir = Path::new(&xdg_dir(&home, "XDG_VIDEOS_DIR", tr("Videos", "Vídeos")))
        .join(tr("Screencasts", "Gravações de tela"));
    let _ = fs::create_dir_all(&dir);
    dir
}

fn runtime_file(name: &str) -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| Path::new(&d).join(name))
}

/// Arquivo da gravação em andamento: "PID\ncaminho do vídeo".
fn recording_file() -> Option<PathBuf> {
    runtime_file("kalango-gravacao")
}

/// Arquivo de aviso para a barra da área de trabalho.
fn notice_file() -> Option<PathBuf> {
    runtime_file("kalango-aviso")
}

/// Manda um aviso para o centro da barra (a área de trabalho o lê em até 1 s).
pub fn notify(text: &str) {
    if let Some(path) = notice_file() {
        let _ = fs::write(path, text);
    }
}

/// Gravação em andamento: o PID do `wf-recorder`, se ele ainda está vivo.
fn recording_pid() -> Option<u32> {
    let text = fs::read_to_string(recording_file()?).ok()?;
    let pid: u32 = text.lines().next()?.trim().parse().ok()?;
    let comm = fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    (comm.trim() == "wf-recorder").then_some(pid)
}

/// Termina a gravação (SIGINT, para o MP4 fechar direito) e avisa onde ficou.
fn stop_recording(pid: u32) {
    let _ = Command::new("kill").args(["-INT", &pid.to_string()]).status();
    // O wf-recorder leva um instante para gravar o fim do arquivo.
    for _ in 0..50 {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    if let Some(f) = recording_file() {
        let _ = fs::remove_file(f);
    }
    crate::settings::device::play_sound("complete", None);
    notify(tr(
        "Screen recording saved in Videos/Screencasts",
        "Gravação salva em Vídeos/Gravações de tela",
    ));
}

/// Começa a gravar (`wf-recorder` num processo próprio, que continua depois
/// de este sair).
fn start_recording(target: &Target) -> bool {
    use std::os::unix::process::CommandExt;
    if find_in_path("wf-recorder").is_none() {
        notify(tr("Screen recording needs the wf-recorder package", "A gravação precisa do pacote wf-recorder"));
        return false;
    }
    let file = screencasts_dir().join(file_name(
        tr("Screencast", "Gravação de tela"),
        chrono::Local::now().naive_local(),
        "mp4",
    ));
    let mut cmd = Command::new("wf-recorder");
    cmd.args(target.args())
        .arg("-f")
        .arg(&file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Grupo próprio: não morre junto com este processo.
        .process_group(0);
    match cmd.spawn() {
        Ok(child) => {
            if let Some(f) = recording_file() {
                let _ = fs::write(f, format!("{}\n{}\n", child.id(), file.display()));
            }
            true
        }
        Err(err) => {
            eprintln!("kalango-shell: falha ao abrir o wf-recorder: {err}");
            false
        }
    }
}

/// Tira a foto, copia para a área de transferência e avisa.
fn take_screenshot(target: &Target) -> bool {
    if find_in_path("grim").is_none() {
        notify(tr("Screenshots need the grim package", "A captura precisa do pacote grim"));
        return false;
    }
    let file = screenshots_dir().join(file_name(
        tr("Screenshot", "Captura de tela"),
        chrono::Local::now().naive_local(),
        "png",
    ));
    let ok = Command::new("grim")
        .args(target.args())
        .arg(&file)
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !ok {
        notify(tr("Could not take the screenshot", "Não foi possível capturar a tela"));
        return false;
    }
    crate::settings::device::play_sound("camera-shutter", None);
    // Área de transferência: o wl-copy fica em segundo plano servindo a imagem.
    let copied = find_in_path("wl-copy").is_some()
        && fs::File::open(&file).is_ok_and(|f| {
            Command::new("wl-copy")
                .args(["--type", "image/png"])
                .stdin(f)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        });
    notify(if copied {
        tr(
            "Screenshot saved in Pictures/Screenshots and copied",
            "Captura salva em Imagens/Capturas de tela e copiada",
        )
    } else {
        tr("Screenshot saved in Pictures/Screenshots", "Captura salva em Imagens/Capturas de tela")
    });
    true
}

/// Antes de abrir janela: com uma gravação em andamento, qualquer atalho da
/// captura a termina; `--captura=tela` e `--captura=video` agem sem janela.
/// Devolve `true` se já fez tudo (o processo sai sem abrir a ferramenta).
pub fn run_without_window(action: Action) -> bool {
    if let Some(pid) = recording_pid() {
        stop_recording(pid);
        return true;
    }
    match action {
        Action::Tool => false,
        Action::Screen | Action::Record => {
            let Some(out) = primary_output() else {
                notify(tr("Could not find the screen", "Não foi possível achar a tela"));
                return true;
            };
            let target = Target::Screen(out.name);
            if action == Action::Screen {
                take_screenshot(&target);
            } else {
                start_recording(&target);
            }
            true
        }
    }
}

/// Liga o botão de capturar da ferramenta: some com a janela, espera o
/// compositor tirá-la da tela e captura (a área é relativa ao monitor
/// principal, onde a ferramenta abre).
pub fn install(ui: &AppWindow) {
    let weak = ui.as_weak();
    ui.global::<Capture>().on_capture(move || {
        let Some(ui) = weak.upgrade() else { return };
        let c = ui.global::<Capture>();
        let (area, video) = (c.get_area(), c.get_video());
        let (x, y, w, h) = (c.get_sel_x(), c.get_sel_y(), c.get_sel_width(), c.get_sel_height());
        let _ = ui.hide();
        Timer::single_shot(Duration::from_millis(300), move || {
            std::thread::spawn(move || {
                if let Some(out) = primary_output() {
                    let target = if area {
                        Target::Region(geometry((out.x, out.y), x, y, w, h))
                    } else {
                        Target::Screen(out.name)
                    };
                    if video {
                        start_recording(&target);
                    } else {
                        take_screenshot(&target);
                    }
                }
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
            });
        });
    });
}

/// Área de trabalho: aviso de gravação na barra e avisos dos outros processos.
pub fn install_desktop(ui: &AppWindow, mode: Mode) -> Option<Timer> {
    if mode != Mode::Desktop || !crate::shell::session::is_native() {
        return None;
    }
    let weak = ui.as_weak();
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = weak.upgrade() else { return };
        let recording = recording_pid().is_some();
        let shell = ui.global::<Shell>();
        if shell.get_recording() != recording {
            shell.set_recording(recording);
        }
        if let Some(path) = notice_file() {
            if let Ok(text) = fs::read_to_string(&path) {
                let _ = fs::remove_file(&path);
                let text = text.trim();
                if !text.is_empty() {
                    ui.invoke_show_toast(text.into());
                }
            }
        }
    });
    Some(timer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_a_acao_dos_argumentos() {
        let a = |v: &[&str]| action_from_args(v.iter().map(|s| s.to_string()));
        assert_eq!(a(&["x", "--captura"]), Some(Action::Tool));
        assert_eq!(a(&["x", "--captura=tela"]), Some(Action::Screen));
        assert_eq!(a(&["x", "--captura=video"]), Some(Action::Record));
        assert_eq!(a(&["x", "--launcher"]), None);
    }

    #[test]
    fn regiao_no_arranjo_dos_monitores() {
        assert_eq!(geometry((0, 0), 10.4, 20.6, 300.0, 200.0), "10,21 300x200");
        // Monitor principal à direita de outro de 1920 px.
        assert_eq!(geometry((1920, 0), 0.0, 0.0, 0.2, 50.0), "1920,0 1x50");
    }

    #[test]
    fn nome_do_arquivo_com_data() {
        let when = chrono::NaiveDate::from_ymd_opt(2026, 11, 17).unwrap().and_hms_opt(20, 5, 3).unwrap();
        assert_eq!(file_name("Captura de tela", when, "png"), "Captura de tela 2026-11-17 20-05-03.png");
    }
}
