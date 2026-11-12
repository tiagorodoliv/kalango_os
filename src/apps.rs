//! Busca do launcher e a lista "Todos os apps".
//!
//! A busca ignora maiúsculas e acentos ("configuracoes" acha "Configurações",
//! "MUSICA" acha "Música"), o que o Slint não faz sozinho — por isso fica no Rust.
//! Ela procura primeiro nos fixados e depois nos apps instalados no sistema
//! (`Apps.all`, lidos dos `.desktop` por [`crate::desktop`]), sem repetir os
//! que já são fixados.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;

thread_local! {
    /// Ids de `Apps.all` que repetem um app fixado (ex.: o "Thunar", que já é
    /// "Arquivos"): ficam fora dos resultados da busca. Só a thread da UI usa.
    static DUPLICATES: RefCell<HashSet<SharedString>> = RefCell::default();
}

use slint::{Color, ComponentHandle, Image, Model, ModelRc, SharedString, VecModel};

use crate::desktop::{self, DesktopApp};
use crate::{AppEntry, AppWindow, Apps, Shell};

/// Normaliza para comparação: minúsculas e sem acentos do português.
pub fn normalize(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            other => other,
        })
        .collect()
}

/// Diz se um app combina com a busca: o nome ou o texto extra (id, nome
/// genérico, palavras-chave) contêm o que foi digitado.
pub fn matches(name: &str, extra: &str, query: &str) -> bool {
    let q = normalize(query.trim());
    q.is_empty() || normalize(name).contains(&q) || normalize(extra).contains(&q)
}

/// Programas dos apps fixados: os `.desktop` desses programas não se repetem
/// nos resultados da busca (ex.: "Thunar" já aparece como "Arquivos").
fn pinned_programs(ids: impl Iterator<Item = String>) -> HashSet<String> {
    ids.filter_map(|id| crate::session::command_for(&id))
        .flat_map(|argv| {
            let first = argv.first().cloned();
            let last = argv.last().cloned();
            first.into_iter().chain(last)
        })
        .collect()
}

/// Cores das iniciais dos apps sem ícone (as cores de destaque do tema).
const LETTER_TINTS: [u32; 6] = [0x3dd6f5, 0xa78bfa, 0x7ea6f7, 0x5fd38d, 0xe8b04b, 0xf28b95];

/// Inicial maiúscula do nome (para o app sem ícone).
pub fn initial(name: &str) -> String {
    name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default()
}

/// Converte um app do sistema para o item do menu. Com ícone, `tint` fica
/// transparente (o ícone já é colorido); sem ícone, o menu mostra a inicial
/// numa cor fixa por app.
fn to_entry(app: &DesktopApp, icon: Option<&PathBuf>) -> AppEntry {
    let image = icon.and_then(|p| Image::load_from_path(p).ok());
    let tint = if image.is_some() {
        Color::from_argb_u8(0, 0, 0, 0)
    } else {
        let sum: u32 = app.name.bytes().map(u32::from).sum();
        Color::from_argb_encoded(0xff00_0000 | LETTER_TINTS[sum as usize % LETTER_TINTS.len()])
    };
    AppEntry {
        id: app.id().into(),
        name: app.name.clone().into(),
        icon: image.clone().unwrap_or_default(),
        tint,
        letter: if image.is_some() { SharedString::new() } else { initial(&app.name).into() },
        keywords: app.keywords.clone().into(),
    }
}

/// Liga o callback `Shell.filter-apps` e, no menu da sessão real, preenche
/// `Apps.all` com os apps instalados.
pub fn install(ui: &AppWindow, load_installed: bool) {
    let weak = ui.as_weak();
    ui.global::<Shell>().on_filter_apps(move |query| {
        let Some(ui) = weak.upgrade() else {
            return ModelRc::default();
        };
        let apps = ui.global::<Apps>();
        let pinned = apps.get_pinned();
        let pinned_ids: HashSet<SharedString> = pinned.iter().map(|a| a.id).collect();
        let found: Vec<AppEntry> = DUPLICATES.with_borrow(|dup| {
            pinned
                .iter()
                .filter(|app| matches(&app.name, &app.id, &query))
                .chain(apps.get_all().iter().filter(|app| {
                    !pinned_ids.contains(&app.id)
                        && !dup.contains(&app.id)
                        && matches(&app.name, &app.keywords, &query)
                }))
                .collect()
        });
        ModelRc::from(Rc::new(VecModel::from(found)))
    });

    if !load_installed {
        return;
    }
    // Ler e conferir dezenas de arquivos fica numa thread para o menu abrir
    // na hora; as imagens (que não são `Send`) são criadas no laço da UI.
    let programs = pinned_programs(ui.global::<Apps>().get_pinned().iter().map(|a| a.id.into()));
    let weak = ui.as_weak();
    std::thread::spawn(move || {
        let dirs = desktop::data_dirs();
        let found: Vec<(DesktopApp, Option<PathBuf>)> = desktop::scan(&dirs)
            .into_iter()
            .map(|app| {
                let icon = desktop::find_icon(&app.icon, &dirs);
                (app, icon)
            })
            .collect();
        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            let entries: Vec<AppEntry> = DUPLICATES.with_borrow_mut(|dup| {
                found
                    .iter()
                    .map(|(app, icon)| {
                        if programs.contains(app.program()) {
                            dup.insert(app.id().into());
                        }
                        to_entry(app, icon.as_ref())
                    })
                    .collect()
            });
            ui.global::<Apps>()
                .set_all(ModelRc::from(Rc::new(VecModel::from(entries))));
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignora_acentos_e_maiusculas() {
        assert!(matches("Configurações", "settings", "configuracoes"));
        assert!(matches("Música", "music", "MUSICA"));
        assert!(matches("Atualizações", "updates", "  atual "));
    }

    #[test]
    fn acha_pelo_id_e_rejeita_o_resto() {
        assert!(matches("Navegador", "browser", "brow"));
        assert!(!matches("Loja", "store", "terminal"));
        assert!(!matches("GIMP", " Editor de Imagens gimp", "edição"));
        assert!(matches("GIMP", " Editor de Imagens gimp", "editor"));
    }

    #[test]
    fn programas_dos_fixados() {
        let p = pinned_programs(["files", "monitor", "browser"].iter().map(|s| s.to_string()));
        assert!(p.contains("thunar"));
        assert!(p.contains("htop"));
        assert!(p.contains("firefox-esr"));
        assert!(!p.contains("gimp"));
    }

    #[test]
    fn inicial_maiuscula() {
        assert_eq!(initial("épico"), "É");
        assert_eq!(initial(""), "");
    }
}
