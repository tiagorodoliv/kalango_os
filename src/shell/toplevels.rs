//! Janelas abertas na sessão, pelo protocolo `wlr-foreign-toplevel` do labwc
//! (o mesmo do `wlrctl`).
//!
//! Uma thread abre a própria conexão Wayland (separada da do Slint/winit) e
//! acompanha cada janela: `app_id`, título, se está em foco ou minimizada.
//! Guarda também a ordem de uso (a janela em foco mais recente primeiro),
//! que o Alt+Tab usa para alternar com a anterior. A cada mudança chama o
//! `on_change` (na thread da conexão) com a lista nova.
//!
//! As ações (ativar, minimizar, fechar) podem ser chamadas de qualquer
//! thread: os objetos do `wayland-client` funcionam fora da thread da fila.

use std::sync::{Arc, Mutex};

use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

/// Uma janela aberta.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// Id da janela nesta conexão (vale enquanto ela existir).
    pub id: u32,
    pub app_id: String,
    pub title: String,
    /// Em foco.
    pub active: bool,
    pub minimized: bool,
    /// Maximizada (ocupa a área livre entre as margens da barra e da dock).
    pub maximized: bool,
}

/// Estados de uma janela no protocolo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct States {
    active: bool,
    minimized: bool,
    maximized: bool,
}

/// Estados do protocolo (array de u32 em bytes nativos): 0 = maximizada,
/// 1 = minimizada, 2 = em foco, 3 = tela cheia.
fn parse_states(bytes: &[u8]) -> States {
    let states: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .collect();
    States { active: states.contains(&2), minimized: states.contains(&1), maximized: states.contains(&0) }
}

struct Entry {
    handle: ZwlrForeignToplevelHandleV1,
    info: Window,
    /// Quando ficou em foco pela última vez (contador; maior = mais recente).
    used: u64,
}

#[derive(Default)]
struct Inner {
    list: Vec<Entry>,
    seat: Option<WlSeat>,
    counter: u64,
}

impl Inner {
    /// Janelas da mais recentemente usada para a menos.
    fn windows(&self) -> Vec<Window> {
        let mut v: Vec<&Entry> = self.list.iter().filter(|e| !e.info.app_id.is_empty() || !e.info.title.is_empty()).collect();
        v.sort_by_key(|e| std::cmp::Reverse(e.used));
        v.into_iter().map(|e| e.info.clone()).collect()
    }
}

type OnChange = Box<dyn Fn(Vec<Window>) + Send>;

struct State {
    inner: Arc<Mutex<Inner>>,
    on_change: OnChange,
}

impl State {
    fn publish(&self) {
        let list = match self.inner.lock() {
            Ok(inner) => inner.windows(),
            Err(_) => return,
        };
        (self.on_change)(list);
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            if interface == ZwlrForeignToplevelManagerV1::interface().name {
                registry.bind::<ZwlrForeignToplevelManagerV1, _, _>(name, version.min(3), qh, ());
            } else if interface == WlSeat::interface().name {
                let seat = registry.bind::<WlSeat, _, _>(name, 1, qh, ());
                if let Ok(mut inner) = state.inner.lock() {
                    inner.seat.get_or_insert(seat);
                }
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(_: &mut Self, _: &WlSeat, _: wayland_client::protocol::wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn event(state: &mut Self, _: &ZwlrForeignToplevelManagerV1, event: zwlr_foreign_toplevel_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
            if let Ok(mut inner) = state.inner.lock() {
                inner.counter += 1;
                let used = inner.counter;
                let id = toplevel.id().protocol_id();
                inner.list.push(Entry { handle: toplevel, info: Window { id, ..Default::default() }, used });
            }
        }
    }

    event_created_child!(State, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for State {
    fn event(state: &mut Self, handle: &ZwlrForeignToplevelHandleV1, event: zwlr_foreign_toplevel_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        use zwlr_foreign_toplevel_handle_v1::Event;
        let mut publish = false;
        if let Ok(mut inner) = state.inner.lock() {
            inner.counter += 1;
            let now = inner.counter;
            match event {
                Event::Closed => {
                    handle.destroy();
                    inner.list.retain(|e| &e.handle != handle);
                    publish = true;
                }
                event => {
                    let Some(e) = inner.list.iter_mut().find(|e| &e.handle == handle) else { return };
                    match event {
                        Event::Title { title } => e.info.title = title,
                        Event::AppId { app_id } => e.info.app_id = app_id,
                        Event::State { state: bytes } => {
                            let st = parse_states(&bytes);
                            if st.active && !e.info.active {
                                e.used = now;
                            }
                            e.info.active = st.active;
                            e.info.minimized = st.minimized;
                            e.info.maximized = st.maximized;
                        }
                        Event::Done => publish = true,
                        _ => {}
                    }
                }
            }
        }
        if publish {
            state.publish();
        }
    }
}

/// Conexão que acompanha as janelas; as ações valem para qualquer thread.
#[derive(Clone)]
pub struct Tracker {
    conn: Connection,
    inner: Arc<Mutex<Inner>>,
}

impl Tracker {
    /// Conecta ao compositor e começa a acompanhar as janelas. `None` fora
    /// do Wayland ou sem o protocolo.
    pub fn start(on_change: impl Fn(Vec<Window>) + Send + 'static) -> Option<Tracker> {
        let conn = Connection::connect_to_env().ok()?;
        let inner = Arc::new(Mutex::new(Inner::default()));
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = State { inner: inner.clone(), on_change: Box::new(on_change) };
        // Primeira volta aqui mesmo: o gerenciador e o seat chegam no
        // registro; as janelas que já existem, logo depois.
        queue.roundtrip(&mut state).ok()?;
        queue.roundtrip(&mut state).ok()?;
        std::thread::spawn(move || loop {
            if let Err(err) = queue.blocking_dispatch(&mut state) {
                eprintln!("kalango-shell: janelas abertas: {err}");
                break;
            }
        });
        Some(Tracker { conn, inner })
    }

    /// Janelas da mais recentemente usada para a menos.
    pub fn windows(&self) -> Vec<Window> {
        self.inner.lock().map(|i| i.windows()).unwrap_or_default()
    }

    fn with_handle(&self, id: u32, f: impl FnOnce(&ZwlrForeignToplevelHandleV1, Option<&WlSeat>)) {
        if let Ok(inner) = self.inner.lock() {
            if let Some(e) = inner.list.iter().find(|e| e.info.id == id) {
                f(&e.handle, inner.seat.as_ref());
            }
        }
        let _ = self.conn.flush();
    }

    /// Põe a janela em foco (e tira de minimizada).
    pub fn activate(&self, id: u32) {
        self.with_handle(id, |h, seat| {
            if let Some(seat) = seat {
                h.unset_minimized();
                h.activate(seat);
            }
        });
    }

    /// Minimiza a janela.
    pub fn minimize(&self, id: u32) {
        self.with_handle(id, |h, _| h.set_minimized());
    }

    /// Pede para a janela fechar (o app pode perguntar antes).
    pub fn close(&self, id: u32) {
        self.with_handle(id, |h, _| h.close());
    }

    /// Maximiza de novo as janelas maximizadas, para caberem nas margens
    /// atuais do labwc. O labwc só recalcula a área de uma janela maximizada
    /// ao maximizá-la: sem isso, mudar a margem da dock (tamanho, "solta da
    /// borda", esconder) deixava as janelas abertas por trás da dock ou sem
    /// ocupar a tela toda. As minimizadas ficam como estão.
    pub fn refit_maximized(&self) {
        if let Ok(inner) = self.inner.lock() {
            for e in inner.list.iter().filter(|e| e.info.maximized && !e.info.minimized) {
                e.handle.unset_maximized();
                e.handle.set_maximized();
            }
        }
        let _ = self.conn.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estados_do_protocolo() {
        let bytes: Vec<u8> = [0u32, 2].iter().flat_map(|v| v.to_ne_bytes()).collect();
        assert_eq!(parse_states(&bytes), States { active: true, minimized: false, maximized: true });
        let bytes: Vec<u8> = [1u32].iter().flat_map(|v| v.to_ne_bytes()).collect();
        assert_eq!(parse_states(&bytes), States { minimized: true, ..States::default() });
        assert_eq!(parse_states(&[]), States::default());
    }
}
