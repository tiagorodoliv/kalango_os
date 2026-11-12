//! Busca do launcher: filtra o catálogo de apps pelo texto digitado.
//!
//! A busca ignora maiúsculas e acentos ("configuracoes" acha "Configurações",
//! "MUSICA" acha "Música"), o que o Slint não faz sozinho — por isso fica no Rust.

use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, VecModel};

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

/// Diz se um app combina com a busca (nome ou id contêm o texto).
pub fn matches(name: &str, id: &str, query: &str) -> bool {
    let q = normalize(query.trim());
    q.is_empty() || normalize(name).contains(&q) || id.contains(&q)
}

/// Liga o callback `Shell.filter-apps` ao catálogo `Apps.pinned`.
pub fn install(ui: &AppWindow) {
    let weak = ui.as_weak();
    ui.global::<Shell>().on_filter_apps(move |query| {
        let Some(ui) = weak.upgrade() else {
            return ModelRc::default();
        };
        let all = ui.global::<Apps>().get_pinned();
        let found: Vec<AppEntry> = all
            .iter()
            .filter(|app| matches(&app.name, &app.id, &query))
            .collect();
        ModelRc::from(Rc::new(VecModel::from(found)))
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
    }
}
