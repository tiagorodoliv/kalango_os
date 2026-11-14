//! Áreas de trabalho na barra superior, pelo protocolo `ext-workspace-v1`
//! do labwc (0.8.3 em diante).
//!
//! Uma thread abre a própria conexão Wayland (separada da do Slint/winit),
//! acompanha as áreas de trabalho (nome, ordem, qual está ativa) e manda
//! para a barra a quantidade e a ativa (`Shell.workspace-count` e
//! `Shell.workspace-active`). Clicar num ponto da barra pede ao labwc para
//! ativar aquela área (`activate` + `commit`), de qualquer thread: os objetos
//! do `wayland-client` podem ser usados fora da thread da fila.
//!
//! As áreas são definidas no `rc.xml` (`<desktops number="4">`); os atalhos
//! (Super+1…4, Ctrl+Alt+setas) também estão lá.

use std::sync::{Arc, Mutex};

use slint::ComponentHandle;
use wayland_client::protocol::wl_registry;
use wayland_client::{event_created_child, Connection, Dispatch, Proxy, QueueHandle};
use wayland_protocols::ext::workspace::v1::client::{
    ext_workspace_group_handle_v1::{self, ExtWorkspaceGroupHandleV1},
    ext_workspace_handle_v1::{self, ExtWorkspaceHandleV1},
    ext_workspace_manager_v1::{self, ExtWorkspaceManagerV1},
};

use crate::session::Mode;
use crate::{AppWindow, Shell};

/// Uma área de trabalho anunciada pelo compositor.
struct Workspace {
    handle: ExtWorkspaceHandleV1,
    /// Posição (o labwc manda a ordem em `coordinates`; sem ela, a ordem de
    /// chegada).
    order: u32,
    active: bool,
}

/// O que a UI precisa para trocar de área: o gerenciador e as áreas na ordem.
#[derive(Default)]
struct Shared {
    manager: Option<ExtWorkspaceManagerV1>,
    ordered: Vec<ExtWorkspaceHandleV1>,
}

struct State {
    list: Vec<Workspace>,
    shared: Arc<Mutex<Shared>>,
    weak: slint::Weak<AppWindow>,
    /// Último estado enviado à barra (para não repetir).
    sent: Option<(i32, i32)>,
}

impl State {
    /// Fim de um lote de mudanças (`done`): ordena e atualiza a barra.
    fn publish(&mut self) {
        self.list.sort_by_key(|w| w.order);
        let count = self.list.len() as i32;
        let active = self.list.iter().position(|w| w.active).map_or(0, |i| i as i32);
        if let Ok(mut shared) = self.shared.lock() {
            shared.ordered = self.list.iter().map(|w| w.handle.clone()).collect();
        }
        if self.sent == Some((count, active)) {
            return;
        }
        self.sent = Some((count, active));
        let weak = self.weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() {
                let shell = ui.global::<Shell>();
                shell.set_workspace_count(count);
                shell.set_workspace_active(active);
            }
        });
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, .. } = event {
            if interface == ExtWorkspaceManagerV1::interface().name {
                let manager = registry.bind::<ExtWorkspaceManagerV1, _, _>(name, 1, qh, ());
                if let Ok(mut shared) = state.shared.lock() {
                    shared.manager = Some(manager);
                }
            }
        }
    }
}

impl Dispatch<ExtWorkspaceManagerV1, ()> for State {
    fn event(state: &mut Self, _: &ExtWorkspaceManagerV1, event: ext_workspace_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            ext_workspace_manager_v1::Event::Workspace { workspace } => {
                let order = state.list.len() as u32;
                state.list.push(Workspace { handle: workspace, order, active: false });
            }
            ext_workspace_manager_v1::Event::Done => state.publish(),
            _ => {}
        }
    }

    event_created_child!(State, ExtWorkspaceManagerV1, [
        ext_workspace_manager_v1::EVT_WORKSPACE_GROUP_OPCODE => (ExtWorkspaceGroupHandleV1, ()),
        ext_workspace_manager_v1::EVT_WORKSPACE_OPCODE => (ExtWorkspaceHandleV1, ()),
    ]);
}

impl Dispatch<ExtWorkspaceGroupHandleV1, ()> for State {
    fn event(_: &mut Self, _: &ExtWorkspaceGroupHandleV1, _: ext_workspace_group_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtWorkspaceHandleV1, ()> for State {
    fn event(state: &mut Self, handle: &ExtWorkspaceHandleV1, event: ext_workspace_handle_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        let Some(ws) = state.list.iter_mut().find(|w| &w.handle == handle) else { return };
        match event {
            ext_workspace_handle_v1::Event::Coordinates { coordinates } => {
                // Coordenadas: u32 em bytes nativos; a primeira é a posição.
                if coordinates.len() >= 4 {
                    ws.order = u32::from_ne_bytes([coordinates[0], coordinates[1], coordinates[2], coordinates[3]]);
                }
            }
            ext_workspace_handle_v1::Event::State { state: bits } => {
                ws.active = bits
                    .into_result()
                    .is_ok_and(|s| s.contains(ext_workspace_handle_v1::State::Active));
            }
            ext_workspace_handle_v1::Event::Removed => {
                handle.destroy();
                state.list.retain(|w| &w.handle != handle);
            }
            _ => {}
        }
    }
}

/// Começa a acompanhar as áreas de trabalho (só a área de trabalho da sessão
/// real tem a barra). Sem o protocolo, a barra esconde o indicador.
pub fn install(ui: &AppWindow, mode: Mode) {
    if mode != Mode::Desktop || !crate::session::is_native() {
        return;
    }
    let shell = ui.global::<Shell>();
    // Até o compositor responder (ou se ele não tiver o protocolo): sem indicador.
    shell.set_workspace_count(0);

    let Ok(conn) = Connection::connect_to_env() else { return };
    let shared = Arc::new(Mutex::new(Shared::default()));

    let ui_shared = shared.clone();
    let ui_conn = conn.clone();
    shell.on_switch_workspace(move |index| {
        let Ok(shared) = ui_shared.lock() else { return };
        let (Some(manager), Some(ws)) = (&shared.manager, shared.ordered.get(index.max(0) as usize)) else {
            return;
        };
        ws.activate();
        manager.commit();
        let _ = ui_conn.flush();
    });

    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let mut queue = conn.new_event_queue();
        let qh = queue.handle();
        conn.display().get_registry(&qh, ());
        let mut state = State { list: Vec::new(), shared, weak, sent: None };
        loop {
            if let Err(err) = queue.blocking_dispatch(&mut state) {
                eprintln!("kalango-shell: áreas de trabalho: {err}");
                break;
            }
        }
    });
}
