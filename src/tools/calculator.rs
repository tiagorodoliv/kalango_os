//! Ligação da Calculadora (`kalango-shell --calculadora`) com a UI
//! (`ui/screens/calculator.slint`, global `Calc`).
//!
//! O histórico (últimas 50 contas) fica em
//! `~/.local/state/kalango/calculadora.txt` ("expressão<TAB>resultado").

use std::cell::Cell;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::common::i18n::tr;
use crate::tools::{codec, expr};
use crate::{AppWindow, Calc, CalcHistory};

/// Quantas contas o histórico guarda.
const HISTORY_MAX: usize = 50;

fn history_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/state")))?;
    Some(base.join("kalango/calculadora.txt"))
}

/// Linhas "expressão<TAB>resultado" → histórico (mais recente primeiro).
pub fn parse_history(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(e, r)| (e.to_string(), r.to_string()))
        .take(HISTORY_MAX)
        .collect()
}

fn save_history(items: &[(String, String)]) {
    let Some(path) = history_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let text: String = items
        .iter()
        .map(|(e, r)| format!("{}\t{}\n", e.replace(['\t', '\n'], " "), r))
        .collect();
    let _ = fs::write(path, text);
}

fn show_history(calc: &Calc, items: &[(String, String)]) {
    let rows: Vec<CalcHistory> =
        items.iter().map(|(e, r)| CalcHistory { expr: e.into(), result: r.into() }).collect();
    calc.set_history(ModelRc::from(Rc::new(VecModel::from(rows))));
}

/// Resultado de uma ferramenta: texto e se é erro.
fn run_tool(tool: i32, action: i32, input: &str) -> (String, bool) {
    let ok = |s: String| (s, false);
    let fail = |s: String| (s, true);
    match (tool, action) {
        (0, 0) => ok(codec::base64_encode(input.as_bytes())),
        (0, _) => match codec::base64_decode(input) {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => ok(text),
                Err(e) => ok(format!(
                    "{}\n{}",
                    tr("(binary data, in hex:)", "(dados binários, em hexa:)"),
                    e.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(" ")
                )),
            },
            Err(e) => fail(e),
        },
        (1, 0) => ok(codec::url_encode(input)),
        (1, _) => codec::url_decode(input).map_or_else(fail, ok),
        (2, _) => {
            let b = input.as_bytes();
            ok(format!("MD5      {}\nSHA-1    {}\nSHA-256  {}", codec::md5(b), codec::sha1(b), codec::sha256(b)))
        }
        (3, 0) => codec::timestamp(input).map_or_else(fail, ok),
        (3, _) => codec::timestamp("").map_or_else(fail, ok),
        (4, _) => codec::color(input).map(|(t, _)| t).map_or_else(fail, ok),
        (5, _) => ok((0..5).map(|_| codec::uuid_v4()).collect::<Vec<_>>().join("\n")),
        (6, a) => codec::json_format(input, if a == 0 { 2 } else { 0 }).map_or_else(fail, ok),
        (7, 0) => ok(format!(
            "{}: {}\n{}: {}\n{}: {}\n{}: {}",
            tr("Characters", "Caracteres"),
            input.chars().count(),
            tr("Words", "Palavras"),
            input.split_whitespace().count(),
            tr("Lines", "Linhas"),
            if input.is_empty() { 0 } else { input.lines().count() },
            "Bytes (UTF-8)",
            input.len()
        )),
        (7, _) => ok(if input.chars().any(char::is_lowercase) { input.to_uppercase() } else { input.to_lowercase() }),
        _ => (String::new(), false),
    }
}

/// Liga a Calculadora (modo `--calculadora`).
pub fn install(ui: &AppWindow) {
    let calc = ui.global::<Calc>();
    let ans = Rc::new(Cell::new(0.0f64));
    let ans_int = Rc::new(Cell::new(0i128));
    let history = Rc::new(std::cell::RefCell::new(
        history_path().and_then(|p| fs::read_to_string(p).ok()).map(|t| parse_history(&t)).unwrap_or_default(),
    ));
    if let Some((_, last)) = history.borrow().first() {
        ans.set(expr::eval(last, 0.0, false).unwrap_or(0.0));
    }
    show_history(&calc, &history.borrow());

    // --- Calculadora ---
    let weak = ui.as_weak();
    let a = ans.clone();
    calc.on_changed(move || {
        let Some(ui) = weak.upgrade() else { return };
        let calc = ui.global::<Calc>();
        match expr::eval(&calc.get_expr(), a.get(), calc.get_degrees()) {
            Ok(v) => {
                calc.set_preview(expr::format_f(v).into());
                calc.set_preview_error(false);
            }
            Err(e) => {
                calc.set_preview(e.into());
                calc.set_preview_error(true);
            }
        }
    });
    let weak = ui.as_weak();
    let (a, h) = (ans.clone(), history.clone());
    calc.on_commit(move || {
        let Some(ui) = weak.upgrade() else { return };
        let calc = ui.global::<Calc>();
        let text = calc.get_expr().to_string();
        let Ok(v) = expr::eval(&text, a.get(), calc.get_degrees()) else { return };
        let result = expr::format_f(v);
        a.set(v);
        {
            let mut items = h.borrow_mut();
            items.insert(0, (text.trim().to_string(), result.clone()));
            items.truncate(HISTORY_MAX);
            save_history(&items);
            show_history(&calc, &items);
        }
        calc.set_expr(result.into());
        calc.set_preview(SharedString::new());
        calc.set_preview_error(false);
    });
    let weak = ui.as_weak();
    let h = history.clone();
    calc.on_clear_history(move || {
        let Some(ui) = weak.upgrade() else { return };
        h.borrow_mut().clear();
        save_history(&[]);
        show_history(&ui.global::<Calc>(), &[]);
    });
    let weak = ui.as_weak();
    calc.on_backspace(move || {
        let Some(ui) = weak.upgrade() else { return };
        let calc = ui.global::<Calc>();
        let mut text = calc.get_expr().to_string();
        text.pop();
        calc.set_expr(text.into());
        calc.invoke_changed();
    });

    // --- Programador ---
    let weak = ui.as_weak();
    let ai = ans_int.clone();
    calc.on_prog_changed(move || {
        let Some(ui) = weak.upgrade() else { return };
        let calc = ui.global::<Calc>();
        let bits = calc.get_bits().clamp(8, 64) as u32;
        let text = calc.get_prog_expr().to_string();
        let value = if text.trim().is_empty() { Ok(0) } else { expr::eval_int(&text, ai.get()) };
        match value {
            Ok(v) => {
                ai.set(v);
                let (raw, shown) = expr::wrap(v, bits, calc.get_signed());
                calc.set_hex(expr::grouped(raw, 16, bits).into());
                calc.set_dec(shown.to_string().into());
                calc.set_oct(expr::grouped(raw, 8, bits).into());
                calc.set_bin(expr::grouped(raw, 2, bits).into());
                let low = (raw & 0xff) as u8;
                calc.set_ascii(if (0x20..0x7f).contains(&low) { format!("'{}'", low as char) } else { String::new() }.into());
                calc.set_prog_error(SharedString::new());
                let bit_values: Vec<bool> = (0..bits).map(|i| raw >> i & 1 == 1).collect();
                calc.set_bit_values(ModelRc::from(Rc::new(VecModel::from(bit_values))));
            }
            Err(e) => calc.set_prog_error(e.into()),
        }
    });
    let weak = ui.as_weak();
    let ai = ans_int.clone();
    calc.on_toggle_bit(move |bit| {
        let Some(ui) = weak.upgrade() else { return };
        let calc = ui.global::<Calc>();
        let bits = calc.get_bits().clamp(8, 64) as u32;
        if !(0..bits as i32).contains(&bit) {
            return;
        }
        let (raw, _) = expr::wrap(ai.get(), bits, false);
        calc.set_prog_expr(format!("0x{:X}", raw ^ (1u64 << bit)).into());
        calc.invoke_prog_changed();
    });
    calc.invoke_prog_changed();

    // --- Ferramentas ---
    let weak = ui.as_weak();
    calc.on_run_tool(move |action| {
        let Some(ui) = weak.upgrade() else { return };
        let calc = ui.global::<Calc>();
        let tool = calc.get_tool();
        let input = calc.get_tool_input().to_string();
        let (text, error) = run_tool(tool, action, &input);
        calc.set_tool_output(text.into());
        calc.set_tool_error(error);
        if tool == 4 {
            match codec::color(&input) {
                Ok((_, [r, g, b])) => {
                    calc.set_tool_color(slint::Color::from_rgb_u8(r, g, b));
                    calc.set_tool_has_color(true);
                }
                Err(_) => calc.set_tool_has_color(false),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn historico_e_ferramentas() {
        let h = parse_history("2+2\t4\nlixo\nsqrt(9)\t3\n");
        assert_eq!(h, vec![("2+2".into(), "4".into()), ("sqrt(9)".into(), "3".into())]);
        assert_eq!(run_tool(0, 0, "olá"), ("b2zDoQ==".into(), false));
        assert_eq!(run_tool(0, 1, "b2zDoQ=="), ("olá".into(), false));
        assert!(run_tool(0, 1, "@@").1);
        assert!(run_tool(2, 0, "abc").0.contains("ba7816bf8f01cfea414140de5dae2223"));
        assert_eq!(run_tool(5, 0, "").0.lines().count(), 5);
        assert!(run_tool(6, 0, "{\"a\":1").1);
        assert!(run_tool(7, 0, "um dois\ntrês").0.contains(": 3"));
        assert_eq!(run_tool(7, 1, "Abc").0, "ABC");
        assert_eq!(run_tool(7, 1, "ABC").0, "abc");
    }
}
