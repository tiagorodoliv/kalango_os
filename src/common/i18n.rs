//! Idioma da interface.
//!
//! O inglês é o idioma padrão do KalangOS; o português do Brasil é uma
//! tradução. Os textos dos `.slint` usam `@tr("...")` e a tradução fica em
//! `lang/pt_BR/LC_MESSAGES/kalango_os.po`, embutida no binário pelo `build.rs`.
//! Os poucos textos montados no Rust (relógio, avisos, status de rede...)
//! usam [`tr`], que segue a mesma escolha de idioma.
//!
//! O idioma vem do ambiente, como em qualquer programa Linux: a primeira
//! variável não vazia entre `LC_ALL`, `LC_MESSAGES` e `LANG`. Começando com
//! `pt` (ex.: `pt_BR.UTF-8`), português; qualquer outra coisa, inglês.

use std::sync::OnceLock;

/// Idiomas com tradução.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Inglês (padrão).
    English,
    /// Português do Brasil.
    Portuguese,
}

impl Lang {
    /// Idioma a partir de um nome de locale (`pt_BR.UTF-8`, `en_US`, `C`...).
    pub fn from_locale(locale: &str) -> Lang {
        if locale.to_ascii_lowercase().starts_with("pt") {
            Lang::Portuguese
        } else {
            Lang::English
        }
    }

    /// Nome da tradução embutida do Slint (`""` = textos originais, em inglês).
    pub fn slint_name(self) -> &'static str {
        match self {
            Lang::English => "",
            Lang::Portuguese => "pt_BR",
        }
    }
}

/// Locale da interface, lido do ambiente (`LC_ALL`, `LC_MESSAGES`, `LANG`).
fn env_locale() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .unwrap_or_default()
}

/// Idioma desta execução (lido uma vez).
pub fn lang() -> Lang {
    static LANG: OnceLock<Lang> = OnceLock::new();
    *LANG.get_or_init(|| Lang::from_locale(&env_locale()))
}

/// Escolhe o texto do idioma atual: `tr("Connected", "Conectado")`.
pub fn tr<'a>(en: &'a str, pt: &'a str) -> &'a str {
    match lang() {
        Lang::English => en,
        Lang::Portuguese => pt,
    }
}

/// Aplica na interface o idioma do ambiente. Chamar depois de criar a
/// primeira janela (exigência do Slint).
pub fn apply() {
    if let Err(err) = slint::select_bundled_translation(lang().slint_name()) {
        eprintln!("kalango-shell: tradução indisponível: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idioma_pelo_locale() {
        assert_eq!(Lang::from_locale("pt_BR.UTF-8"), Lang::Portuguese);
        assert_eq!(Lang::from_locale("pt_PT"), Lang::Portuguese);
        assert_eq!(Lang::from_locale("en_US.UTF-8"), Lang::English);
        assert_eq!(Lang::from_locale("C.UTF-8"), Lang::English);
        assert_eq!(Lang::from_locale(""), Lang::English);
    }
}
