//! Monitores: resolução, taxa de atualização, escala e rotação.
//!
//! Lê e altera as saídas de vídeo com o `wlr-randr` (protocolo
//! wlr-output-management, que o labwc implementa). Como em outros sistemas,
//! a mudança vale na hora mas só é guardada depois de confirmada: se ninguém
//! clicar em "Manter" em 15 s (por exemplo, a tela ficou preta), o shell volta
//! ao que estava antes.
//!
//! O que foi mantido vai para `~/.config/kalango/monitores.conf`, e a área de
//! trabalho reaplica o arquivo a cada início de sessão (o labwc não guarda).

use crate::i18n::tr;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::{AppWindow, Device, DisplayOutput, DisplayResolution, Mode};

/// Escalas oferecidas (mesma ordem de `Device.display-scales`).
pub const SCALES: [f64; 5] = [1.0, 1.25, 1.5, 1.75, 2.0];
/// Rotações do `wlr-randr --transform` (mesma ordem de `Device.display-rotations`).
pub const TRANSFORMS: [&str; 4] = ["normal", "90", "180", "270"];
/// Segundos para confirmar a mudança antes de voltar sozinho.
const CONFIRM_SECONDS: i32 = 15;

/// Um modo de vídeo de uma saída.
#[derive(Debug, Clone, PartialEq)]
pub struct VideoMode {
    pub width: u32,
    pub height: u32,
    /// Taxa como o `wlr-randr` escreve ("59.887001"): é o texto que ele
    /// reconhece de volta em `--mode`.
    pub refresh: String,
    pub preferred: bool,
    pub current: bool,
}

impl VideoMode {
    /// Taxa em Hz.
    pub fn hz(&self) -> f64 {
        self.refresh.parse().unwrap_or(0.0)
    }

    /// Valor de `--mode` ("1440x900@59.887001Hz").
    pub fn arg(&self) -> String {
        format!("{}x{}@{}Hz", self.width, self.height, self.refresh)
    }
}

/// Uma saída de vídeo (monitor) como o `wlr-randr` lista.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Output {
    /// Conector ("HDMI-A-1", "eDP-1", "Virtual-1").
    pub name: String,
    /// Fabricante e modelo ("Red Hat, Inc. QEMU Monitor").
    pub description: String,
    pub enabled: bool,
    pub modes: Vec<VideoMode>,
    pub scale: f64,
    /// "normal", "90", "180", "270" (ou "flipped-*").
    pub transform: String,
}

/// Uma resolução com as taxas disponíveis (da maior para a menor).
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution<'a> {
    pub width: u32,
    pub height: u32,
    pub preferred: bool,
    pub modes: Vec<&'a VideoMode>,
}

/// Lê a saída de texto do `wlr-randr`.
pub fn parse(text: &str) -> Vec<Output> {
    let mut outputs: Vec<Output> = Vec::new();
    let mut in_modes = false;
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        if !line.starts_with(' ') {
            // Cabeçalho: Virtual-1 "Red Hat, Inc. QEMU Monitor (Virtual-1)"
            let (name, rest) = line.split_once(' ').unwrap_or((line, ""));
            let description = rest.trim().trim_matches('"');
            let description = description
                .strip_suffix(&format!(" ({name})"))
                .unwrap_or(description);
            outputs.push(Output {
                name: name.to_string(),
                description: description.to_string(),
                scale: 1.0,
                transform: "normal".into(),
                ..Default::default()
            });
            in_modes = false;
            continue;
        }
        let Some(out) = outputs.last_mut() else { continue };
        let trimmed = line.trim();
        // As linhas de modo têm recuo maior que as propriedades.
        if in_modes && line.starts_with("    ") {
            if let Some(mode) = parse_mode(trimmed) {
                out.modes.push(mode);
            }
            continue;
        }
        in_modes = false;
        let Some((key, value)) = trimmed.split_once(':') else { continue };
        let value = value.trim();
        match key {
            "Enabled" => out.enabled = value == "yes",
            "Modes" => in_modes = true,
            "Scale" => out.scale = value.parse().unwrap_or(1.0),
            "Transform" => out.transform = value.to_string(),
            _ => {}
        }
    }
    outputs
}

/// Lê uma linha de modo: "1440x900 px, 59.887001 Hz (preferred, current)".
fn parse_mode(line: &str) -> Option<VideoMode> {
    let (size, rest) = line.split_once(" px, ")?;
    let (w, h) = size.split_once('x')?;
    let (refresh, flags) = rest.split_once(" Hz").unwrap_or((rest, ""));
    Some(VideoMode {
        width: w.trim().parse().ok()?,
        height: h.trim().parse().ok()?,
        refresh: refresh.trim().to_string(),
        preferred: flags.contains("preferred"),
        current: flags.contains("current"),
    })
}

impl Output {
    /// Resoluções sem repetir, da maior para a menor área; em cada uma, as
    /// taxas da maior para a menor.
    pub fn resolutions(&self) -> Vec<Resolution<'_>> {
        let mut list: Vec<Resolution> = Vec::new();
        for mode in &self.modes {
            match list.iter_mut().find(|r| r.width == mode.width && r.height == mode.height) {
                Some(r) => {
                    if !r.modes.iter().any(|m| m.refresh == mode.refresh) {
                        r.modes.push(mode);
                    }
                    r.preferred |= mode.preferred;
                }
                None => list.push(Resolution {
                    width: mode.width,
                    height: mode.height,
                    preferred: mode.preferred,
                    modes: vec![mode],
                }),
            }
        }
        list.sort_by_key(|r| std::cmp::Reverse((r.width * r.height, r.width)));
        for r in &mut list {
            r.modes.sort_by(|a, b| b.hz().total_cmp(&a.hz()));
        }
        list
    }

    /// Índices (resolução, taxa) do modo atual em [`Output::resolutions`].
    pub fn current_indices(&self) -> (usize, usize) {
        for (i, r) in self.resolutions().iter().enumerate() {
            if let Some(j) = r.modes.iter().position(|m| m.current) {
                return (i, j);
            }
        }
        (0, 0)
    }

    /// Índice em [`SCALES`] mais perto da escala atual.
    pub fn scale_index(&self) -> usize {
        SCALES
            .iter()
            .enumerate()
            .min_by(|a, b| (a.1 - self.scale).abs().total_cmp(&(b.1 - self.scale).abs()))
            .map_or(0, |(i, _)| i)
    }

    /// Índice em [`TRANSFORMS`] da rotação atual (espelhadas contam como normal).
    pub fn rotation_index(&self) -> usize {
        TRANSFORMS.iter().position(|t| *t == self.transform).unwrap_or(0)
    }

    /// Estado atual da saída, para reaplicar (voltar atrás).
    pub fn current_setting(&self) -> Setting {
        let mode = self.modes.iter().find(|m| m.current).cloned();
        Setting {
            output: self.name.clone(),
            enabled: self.enabled,
            mode,
            scale: self.scale,
            transform: self.transform.clone(),
        }
    }
}

/// Configuração de uma saída para aplicar com o `wlr-randr`.
#[derive(Debug, Clone, PartialEq)]
pub struct Setting {
    pub output: String,
    pub enabled: bool,
    /// `None`: mantém o modo atual.
    pub mode: Option<VideoMode>,
    pub scale: f64,
    pub transform: String,
}

impl Setting {
    /// Argumentos do `wlr-randr`.
    pub fn args(&self) -> Vec<String> {
        let mut args = vec!["--output".to_string(), self.output.clone()];
        if !self.enabled {
            args.push("--off".into());
            return args;
        }
        args.push("--on".into());
        if let Some(mode) = &self.mode {
            args.push("--mode".into());
            args.push(mode.arg());
        }
        args.push("--scale".into());
        args.push(format_scale(self.scale));
        args.push("--transform".into());
        args.push(self.transform.clone());
        args
    }

    /// Linha do `monitores.conf`.
    pub fn to_line(&self) -> String {
        let mode = self
            .mode
            .as_ref()
            .map(|m| format!(" modo={}x{}@{}", m.width, m.height, m.refresh))
            .unwrap_or_default();
        format!(
            "{}{mode} escala={} rotacao={} ativo={}",
            self.output,
            format_scale(self.scale),
            self.transform,
            if self.enabled { "sim" } else { "nao" }
        )
    }

    /// Lê uma linha do `monitores.conf` (`None` em comentário ou linha inválida).
    pub fn from_line(line: &str) -> Option<Setting> {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let mut parts = line.split_whitespace();
        let mut setting = Setting {
            output: parts.next()?.to_string(),
            enabled: true,
            mode: None,
            scale: 1.0,
            transform: "normal".into(),
        };
        for part in parts {
            let Some((key, value)) = part.split_once('=') else { continue };
            match key {
                "modo" => {
                    let (size, refresh) = value.split_once('@')?;
                    let (w, h) = size.split_once('x')?;
                    setting.mode = Some(VideoMode {
                        width: w.parse().ok()?,
                        height: h.parse().ok()?,
                        refresh: refresh.to_string(),
                        preferred: false,
                        current: false,
                    });
                }
                "escala" => setting.scale = value.parse().ok().filter(|s| *s > 0.0)?,
                "rotacao" if TRANSFORMS.contains(&value) => setting.transform = value.into(),
                "ativo" => setting.enabled = value != "nao",
                _ => {}
            }
        }
        Some(setting)
    }
}

/// Escala sem zeros sobrando ("1", "1.25", "1.5").
fn format_scale(scale: f64) -> String {
    let text = format!("{scale:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Taxa para exibir: "75 Hz", "59,94 Hz".
pub fn rate_label(hz: f64) -> String {
    let text = format!("{hz:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{} Hz", text.replace('.', ","))
}

/// A escala deixa espaço para o shell? Ele precisa de pelo menos 1024×600
/// pontos lógicos (a rotação de 90°/270° troca largura e altura).
pub fn scale_fits(width: u32, height: u32, rotation: usize, scale: f64) -> bool {
    let (w, h) = if rotation % 2 == 1 { (height, width) } else { (width, height) };
    scale <= 1.0 || (w as f64 / scale >= 1024.0 && h as f64 / scale >= 600.0)
}

/// Caminho do arquivo dos monitores (`~/.config/kalango/monitores.conf`).
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/monitores.conf"))
}

/// Junta a configuração de uma saída ao arquivo (substitui a linha dela).
pub fn merge_config(text: &str, setting: &Setting) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| Setting::from_line(l).is_none_or(|s| s.output != setting.output))
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect();
    if !lines.iter().any(|l| l.starts_with('#')) {
        lines.insert(0, "# Monitores do Kalango OS (gravado pelas Configurações).".into());
    }
    lines.push(setting.to_line());
    lines.join("\n") + "\n"
}

/// Roda o `wlr-randr` e devolve a saída de texto (`None` se falhou).
fn wlr_randr(args: &[String]) -> Option<String> {
    let out = Command::new("wlr-randr")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        eprintln!(
            "kalango-shell: wlr-randr {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Lê as saídas atuais.
fn read_outputs() -> Option<Vec<Output>> {
    wlr_randr(&[]).map(|text| parse(&text))
}

/// Reaplica o `monitores.conf` (início de sessão), só nas saídas conectadas.
fn apply_saved() {
    let Some(text) = config_path().and_then(|p| fs::read_to_string(p).ok()) else {
        return;
    };
    let Some(outputs) = read_outputs() else { return };
    for setting in text.lines().filter_map(Setting::from_line) {
        if outputs.iter().any(|o| o.name == setting.output) {
            let _ = wlr_randr(&setting.args());
        }
    }
}

/// Converte as saídas para o modelo da UI.
fn to_ui(outputs: &[Output]) -> Vec<DisplayOutput> {
    outputs
        .iter()
        .map(|out| {
            let resolutions: Vec<DisplayResolution> = out
                .resolutions()
                .iter()
                .map(|r| {
                    let rates: Vec<SharedString> =
                        r.modes.iter().map(|m| rate_label(m.hz()).into()).collect();
                    DisplayResolution {
                        label: format!(
                            "{} × {}{}",
                            r.width,
                            r.height,
                            if r.preferred { " (recomendada)" } else { "" }
                        )
                        .into(),
                        width: r.width as i32,
                        height: r.height as i32,
                        rates: ModelRc::from(Rc::new(VecModel::from(rates))),
                    }
                })
                .collect();
            let labels: Vec<SharedString> = resolutions.iter().map(|r| r.label.clone()).collect();
            let (res, rate) = out.current_indices();
            DisplayOutput {
                name: out.name.clone().into(),
                label: if out.description.is_empty() {
                    out.name.clone().into()
                } else {
                    format!("{} ({})", out.description, out.name).into()
                },
                enabled: out.enabled,
                resolutions: ModelRc::from(Rc::new(VecModel::from(resolutions))),
                resolution_labels: ModelRc::from(Rc::new(VecModel::from(labels))),
                resolution: res as i32,
                rate: rate as i32,
                scale: out.scale_index() as i32,
                rotation: out.rotation_index() as i32,
            }
        })
        .collect()
}

/// Estado do processo das Configurações: saídas lidas e mudança esperando
/// confirmação.
#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    /// Como estava antes da última mudança (para voltar).
    previous: Option<Setting>,
    /// O que foi aplicado e espera "Manter".
    pending: Option<Setting>,
    countdown: Option<Timer>,
}

/// Mostra as saídas na UI.
fn show(ui: &AppWindow, state: &mut State, outputs: Option<Vec<Output>>) {
    let device = ui.global::<Device>();
    match outputs {
        Some(list) if !list.is_empty() => {
            device.set_displays(ModelRc::from(Rc::new(VecModel::from(to_ui(&list)))));
            device.set_displays_available(true);
            state.outputs = list;
        }
        _ => device.set_displays_available(false),
    }
}

/// Termina a confirmação: guarda a mudança (`keep`) ou volta ao que estava.
fn finish(ui: &AppWindow, state: &Rc<RefCell<State>>, keep: bool) {
    let mut st = state.borrow_mut();
    st.countdown = None;
    let device = ui.global::<Device>();
    device.set_display_confirm(false);
    let pending = st.pending.take();
    let previous = st.previous.take();
    if keep {
        if let (Some(setting), Some(path)) = (pending, config_path()) {
            let text = fs::read_to_string(&path).unwrap_or_default();
            if let Some(dir) = path.parent() {
                let _ = fs::create_dir_all(dir);
            }
            match fs::write(&path, merge_config(&text, &setting)) {
                Ok(()) => device.set_display_message(tr("Display configuration saved.", "Configuração do monitor salva.").into()),
                Err(err) => device.set_display_message(format!("{}: {err}", tr("Could not save", "Não foi possível salvar")).into()),
            }
        }
    } else {
        if let Some(setting) = previous {
            let _ = wlr_randr(&setting.args());
        }
        device.set_display_message(
            tr("The display went back to the previous configuration.", "O monitor voltou à configuração anterior.").into(),
        );
    }
    let outputs = read_outputs();
    show(ui, &mut st, outputs);
}

/// Liga a página Monitores (processo das Configurações) e reaplica o
/// `monitores.conf` no início da sessão (área de trabalho).
pub fn install(ui: &AppWindow, mode: Mode) {
    let native = crate::session::is_native();
    if native && mode == Mode::Desktop {
        std::thread::spawn(apply_saved);
        return;
    }
    if !(native && mode == Mode::Settings) {
        return;
    }

    let state = Rc::new(RefCell::new(State::default()));
    show(ui, &mut state.borrow_mut(), read_outputs());

    let device = ui.global::<Device>();
    let weak = ui.as_weak();
    let st = state.clone();
    device.on_apply_display(move |index, res, rate, scale, rotation, enabled| {
        let Some(ui) = weak.upgrade() else { return };
        let device = ui.global::<Device>();
        let mut s = st.borrow_mut();
        let Some(out) = s.outputs.get(index as usize).cloned() else { return };
        let mode = out
            .resolutions()
            .get(res as usize)
            .and_then(|r| r.modes.get(rate as usize).map(|m| (*m).clone()));
        let setting = Setting {
            output: out.name.clone(),
            enabled,
            mode,
            scale: SCALES.get(scale as usize).copied().unwrap_or(1.0),
            transform: TRANSFORMS.get(rotation as usize).unwrap_or(&"normal").to_string(),
        };
        // Nunca desliga a única saída ligada (a tela ficaria preta).
        let others_on = s.outputs.iter().any(|o| o.name != out.name && o.enabled);
        if !enabled && !others_on {
            device.set_display_message("Pelo menos um monitor precisa ficar ligado.".into());
            return;
        }
        if wlr_randr(&setting.args()).is_none() {
            device.set_display_message(
                tr("The display did not accept this configuration.", "O monitor não aceitou essa configuração.").into(),
            );
            return;
        }
        s.previous = Some(out.current_setting());
        s.pending = Some(setting);
        device.set_display_message(SharedString::new());
        device.set_display_countdown(CONFIRM_SECONDS);
        device.set_display_confirm(true);

        // Contagem regressiva: chegando a zero, volta ao que estava.
        let timer = Timer::default();
        let weak = ui.as_weak();
        let st2 = st.clone();
        timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
            let Some(ui) = weak.upgrade() else { return };
            let device = ui.global::<Device>();
            let left = device.get_display_countdown() - 1;
            device.set_display_countdown(left.max(0));
            if left <= 0 {
                // Fora do callback do timer: `finish` descarta este timer.
                let weak = ui.as_weak();
                let st3 = st2.clone();
                slint::Timer::single_shot(Duration::ZERO, move || {
                    if let Some(ui) = weak.upgrade() {
                        finish(&ui, &st3, false);
                    }
                });
            }
        });
        s.countdown = Some(timer);
        let outputs = read_outputs();
        // Atualiza a lista sem perder a mudança pendente.
        let pending = s.pending.clone();
        let previous = s.previous.clone();
        show(&ui, &mut s, outputs);
        s.pending = pending;
        s.previous = previous;
    });

    let weak = ui.as_weak();
    let st = state.clone();
    device.on_keep_display(move || {
        if let Some(ui) = weak.upgrade() {
            finish(&ui, &st, true);
        }
    });
    let weak = ui.as_weak();
    device.on_revert_display(move || {
        if let Some(ui) = weak.upgrade() {
            finish(&ui, &state, false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const QEMU: &str = r#"Virtual-1 "Red Hat, Inc. QEMU Monitor (Virtual-1)"
  Make: Red Hat, Inc.
  Model: QEMU Monitor
  Serial: (null)
  Physical size: 360x220 mm
  Enabled: yes
  Modes:
    1440x900 px, 74.995003 Hz (preferred, current)
    1920x1080 px, 60.000000 Hz
    3840x2160 px, 60.000000 Hz
    3840x2160 px, 59.939999 Hz
    1440x900 px, 59.887001 Hz
    1024x768 px, 60.004002 Hz
  Position: 0,0
  Transform: normal
  Scale: 1.000000
  Adaptive Sync: disabled
HDMI-A-1 "Dell Inc. DELL P2419H (HDMI-A-1)"
  Enabled: no
  Modes:
    1920x1080 px, 60.000000 Hz (preferred)
  Position: 0,0
  Transform: 90
  Scale: 1.500000
"#;

    #[test]
    fn le_as_saidas_do_wlr_randr() {
        let outs = parse(QEMU);
        assert_eq!(outs.len(), 2);
        let q = &outs[0];
        assert_eq!(q.name, "Virtual-1");
        assert_eq!(q.description, "Red Hat, Inc. QEMU Monitor");
        assert!(q.enabled);
        assert_eq!(q.modes.len(), 6);
        assert!(q.modes[0].preferred && q.modes[0].current);
        assert_eq!(q.modes[4].refresh, "59.887001");
        assert_eq!(outs[1].transform, "90");
        assert!(!outs[1].enabled);
        assert_eq!(outs[1].scale, 1.5);
    }

    #[test]
    fn agrupa_resolucoes_e_acha_o_modo_atual() {
        let outs = parse(QEMU);
        let res = outs[0].resolutions();
        let sizes: Vec<(u32, u32)> = res.iter().map(|r| (r.width, r.height)).collect();
        assert_eq!(sizes, [(3840, 2160), (1920, 1080), (1440, 900), (1024, 768)]);
        assert!(res[2].preferred);
        let rates: Vec<&str> = res[2].modes.iter().map(|m| m.refresh.as_str()).collect();
        assert_eq!(rates, ["74.995003", "59.887001"]);
        assert_eq!(outs[0].current_indices(), (2, 0));
        assert_eq!(outs[1].rotation_index(), 1);
        assert_eq!(outs[1].scale_index(), 2);
    }

    #[test]
    fn monta_o_comando_e_o_arquivo() {
        let outs = parse(QEMU);
        let mut s = outs[0].current_setting();
        assert_eq!(
            s.args(),
            ["--output", "Virtual-1", "--on", "--mode", "1440x900@74.995003Hz", "--scale", "1", "--transform", "normal"]
        );
        s.scale = 1.25;
        s.transform = "270".into();
        let line = s.to_line();
        assert_eq!(line, "Virtual-1 modo=1440x900@74.995003 escala=1.25 rotacao=270 ativo=sim");
        assert_eq!(Setting::from_line(&line).unwrap().args(), s.args());
        s.enabled = false;
        assert_eq!(s.args(), ["--output", "Virtual-1", "--off"]);
        assert!(Setting::from_line("# comentário").is_none());
    }

    #[test]
    fn substitui_a_linha_da_saida_no_arquivo() {
        let mut s = parse(QEMU)[0].current_setting();
        let text = merge_config("", &s);
        s.scale = 2.0;
        let text = merge_config(&format!("{text}HDMI-A-1 ativo=nao\n"), &s);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[0].starts_with('#'));
        assert_eq!(lines[1], "HDMI-A-1 ativo=nao");
        assert!(lines[2].contains("escala=2 "));
    }

    #[test]
    fn rotulos_e_limites_de_escala() {
        assert_eq!(rate_label(74.995003), "75 Hz");
        assert_eq!(rate_label(59.939999), "59,94 Hz");
        assert_eq!(rate_label(60.0), "60 Hz");
        assert!(scale_fits(1440, 900, 0, 1.0));
        assert!(!scale_fits(1440, 900, 0, 1.5));
        assert!(scale_fits(2880, 1800, 0, 2.0));
        // Em pé, a largura vira 1800: 1800/2 = 900 < 1024.
        assert!(!scale_fits(2880, 1800, 1, 2.0));
    }
}
