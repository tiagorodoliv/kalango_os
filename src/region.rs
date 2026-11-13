//! Configurações → Idioma e região: idioma da sessão e layout do teclado.
//!
//! A escolha fica em `~/.config/kalango/regiao.conf` (variáveis de shell
//! `LANG`, `LANGUAGE`, `XKBLAYOUT`, `XKBVARIANT`), lido pelo `kalango-session`
//! (`/usr/local/lib/kalango/regiao.sh`) ao entrar. O idioma vale a partir do
//! próximo login (os programas leem o `LANG` ao abrir). O teclado muda na hora:
//! o shell grava as variáveis `XKB_DEFAULT_*` em `~/.config/labwc/environment`
//! e pede `labwc --reconfigure`, que relê esse arquivo e refaz o mapa de teclas.
//!
//! Os dois arquivos levam a marca [`MARK`]. Um `environment` do labwc feito à
//! mão (sem a marca) nunca é sobrescrito: aí o teclado só muda no próximo
//! login, pelo `regiao.conf`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::i18n::{tr, Lang};
use crate::{AppWindow, Device, Mode};

/// Marca dos arquivos gerados pelo shell.
pub const MARK: &str = "# Gerado pelo kalango-shell (Configurações → Idioma e região).";

/// Idiomas oferecidos: (locale, `LANGUAGE`, nome no próprio idioma).
pub const LANGUAGES: [(&str, &str, &str); 2] =
    [("en_US.UTF-8", "", "English"), ("pt_BR.UTF-8", "pt_BR:pt", "Português (Brasil)")];

/// Layouts de teclado oferecidos: (layout XKB, variante, nome em inglês, nome em português).
pub const LAYOUTS: [(&str, &str, &str, &str); 10] = [
    ("us", "", "English (US)", "Inglês (EUA)"),
    ("us", "intl", "English (US, international)", "Inglês (EUA, internacional)"),
    ("gb", "", "English (UK)", "Inglês (Reino Unido)"),
    ("br", "", "Portuguese (Brazil, ABNT2)", "Português (Brasil, ABNT2)"),
    ("pt", "", "Portuguese (Portugal)", "Português (Portugal)"),
    ("es", "", "Spanish (Spain)", "Espanhol (Espanha)"),
    ("latam", "", "Spanish (Latin America)", "Espanhol (América Latina)"),
    ("fr", "", "French", "Francês"),
    ("de", "", "German", "Alemão"),
    ("it", "", "Italian", "Italiano"),
];

/// Escolha de idioma e teclado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// Valor de `LANG` (ex.: `en_US.UTF-8`).
    pub lang: String,
    /// Layout XKB (ex.: `us`, `br`).
    pub layout: String,
    /// Variante XKB (vazia = padrão do layout).
    pub variant: String,
}

impl Region {
    /// Índice do idioma em [`LANGUAGES`] (pelo idioma, ignorando a região).
    pub fn language_index(&self) -> usize {
        match Lang::from_locale(&self.lang) {
            Lang::Portuguese => 1,
            Lang::English => 0,
        }
    }

    /// Índice do layout em [`LAYOUTS`] (`None` se não está na lista).
    pub fn layout_index(&self) -> Option<usize> {
        LAYOUTS.iter().position(|(l, v, ..)| *l == self.layout && *v == self.variant)
    }

    /// Lê um `regiao.conf` (ou `/etc/default/keyboard`), partindo de `base`.
    pub fn parse(text: &str, base: &Region) -> Region {
        let mut r = base.clone();
        for line in text.lines() {
            let Some((key, value)) = line.trim().split_once('=') else { continue };
            let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
            match key.trim() {
                "LANG" if !value.is_empty() => r.lang = value,
                "XKBLAYOUT" if !value.is_empty() => {
                    // "us,br" = vários layouts: o primeiro é o ativo.
                    r.layout = value.split(',').next().unwrap_or_default().to_string();
                }
                "XKBVARIANT" => r.variant = value.split(',').next().unwrap_or_default().to_string(),
                _ => {}
            }
        }
        r
    }

    /// Texto do `regiao.conf`.
    pub fn to_conf(&self) -> String {
        let language = LANGUAGES[self.language_index()].1;
        format!(
            "{MARK}\nLANG=\"{}\"\nLANGUAGE=\"{language}\"\nXKBLAYOUT=\"{}\"\nXKBVARIANT=\"{}\"\n",
            self.lang, self.layout, self.variant
        )
    }

    /// Texto do `~/.config/labwc/environment` (só o teclado). A variante vai
    /// sempre, mesmo vazia: ao reler o arquivo, o labwc não apaga uma
    /// variável que sumiu dele, e a variante do layout anterior ficaria.
    pub fn to_labwc_env(&self) -> String {
        format!("{MARK}\nXKB_DEFAULT_LAYOUT={}\nXKB_DEFAULT_VARIANT={}\n", self.layout, self.variant)
    }
}

/// `~/.config` (ou `$XDG_CONFIG_HOME`).
fn config_home() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
}

fn conf_path() -> Option<PathBuf> {
    Some(config_home()?.join("kalango/regiao.conf"))
}

fn labwc_env_path() -> Option<PathBuf> {
    Some(config_home()?.join("labwc/environment"))
}

/// Escolha atual: a da sessão (variáveis de ambiente), com o `regiao.conf`
/// por cima (pode ter mudado depois de entrar).
pub fn current() -> Region {
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let base = Region {
        lang: env("LANG").unwrap_or_else(|| "en_US.UTF-8".into()),
        layout: env("XKB_DEFAULT_LAYOUT")
            .map(|l| l.split(',').next().unwrap_or_default().to_string())
            .unwrap_or_else(|| "us".into()),
        variant: env("XKB_DEFAULT_VARIANT").unwrap_or_default(),
    };
    match conf_path().and_then(|p| fs::read_to_string(p).ok()) {
        Some(text) => Region::parse(&text, &base),
        None => base,
    }
}

/// Grava a escolha no `regiao.conf`.
fn save(region: &Region) -> std::io::Result<()> {
    let path = conf_path().ok_or_else(|| std::io::Error::other("sem HOME"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, region.to_conf())
}

/// Troca o teclado na sessão labwc em andamento. `false` se não deu (fora do
/// labwc ou `environment` feito à mão).
fn apply_keyboard(region: &Region) -> bool {
    if std::env::var_os("LABWC_PID").is_none() {
        return false;
    }
    let Some(path) = labwc_env_path() else { return false };
    if fs::read_to_string(&path).is_ok_and(|t| !t.contains(MARK)) {
        eprintln!("kalango-shell: {} foi feito à mão; o teclado muda no próximo login", path.display());
        return false;
    }
    let written = path
        .parent()
        .map_or(Ok(()), fs::create_dir_all)
        .and_then(|_| fs::write(&path, region.to_labwc_env()));
    if let Err(err) = written {
        eprintln!("kalango-shell: falha ao gravar {}: {err}", path.display());
        return false;
    }
    Command::new("labwc")
        .arg("--reconfigure")
        .stdin(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Nomes dos layouts no idioma da interface.
fn layout_names() -> Vec<SharedString> {
    LAYOUTS.iter().map(|(.., en, pt)| tr(en, pt).into()).collect()
}

/// Liga a página Idioma e região ao global `Device`. Só as Configurações da
/// sessão real gravam a escolha; no protótipo a página só muda a seleção.
pub fn install(ui: &AppWindow, mode: Mode) {
    if !matches!(mode, Mode::Settings | Mode::Desktop) {
        return;
    }
    let native = crate::session::is_native();
    let d = ui.global::<Device>();
    let region = current();
    let languages: Vec<SharedString> = LANGUAGES.iter().map(|(.., name)| (*name).into()).collect();
    d.set_languages(ModelRc::from(Rc::new(VecModel::from(languages))));
    d.set_language_index(region.language_index() as i32);
    d.set_keyboard_layouts(ModelRc::from(Rc::new(VecModel::from(layout_names()))));
    d.set_keyboard_index(region.layout_index().map_or(-1, |i| i as i32));
    // O idioma da sessão é o que estava no ambiente ao abrir: se a escolha
    // salva for outra, falta sair e entrar de novo.
    let session_lang = crate::i18n::lang();
    d.set_language_pending(Lang::from_locale(&region.lang) != session_lang);

    let weak = ui.as_weak();
    d.on_set_language(move |i| {
        let Some(&(lang, ..)) = LANGUAGES.get(i as usize) else { return };
        let mut region = current();
        region.lang = lang.to_string();
        let Some(ui) = weak.upgrade() else { return };
        let d = ui.global::<Device>();
        if !native {
            d.set_language_index(i);
            return;
        }
        match save(&region) {
            Ok(()) => {
                d.set_language_index(i);
                d.set_language_pending(Lang::from_locale(lang) != session_lang);
            }
            Err(err) => eprintln!("kalango-shell: falha ao gravar regiao.conf: {err}"),
        }
    });

    let weak = ui.as_weak();
    d.on_set_keyboard(move |i| {
        let Some(&(layout, variant, ..)) = LAYOUTS.get(i as usize) else { return };
        let mut region = current();
        region.layout = layout.to_string();
        region.variant = variant.to_string();
        let Some(ui) = weak.upgrade() else { return };
        let d = ui.global::<Device>();
        if !native {
            d.set_keyboard_index(i);
            return;
        }
        if let Err(err) = save(&region) {
            eprintln!("kalango-shell: falha ao gravar regiao.conf: {err}");
            return;
        }
        d.set_keyboard_index(i);
        let message = if apply_keyboard(&region) {
            tr("Keyboard changed.", "Teclado trocado.")
        } else {
            tr(
                "The new keyboard will be used the next time you sign in.",
                "O teclado novo vale a partir do próximo login.",
            )
        };
        d.set_keyboard_message(message.into());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn padrao() -> Region {
        Region { lang: "en_US.UTF-8".into(), layout: "us".into(), variant: String::new() }
    }

    #[test]
    fn le_o_arquivo_e_o_keyboard_do_debian() {
        let r = Region::parse("LANG=\"pt_BR.UTF-8\"\nXKBLAYOUT=\"br\"\nXKBVARIANT=\"\"\n", &padrao());
        assert_eq!(r.language_index(), 1);
        assert_eq!(r.layout_index(), Some(3));
        // /etc/default/keyboard com dois layouts: vale o primeiro.
        let r = Region::parse("XKBMODEL=\"pc105\"\nXKBLAYOUT=\"us,br\"\nXKBVARIANT=\"intl,\"\n", &padrao());
        assert_eq!((r.layout.as_str(), r.variant.as_str()), ("us", "intl"));
        assert_eq!(r.layout_index(), Some(1));
        assert_eq!(Region::parse("lixo\n", &padrao()), padrao());
    }

    #[test]
    fn grava_e_le_de_volta() {
        let r = Region { lang: "pt_BR.UTF-8".into(), layout: "us".into(), variant: "intl".into() };
        let text = r.to_conf();
        assert!(text.starts_with(MARK));
        assert!(text.contains("LANGUAGE=\"pt_BR:pt\""));
        assert_eq!(Region::parse(&text, &padrao()), r);
        let env = r.to_labwc_env();
        assert!(env.contains("XKB_DEFAULT_LAYOUT=us\n") && env.contains("XKB_DEFAULT_VARIANT=intl\n"));
        assert!(padrao().to_labwc_env().ends_with("XKB_DEFAULT_VARIANT=\n"));
    }
}
