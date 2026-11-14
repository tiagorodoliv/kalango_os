//! Alt+Tab com os ícones das janelas abertas, como no Ubuntu.
//!
//! O labwc roda `kalango-shell --alternar` (ou `--alternar=anterior`, com
//! Shift) a cada Alt+Tab. Um processo do Alt+Tab fica aberto a sessão toda
//! (a área de trabalho o inicia com `--alternar=iniciar`), escondido, e
//! guarda a ordem de uso das janelas ([`crate::toplevels`]); o comando do
//! atalho só manda o pedido para ele pelo socket
//! `$XDG_RUNTIME_DIR/kalango-alternar.sock` e sai. Sem o processo, o comando
//! vira ele.
//!
//! O primeiro Alt+Tab mostra a janela (tela cheia, transparente, por cima)
//! com a janela anterior já escolhida; os seguintes passam para a próxima.
//! Soltar o Alt põe a escolhida em foco. Se o Alt foi solto antes de a janela
//! aparecer (Alt+Tab rápido), o compositor avisa ao dar o foco
//! (`ModifiersChanged` sem Alt) e a troca acontece na hora; o painel só se
//! desenha depois de 120 ms, então nem chega a piscar. Esc cancela; setas
//! andam; Enter e clique escolhem.

use std::cell::RefCell;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use slint::winit_030::winit::event::{ElementState, WindowEvent};
use slint::winit_030::winit::keyboard::{Key, NamedKey};
use slint::winit_030::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, ModelRc, VecModel};

use crate::session::Mode;
use crate::toplevels::{Tracker, Window};
use crate::{AppWindow, Shell, Switcher, SwitcherItem};

/// Pedidos do atalho.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Request {
    /// Só deixa o processo aberto (início da sessão).
    Start,
    Next,
    Previous,
}

impl Request {
    /// Do argumento `--alternar[=anterior|=iniciar]`.
    pub fn from_args<I: IntoIterator<Item = String>>(args: I) -> Request {
        for a in args {
            match a.as_str() {
                "--alternar=anterior" => return Request::Previous,
                "--alternar=iniciar" => return Request::Start,
                _ => {}
            }
        }
        Request::Next
    }

    fn word(self) -> &'static str {
        match self {
            Request::Start => "iniciar",
            Request::Next => "proximo",
            Request::Previous => "anterior",
        }
    }

    fn parse(word: &str) -> Option<Request> {
        match word.trim() {
            "iniciar" => Some(Request::Start),
            "proximo" => Some(Request::Next),
            "anterior" => Some(Request::Previous),
            _ => None,
        }
    }
}

/// Primeira escolha ao abrir: a janela anterior (se a primeira está em
/// foco), ou a última da lista para trás.
pub fn first_choice(windows: &[Window], request: Request) -> usize {
    let n = windows.len();
    match request {
        Request::Previous => n.saturating_sub(1),
        _ if n > 1 && windows[0].active => 1,
        _ => 0,
    }
}

/// Próxima escolha (anda e dá a volta).
pub fn step(selected: usize, n: usize, forward: bool) -> usize {
    if n == 0 {
        return 0;
    }
    if forward {
        (selected + 1) % n
    } else {
        (selected + n - 1) % n
    }
}

fn socket_path() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|d| PathBuf::from(d).join("kalango-alternar.sock"))
}

/// Manda o pedido para o processo aberto. `false` se não há nenhum.
fn send(request: Request) -> bool {
    let Some(path) = socket_path() else { return false };
    UnixStream::connect(path).and_then(|mut s| s.write_all(request.word().as_bytes())).is_ok()
}

/// Inicia o processo do Alt+Tab escondido (área de trabalho, ao entrar).
pub fn spawn_background() {
    if socket_path().is_some_and(|p| UnixStream::connect(p).is_ok()) {
        return;
    }
    let _ = Command::new(crate::session::own_exe()).arg("--alternar=iniciar").stdin(Stdio::null()).spawn();
}

struct State {
    /// Janela do Alt+Tab aberta agora (criada a cada uso, destruída ao
    /// terminar).
    window: Option<AppWindow>,
    tracker: Option<Tracker>,
    /// Janelas mostradas (mesma ordem de `Switcher.items`).
    windows: Vec<Window>,
    visible: bool,
    /// A janela já recebeu o foco do teclado (perder o foco depois disso = cancelar).
    focused: bool,
    /// Pedido de mostrar o painel (depois de 120 ms).
    reveal: Option<slint::Timer>,
}

/// Diagnóstico (`KALANGO_DEBUG_ALTERNAR=1`): eventos e pedidos na saída de erro.
fn debug(msg: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("KALANGO_DEBUG_ALTERNAR").is_some()) {
        eprintln!("alternar: {}", msg());
    }
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with_borrow_mut(|s| s.as_mut().map(f))
}

/// Fecha a janela (o processo continua).
fn hide() {
    let ui = with_state(|s| {
        s.visible = false;
        s.focused = false;
        s.reveal = None;
        s.window.take()
    })
    .flatten();
    if let Some(ui) = ui {
        let _ = ui.hide();
    }
}

/// A janela aberta agora, se houver.
fn current() -> Option<AppWindow> {
    with_state(|s| s.window.as_ref().map(|w| w.clone_strong())).flatten()
}

/// Cria a janela do Alt+Tab.
///
/// No Wayland o winit não mapeia de novo uma janela escondida (`show()`
/// depois de `hide()` não faz nada), então cada Alt+Tab ganha uma janela
/// nova, só com o que o painel usa: modo, ícones dos apps e aparência.
fn new_window() -> Option<AppWindow> {
    let ui = AppWindow::new().map_err(|e| eprintln!("kalango-shell: Alt+Tab: {e}")).ok()?;
    let _ = slint::set_xdg_app_id(Mode::Switcher.app_id());
    // A tradução só pode ser escolhida depois de criar um componente.
    crate::i18n::apply();
    crate::session::install(&ui, Mode::Switcher);
    crate::apps::install(&ui, false, crate::session::is_native());
    crate::appearance::load(&ui);
    let sw = ui.global::<Switcher>();
    sw.on_choose(choose);
    sw.on_cancel(hide);
    ui.window().on_winit_window_event(|_, event| {
        on_window_event(event);
        EventResult::Propagate
    });
    Some(ui)
}

/// Põe a janela escolhida em foco e esconde.
fn commit() {
    let target = with_state(|s| {
        if !s.visible {
            return None;
        }
        let ui = s.window.as_ref()?.clone_strong();
        let i = ui.global::<Switcher>().get_selected().max(0) as usize;
        Some((s.tracker.clone()?, s.windows.get(i)?.id))
    })
    .flatten();
    hide();
    debug(|| format!("escolhida {target:?}", target = target.as_ref().map(|t| t.1)));
    if let Some((tracker, id)) = target {
        tracker.activate(id);
    }
}

fn choose(index: i32) {
    if let Some(ui) = current() {
        ui.global::<Switcher>().set_selected(index);
    }
    commit();
}

fn move_selection(forward: bool) {
    let Some(ui) = current() else { return };
    let n = with_state(|s| s.windows.len()).unwrap_or(0);
    let sw = ui.global::<Switcher>();
    sw.set_selected(step(sw.get_selected().max(0) as usize, n, forward) as i32);
}

/// Trata um pedido do atalho.
fn handle(request: Request) {
    debug(|| format!("pedido {request:?}"));
    if request == Request::Start {
        return;
    }
    let visible = with_state(|s| s.visible).unwrap_or(false);
    if visible {
        move_selection(request == Request::Next);
        return;
    }
    let windows: Vec<Window> = with_state(|s| s.tracker.as_ref().map(|t| t.windows()))
        .flatten()
        .map(crate::taskbar::with_ids)
        .unwrap_or_default()
        .into_iter()
        .map(|(w, _)| w)
        .collect();
    debug(|| format!("{} janelas: {:?}", windows.len(), windows.iter().map(|w| (&w.app_id, w.active)).collect::<Vec<_>>()));
    if windows.is_empty() {
        return;
    }
    let Some(ui) = new_window() else { return };
    let items: Vec<SwitcherItem> = windows
        .iter()
        .map(|w| {
            let id = crate::taskbar::dock_id(&w.app_id).unwrap_or_default();
            let app = crate::apps::entry_for(&ui, &id).unwrap_or_default();
            SwitcherItem {
                title: if w.title.is_empty() { app.name.clone() } else { w.title.clone().into() },
                app,
                minimized: w.minimized,
            }
        })
        .collect();
    let sw = ui.global::<Switcher>();
    sw.set_selected(first_choice(&windows, request) as i32);
    sw.set_items(ModelRc::from(Rc::new(VecModel::from(items))));
    ui.global::<Shell>().set_switcher_reveal(false);
    // O painel só aparece depois de 120 ms: num Alt+Tab rápido a troca
    // acontece antes e nada pisca.
    let reveal = slint::Timer::default();
    let weak = ui.as_weak();
    reveal.start(slint::TimerMode::SingleShot, Duration::from_millis(120), move || {
        if let Some(ui) = weak.upgrade() {
            ui.global::<Shell>().set_switcher_reveal(true);
        }
    });
    with_state(|s| {
        s.window = Some(ui.clone_strong());
        s.windows = windows;
        s.visible = true;
        s.focused = false;
        s.reveal = Some(reveal);
    });
    let shown = ui.show();
    debug(|| format!("show: {shown:?}"));
}

/// Eventos crus da janela: soltar o Alt, Esc, setas, perder o foco.
fn on_window_event(event: &WindowEvent) {
    if !matches!(event, WindowEvent::RedrawRequested | WindowEvent::CursorMoved { .. }) {
        debug(|| format!("evento {event:?}"));
    }
    // Fora do laço do winit (esconder a janela aqui dentro a destruiria no
    // meio do evento).
    let later = |f: fn()| {
        let _ = slint::invoke_from_event_loop(f);
    };
    match event {
        WindowEvent::ModifiersChanged(m) if !m.state().alt_key() && !m.state().super_key() => later(commit),
        WindowEvent::Focused(true) => {
            with_state(|s| s.focused = true);
        }
        WindowEvent::Focused(false) => {
            if with_state(|s| s.visible && s.focused).unwrap_or(false) {
                later(hide);
            }
        }
        WindowEvent::KeyboardInput { event, .. } => match (&event.logical_key, event.state) {
            (Key::Named(NamedKey::Alt | NamedKey::Super | NamedKey::Meta), ElementState::Released) => later(commit),
            (Key::Named(NamedKey::Escape), ElementState::Pressed) => later(hide),
            (Key::Named(NamedKey::Enter), ElementState::Pressed) => later(commit),
            (Key::Named(NamedKey::ArrowRight | NamedKey::Tab), ElementState::Pressed) => later(|| move_selection(true)),
            (Key::Named(NamedKey::ArrowLeft), ElementState::Pressed) => later(|| move_selection(false)),
            _ => {}
        },
        _ => {}
    }
}

/// Roda o modo `--alternar`: manda o pedido para o processo aberto ou vira ele.
pub fn run() -> Result<(), slint::PlatformError> {
    let request = Request::from_args(std::env::args());
    if send(request) {
        return Ok(());
    }
    let Some(path) = socket_path() else { return Ok(()) };
    let _ = std::fs::remove_file(&path);
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(err) => {
            eprintln!("kalango-shell: Alt+Tab: {err}");
            return Ok(());
        }
    };
    crate::select_renderer(Mode::Switcher);
    let tracker = Tracker::start(|_| {});
    STATE.with_borrow_mut(|s| {
        *s = Some(State { window: None, tracker, windows: Vec::new(), visible: false, focused: false, reveal: None })
    });

    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut text = String::new();
            let mut stream = stream;
            let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
            let _ = stream.read_to_string(&mut text);
            if let Some(request) = Request::parse(&text) {
                let _ = slint::invoke_from_event_loop(move || handle(request));
            }
        }
    });
    handle(request);
    slint::run_event_loop_until_quit()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(id: u32, active: bool) -> Window {
        Window { id, active, ..Default::default() }
    }

    #[test]
    fn pedidos() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(Request::from_args(args(&["x", "--alternar"])), Request::Next);
        assert_eq!(Request::from_args(args(&["x", "--alternar=anterior"])), Request::Previous);
        assert_eq!(Request::from_args(args(&["x", "--alternar=iniciar"])), Request::Start);
        assert_eq!(Request::parse("proximo\n"), Some(Request::Next));
        assert_eq!(Request::parse("?"), None);
    }

    #[test]
    fn escolha_inicial_e_passos() {
        // Em foco a primeira: começa na anterior.
        assert_eq!(first_choice(&[w(1, true), w(2, false)], Request::Next), 1);
        // Nenhuma em foco (área de trabalho): a mais recente.
        assert_eq!(first_choice(&[w(1, false), w(2, false)], Request::Next), 0);
        assert_eq!(first_choice(&[w(1, true)], Request::Next), 0);
        assert_eq!(first_choice(&[w(1, true), w(2, false), w(3, false)], Request::Previous), 2);
        assert_eq!(step(2, 3, true), 0);
        assert_eq!(step(0, 3, false), 2);
        assert_eq!(step(0, 0, true), 0);
    }
}
