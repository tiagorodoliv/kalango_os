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

use crate::common::i18n::tr;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use slint::{ComponentHandle, ModelRc, SharedString, Timer, TimerMode, VecModel};

use crate::{AppWindow, Device, DisplayOutput, DisplayResolution, Mode};

/// Escalas oferecidas (mesma ordem de `Device.display-scales`).
/// 50% e 75% mostram mais coisas na tela (tudo menor), como no Windows.
pub const SCALES: [f64; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];
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
    /// Posição no arranjo dos monitores (px lógicos).
    pub x: i32,
    pub y: i32,
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
            "Position" => {
                if let Some((x, y)) = value.split_once(',') {
                    out.x = x.trim().parse().unwrap_or(0);
                    out.y = y.trim().parse().unwrap_or(0);
                }
            }
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
            position: Some((self.x, self.y)),
        }
    }

    /// Tamanho na tela, em px lógicos (modo atual, escala e rotação).
    pub fn logical_size(&self) -> (u32, u32) {
        // Desligado não tem modo atual: usa o preferido.
        let Some(m) = self.modes.iter().find(|m| m.current).or_else(|| self.modes.iter().find(|m| m.preferred)) else {
            return (0, 0);
        };
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        let (w, h) = ((m.width as f64 / scale).round() as u32, (m.height as f64 / scale).round() as u32);
        if self.transform.ends_with("90") || self.transform.ends_with("270") { (h, w) } else { (w, h) }
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
    /// Posição no arranjo (`None`: mantém a atual).
    pub position: Option<(i32, i32)>,
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
        if let Some((x, y)) = self.position {
            args.push("--pos".into());
            args.push(format!("{x},{y}"));
        }
        args
    }

    /// Linha do `monitores.conf`.
    pub fn to_line(&self) -> String {
        let mode = self
            .mode
            .as_ref()
            .map(|m| format!(" modo={}x{}@{}", m.width, m.height, m.refresh))
            .unwrap_or_default();
        let position = self.position.map(|(x, y)| format!(" posicao={x},{y}")).unwrap_or_default();
        format!(
            "{}{mode} escala={} rotacao={}{position} ativo={}",
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
        let output = parts.next()?;
        // "principal=eDP-1" não é um monitor.
        if output.contains('=') {
            return None;
        }
        let mut setting = Setting {
            output: output.to_string(),
            enabled: true,
            mode: None,
            scale: 1.0,
            transform: "normal".into(),
            position: None,
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
                "posicao" => {
                    let (x, y) = value.split_once(',')?;
                    setting.position = Some((x.parse().ok()?, y.parse().ok()?));
                }
                _ => {}
            }
        }
        Some(setting)
    }
}

// ---------------------------------------------------------------------------
// Arranjo dos monitores (posição, número, principal)
// ---------------------------------------------------------------------------

/// Um retângulo do arranjo, em px lógicos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    fn overlaps(&self, o: &Rect) -> bool {
        self.x < o.x + o.w && o.x < self.x + self.w && self.y < o.y + o.h && o.y < self.y + self.h
    }
}

impl Output {
    /// Retângulo no arranjo.
    pub fn rect(&self) -> Rect {
        let (w, h) = self.logical_size();
        Rect { x: self.x, y: self.y, w: w as i32, h: h as i32 }
    }
}

/// Ordem dos monitores ligados para numerar (1, 2...): da esquerda para a
/// direita, de cima para baixo. Devolve os índices em `outputs`.
pub fn numbering(outputs: &[Output]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..outputs.len()).filter(|&i| outputs[i].enabled).collect();
    idx.sort_by_key(|&i| (outputs[i].x, outputs[i].y));
    idx
}

/// Monitor escolhido como principal no `monitores.conf` (`principal=eDP-1`).
pub fn configured_primary(config: &str) -> Option<String> {
    config.lines().find_map(|l| l.trim().strip_prefix("principal=").map(|n| n.trim().to_string()))
}

/// Monitor principal (barra e dock): o escolhido, se estiver ligado; senão o
/// primeiro da numeração.
pub fn primary<'a>(outputs: &'a [Output], config: &str) -> Option<&'a Output> {
    let chosen = configured_primary(config);
    outputs
        .iter()
        .find(|o| o.enabled && Some(&o.name) == chosen.as_ref())
        .or_else(|| numbering(outputs).first().map(|&i| &outputs[i]))
}

/// `monitores.conf` com outro monitor principal.
pub fn set_primary_in_config(config: &str, name: &str) -> String {
    let mut lines: Vec<&str> = config.lines().filter(|l| !l.trim().starts_with("principal=")).collect();
    if lines.is_empty() {
        lines.push("# Monitores do KalangOS (gravado pelas Configurações).");
    }
    let mut text = lines.join("\n");
    text.push_str(&format!("\nprincipal={name}\n"));
    text
}

/// Posição de um monitor arrastado no desenho do arranjo: gruda nas bordas
/// dos outros (até `snap` px de distância) e não fica por cima de nenhum
/// (empurra para o lado livre mais perto).
pub fn place(others: &[Rect], moved: Rect, snap: i32) -> (i32, i32) {
    let mut r = moved;
    // Encostar: bordas verticais e horizontais dos outros.
    let near = |a: i32, b: i32| (a - b).abs() <= snap;
    let mut best_x: Option<(i32, i32)> = None;
    let mut best_y: Option<(i32, i32)> = None;
    for o in others {
        for cand in [o.x + o.w, o.x - r.w, o.x, o.x + o.w - r.w] {
            let d = (cand - r.x).abs();
            if near(cand, r.x) && best_x.is_none_or(|(_, bd)| d < bd) {
                best_x = Some((cand, d));
            }
        }
        for cand in [o.y + o.h, o.y - r.h, o.y, o.y + o.h - r.h] {
            let d = (cand - r.y).abs();
            if near(cand, r.y) && best_y.is_none_or(|(_, bd)| d < bd) {
                best_y = Some((cand, d));
            }
        }
    }
    if let Some((x, _)) = best_x {
        r.x = x;
    }
    if let Some((y, _)) = best_y {
        r.y = y;
    }
    // Sem sobrepor: empurra para fora do monitor sobreposto, pelo lado com
    // o menor deslocamento (algumas voltas para resolver encadeamentos).
    for _ in 0..4 {
        let Some(o) = others.iter().find(|o| r.overlaps(o)) else { break };
        let moves = [
            (o.x + o.w - r.x, 0),
            (o.x - r.w - r.x, 0),
            (0, o.y + o.h - r.y),
            (0, o.y - r.h - r.y),
        ];
        let (dx, dy) = moves.into_iter().min_by_key(|(dx, dy)| dx.abs() + dy.abs()).unwrap_or((0, 0));
        r.x += dx;
        r.y += dy;
    }
    (r.x, r.y)
}

/// Desloca o arranjo para começar em (0, 0) (o labwc e os apps esperam).
pub fn normalize(positions: &mut [(i32, i32)]) {
    let min_x = positions.iter().map(|p| p.0).min().unwrap_or(0);
    let min_y = positions.iter().map(|p| p.1).min().unwrap_or(0);
    for p in positions.iter_mut() {
        p.0 -= min_x;
        p.1 -= min_y;
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
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("kalango/monitores.conf"))
}

/// Linha de um monitor no `monitores.conf` (`None` se não há).
pub fn config_line(text: &str, output: &str) -> Option<String> {
    text.lines().find(|l| Setting::from_line(l).is_some_and(|s| s.output == output)).map(str::to_string)
}

/// Troca a linha de um monitor pela antiga (`None` = tira a linha), sem
/// mexer no resto (principal, outros monitores).
pub fn restore_line(text: &str, output: &str, old: Option<&str>) -> String {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|l| Setting::from_line(l).is_none_or(|s| s.output != output))
        .map(str::to_string)
        .collect();
    if let Some(old) = old {
        lines.push(old.to_string());
    }
    lines.join("\n") + "\n"
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
        lines.insert(0, "# Monitores do KalangOS (gravado pelas Configurações).".into());
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

/// Reaplica o `monitores.conf` (início de sessão e monitor reconectado), só
/// nas saídas conectadas.
pub fn apply_saved() {
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
fn to_ui(outputs: &[Output], config: &str) -> Vec<DisplayOutput> {
    let order = numbering(outputs);
    let primary_name = primary(outputs, config).map(|o| o.name.clone());
    outputs
        .iter()
        .enumerate()
        .map(|(index, out)| {
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
                x: out.x,
                y: out.y,
                width_px: out.rect().w,
                height_px: out.rect().h,
                number: order.iter().position(|&i| i == index).map_or(0, |p| p as i32 + 1),
                primary: primary_name.as_deref() == Some(out.name.as_str()),
            }
        })
        .collect()
}

/// Estado do processo das Configurações: saídas lidas e mudança esperando
/// confirmação.
#[derive(Default)]
struct State {
    outputs: Vec<Output>,
    /// Como estava antes da última mudança (para voltar), um por monitor.
    previous: Vec<Setting>,
    /// O que foi aplicado e espera "Manter".
    pending: Vec<Setting>,
    /// Linhas do `monitores.conf` dos monitores mudados, como estavam antes
    /// (`None` = não havia), para voltar só elas se a contagem acabar sem
    /// "Manter" (sem desfazer o principal ou a posição gravados nesse meio
    /// tempo). A mudança já vai para o arquivo ao aplicar: fechar as
    /// Configurações durante a contagem mantém o que está na tela.
    previous_lines: Vec<(String, Option<String>)>,
    countdown: Option<Timer>,
    /// Posições arrastadas ainda não aplicadas.
    arranged: bool,
}

/// Área do desenho: os monitores ligados com folga de meio monitor em volta.
fn bounds(outputs: &[Output]) -> (i32, i32, i32, i32) {
    let rects: Vec<Rect> = outputs.iter().filter(|o| o.enabled).map(Output::rect).collect();
    if rects.is_empty() {
        return (0, 0, 1, 1);
    }
    let x0 = rects.iter().map(|r| r.x).min().unwrap_or(0);
    let y0 = rects.iter().map(|r| r.y).min().unwrap_or(0);
    let x1 = rects.iter().map(|r| r.x + r.w).max().unwrap_or(1);
    let y1 = rects.iter().map(|r| r.y + r.h).max().unwrap_or(1);
    let pad_x = rects.iter().map(|r| r.w).max().unwrap_or(0) / 2;
    let pad_y = rects.iter().map(|r| r.h).max().unwrap_or(0) / 2;
    (x0 - pad_x, y0 - pad_y, x1 - x0 + 2 * pad_x, y1 - y0 + 2 * pad_y)
}

fn read_config() -> String {
    config_path().and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default()
}

/// Mostra os monitores; `keep_bounds` mantém a área do desenho (arrastando).
fn show_list(ui: &AppWindow, state: &mut State, keep_bounds: bool) {
    let device = ui.global::<Device>();
    device.set_displays(ModelRc::from(Rc::new(VecModel::from(to_ui(&state.outputs, &read_config())))));
    if !keep_bounds {
        let (x, y, w, h) = bounds(&state.outputs);
        device.set_display_bounds_x(x);
        device.set_display_bounds_y(y);
        device.set_display_bounds_w(w.max(1));
        device.set_display_bounds_h(h.max(1));
    }
    device.set_display_arranged(state.arranged);
}

/// Mostra as saídas na UI.
fn show(ui: &AppWindow, state: &mut State, outputs: Option<Vec<Output>>) {
    let device = ui.global::<Device>();
    match outputs {
        Some(list) if !list.is_empty() => {
            state.outputs = list;
            state.arranged = false;
            device.set_displays_available(true);
            show_list(ui, state, false);
        }
        _ => device.set_displays_available(false),
    }
}

/// Como cada monitor estava e as linhas antigas do `monitores.conf` (para voltar).
type Previous = (Vec<Setting>, Vec<(String, Option<String>)>);

/// Aplica as configurações (uma por monitor) e grava no `monitores.conf`.
///
/// Devolve como cada monitor estava (para voltar) e as linhas antigas do
/// arquivo, ou `None` se o monitor recusou (aí já voltou tudo).
fn apply_and_save(device: &Device, s: &State, settings: &[Setting]) -> Option<Previous> {
    let previous: Vec<Setting> = settings
        .iter()
        .filter_map(|n| s.outputs.iter().find(|o| o.name == n.output).map(Output::current_setting))
        .collect();
    for setting in settings {
        if wlr_randr(&setting.args()).is_none() {
            for p in &previous {
                let _ = wlr_randr(&p.args());
            }
            device.set_display_message(
                tr("The display did not accept this configuration.", "O monitor não aceitou essa configuração.").into(),
            );
            return None;
        }
    }
    let mut old_lines = Vec::new();
    if let Some(path) = config_path() {
        let mut text = fs::read_to_string(&path).unwrap_or_default();
        for setting in settings {
            old_lines.push((setting.output.clone(), config_line(&text, &setting.output)));
            text = merge_config(&text, setting);
        }
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Err(err) = fs::write(&path, text) {
            eprintln!("kalango-shell: monitores.conf: {err}");
        }
    }
    Some((previous, old_lines))
}

/// Aplica configurações (uma por monitor) e começa a contagem de 15 s para
/// confirmar; sem "Manter", tudo volta sozinho.
fn apply_with_confirm(ui: &AppWindow, st: &Rc<RefCell<State>>, settings: Vec<Setting>) -> bool {
    let device = ui.global::<Device>();
    let mut s = st.borrow_mut();
    let Some((previous, old_lines)) = apply_and_save(&device, &s, &settings) else { return false };
    s.previous_lines = old_lines;
    s.previous = previous;
    s.pending = settings;
    s.arranged = false;
    device.set_display_message(SharedString::new());
    device.set_display_countdown(CONFIRM_SECONDS);
    device.set_display_confirm(true);

    let timer = Timer::default();
    let weak = ui.as_weak();
    let st2 = st.clone();
    timer.start(TimerMode::Repeated, Duration::from_secs(1), move || {
        let Some(ui) = weak.upgrade() else { return };
        let device = ui.global::<Device>();
        let left = device.get_display_countdown() - 1;
        device.set_display_countdown(left.max(0));
        if left <= 0 {
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
    if let Some(outputs) = read_outputs() {
        s.outputs = outputs;
    }
    show_list(ui, &mut s, false);
    true
}

/// Termina a confirmação: guarda a mudança (`keep`) ou volta ao que estava.
fn finish(ui: &AppWindow, state: &Rc<RefCell<State>>, keep: bool) {
    let mut st = state.borrow_mut();
    st.countdown = None;
    let device = ui.global::<Device>();
    device.set_display_confirm(false);
    st.pending.clear();
    let previous = std::mem::take(&mut st.previous);
    let old_lines = std::mem::take(&mut st.previous_lines);
    if keep {
        device.set_display_message(tr("Display configuration saved.", "Configuração do monitor salva.").into());
    } else {
        for setting in &previous {
            let _ = wlr_randr(&setting.args());
        }
        // Volta também as linhas desses monitores no arquivo (só elas).
        if let Some(path) = config_path() {
            let mut text = fs::read_to_string(&path).unwrap_or_default();
            for (output, old) in &old_lines {
                text = restore_line(&text, output, old.as_deref());
            }
            let _ = fs::write(&path, text);
        }
        device.set_display_message(
            tr("The display went back to the previous configuration.", "O monitor voltou à configuração anterior.").into(),
        );
    }
    let outputs = read_outputs();
    show(ui, &mut st, outputs);
}

pub fn install(ui: &AppWindow, mode: Mode) {
    let native = crate::shell::session::is_native();
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
        let (out, others_on) = {
            let s = st.borrow();
            let Some(out) = s.outputs.get(index as usize).cloned() else { return };
            let others_on = s.outputs.iter().any(|o| o.name != out.name && o.enabled);
            (out, others_on)
        };
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
            position: Some((out.x, out.y)),
        };
        if !enabled && !others_on {
            device.set_display_message(
                tr("At least one display has to stay on.", "Pelo menos um monitor precisa ficar ligado.").into(),
            );
            return;
        }
        apply_with_confirm(&ui, &st, vec![setting]);
    });

    // --- Arranjo: arrastar, aplicar, identificar, principal ---
    let weak = ui.as_weak();
    let st = state.clone();
    device.on_move_display(move |index, x, y| {
        let Some(ui) = weak.upgrade() else { return };
        let mut s = st.borrow_mut();
        let Some(out) = s.outputs.get(index as usize) else { return };
        let mut moved = out.rect();
        moved.x = x;
        moved.y = y;
        let others: Vec<Rect> = s
            .outputs
            .iter()
            .enumerate()
            .filter(|(i, o)| *i != index as usize && o.enabled)
            .map(|(_, o)| o.rect())
            .collect();
        // Gruda a até ~6% do monitor de distância.
        let snap = (moved.w.min(moved.h) / 16).max(20);
        let (nx, ny) = place(&others, moved, snap);
        if let Some(out) = s.outputs.get_mut(index as usize) {
            out.x = nx;
            out.y = ny;
        }
        s.arranged = true;
        show_list(&ui, &mut s, true);
    });

    let weak = ui.as_weak();
    let st = state.clone();
    device.on_apply_arrangement(move || {
        let Some(ui) = weak.upgrade() else { return };
        let settings = {
            let s = st.borrow();
            let on: Vec<&Output> = s.outputs.iter().filter(|o| o.enabled).collect();
            let mut positions: Vec<(i32, i32)> = on.iter().map(|o| (o.x, o.y)).collect();
            normalize(&mut positions);
            on.iter()
                .zip(positions)
                .map(|(o, p)| Setting { position: Some(p), ..o.current_setting() })
                .collect::<Vec<_>>()
        };
        // Posição não deixa a tela preta: aplica e grava já, sem contagem.
        let device = ui.global::<Device>();
        let mut s = st.borrow_mut();
        if apply_and_save(&device, &s, &settings).is_some() {
            s.arranged = false;
            device.set_display_message(tr("Position saved.", "Posição salva.").into());
        }
        let outputs = read_outputs();
        show(&ui, &mut s, outputs);
    });

    device.on_identify_displays(crate::shell::outputs::request_identify);

    let weak = ui.as_weak();
    let st = state.clone();
    device.on_set_primary_display(move |index| {
        let Some(ui) = weak.upgrade() else { return };
        let mut s = st.borrow_mut();
        let Some(name) = s.outputs.get(index as usize).map(|o| o.name.clone()) else { return };
        let device = ui.global::<Device>();
        let Some(path) = config_path() else { return };
        let text = fs::read_to_string(&path).unwrap_or_default();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if fs::write(&path, set_primary_in_config(&text, &name)).is_ok() {
            device.set_display_message(
                tr("The top bar and the dock move to this display.", "A barra e a dock vão para este monitor.").into(),
            );
        }
        show_list(&ui, &mut s, true);
    });

    // Monitor ligado ou desligado com a página aberta: atualiza a lista.
    let weak = ui.as_weak();
    let st = state.clone();
    let refresh = Timer::default();
    refresh.start(TimerMode::Repeated, Duration::from_secs(3), move || {
        let Some(ui) = weak.upgrade() else { return };
        let busy = { let s = st.borrow(); s.countdown.is_some() || s.arranged };
        if busy {
            return;
        }
        let Some(now) = read_outputs() else { return };
        let mut s = st.borrow_mut();
        let key = |list: &[Output]| list.iter().map(|o| (o.name.clone(), o.enabled, o.x, o.y, o.rect().w)).collect::<Vec<_>>();
        if key(&now) != key(&s.outputs) {
            show(&ui, &mut s, Some(now));
        }
    });
    std::mem::forget(refresh);

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
    fn arranjo_numero_principal_e_encaixe() {
        let mut outs = parse(QEMU);
        outs[1].enabled = true;
        outs[1].x = 1440;
        // Numeração da esquerda para a direita.
        assert_eq!(numbering(&outs), vec![0, 1]);
        assert_eq!(primary(&outs, "").unwrap().name, "Virtual-1");
        assert_eq!(primary(&outs, "principal=HDMI-A-1\n").unwrap().name, "HDMI-A-1");
        let conf = set_primary_in_config("# x\nVirtual-1 escala=1 rotacao=normal ativo=sim\nprincipal=A\n", "HDMI-A-1");
        assert_eq!(configured_primary(&conf).as_deref(), Some("HDMI-A-1"));
        assert_eq!(conf.matches("principal=").count(), 1);
        assert!(Setting::from_line("principal=HDMI-A-1").is_none());
        // Tamanho lógico com rotação e escala (1920×1080, 90°, 1,5 → 720×1280).
        assert_eq!(outs[1].logical_size(), (720, 1280));

        // Encaixe: solto perto da borda direita gruda nela.
        let a = Rect { x: 0, y: 0, w: 1366, h: 768 };
        assert_eq!(place(&[a], Rect { x: 1380, y: 20, w: 1920, h: 1080 }, 40), (1366, 0));
        // Por cima do outro: empurrado para o lado mais perto.
        assert_eq!(place(&[a], Rect { x: 1000, y: 0, w: 1920, h: 1080 }, 40), (1366, 0));
        // Embaixo.
        assert_eq!(place(&[a], Rect { x: 0, y: 700, w: 1366, h: 768 }, 40), (0, 768));
        let mut p = vec![(-1920, 100), (0, 0)];
        normalize(&mut p);
        assert_eq!(p, vec![(0, 100), (1920, 0)]);
    }

    #[test]
    fn volta_so_a_linha_do_monitor() {
        let text = "# x\neDP-1 escala=1 rotacao=normal posicao=2560,0 ativo=sim\nprincipal=HDMI-A-1\nHDMI-A-1 escala=2 rotacao=normal ativo=sim\n";
        let old = "HDMI-A-1 escala=1 rotacao=normal posicao=0,0 ativo=sim";
        let out = restore_line(text, "HDMI-A-1", Some(old));
        assert!(out.contains("principal=HDMI-A-1"));
        assert!(out.contains("eDP-1 escala=1 rotacao=normal posicao=2560,0"));
        assert!(out.contains(old) && !out.contains("escala=2"));
        assert!(!restore_line(text, "HDMI-A-1", None).contains("HDMI-A-1 escala"));
        assert_eq!(config_line(text, "eDP-1").as_deref(), Some("eDP-1 escala=1 rotacao=normal posicao=2560,0 ativo=sim"));
        assert_eq!(config_line(text, "DP-2"), None);
    }

    #[test]
    fn posicao_no_arquivo_e_no_comando() {
        let s = Setting::from_line("HDMI-A-1 escala=1 rotacao=normal posicao=1366,0 ativo=sim").unwrap();
        assert_eq!(s.position, Some((1366, 0)));
        assert!(s.to_line().contains(" posicao=1366,0 "));
        assert!(s.args().join(" ").ends_with("--pos 1366,0"));
    }

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
        assert_eq!(outs[1].scale_index(), 4); // 1,5 (depois de 50%, 75%, 100%, 125%)
    }

    #[test]
    fn monta_o_comando_e_o_arquivo() {
        let outs = parse(QEMU);
        let mut s = outs[0].current_setting();
        assert_eq!(
            s.args(),
            ["--output", "Virtual-1", "--on", "--mode", "1440x900@74.995003Hz", "--scale", "1", "--transform", "normal", "--pos", "0,0"]
        );
        s.scale = 1.25;
        s.transform = "270".into();
        let line = s.to_line();
        assert_eq!(line, "Virtual-1 modo=1440x900@74.995003 escala=1.25 rotacao=270 posicao=0,0 ativo=sim");
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
