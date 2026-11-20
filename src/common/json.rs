//! Leitor de JSON pequeno, sem dependências (para a saída do `pw-dump`).
//!
//! O projeto evita crates novas (cada uma é código de terceiros rodando no
//! computador de quem usa); a Calculadora já tem um formatador próprio
//! (`tools::codec::json_format`), mas ele não monta uma árvore. Este monta
//! [`Value`] e tem os acessos que o som precisa (`get`, `index`, `as_*`).
//! Números viram `f64`; textos com escapes `\uXXXX` são decodificados.

use std::collections::BTreeMap;

/// Um valor JSON.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

/// `Value::Null` para os acessos que não acham nada.
static NULL: Value = Value::Null;

impl Value {
    /// Campo de um objeto (`Null` se não for objeto ou não tiver o campo).
    pub fn get(&self, key: &str) -> &Value {
        match self {
            Value::Object(m) => m.get(key).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    /// Item de uma lista (`Null` fora dela).
    pub fn index(&self, i: usize) -> &Value {
        match self {
            Value::Array(a) => a.get(i).unwrap_or(&NULL),
            _ => &NULL,
        }
    }

    /// Itens da lista (vazio se não for lista).
    pub fn items(&self) -> &[Value] {
        match self {
            Value::Array(a) => a,
            _ => &[],
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// Número inteiro (também aceita texto com número, como `"15"` nas
    /// propriedades do PipeWire).
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Number(n) if n.fract() == 0.0 => Some(*n as i64),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }
}

/// Lê um texto JSON inteiro.
pub fn parse(text: &str) -> Result<Value, String> {
    let mut p = Parser { s: text.as_bytes(), i: 0 };
    p.ws();
    let v = p.value(0)?;
    p.ws();
    if p.i != p.s.len() {
        return Err(format!("texto sobrando na posição {}", p.i));
    }
    Ok(v)
}

/// Profundidade máxima (listas e objetos dentro de outros): protege a pilha
/// contra entrada maliciosa ou quebrada.
const MAX_DEPTH: usize = 128;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\n' | b'\r' | b'\t') {
            self.i += 1;
        }
    }

    fn err<T>(&self, msg: &str) -> Result<T, String> {
        Err(format!("{msg} na posição {}", self.i))
    }

    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > MAX_DEPTH {
            return self.err("JSON fundo demais");
        }
        match self.s.get(self.i) {
            Some(b'{') => self.object(depth),
            Some(b'[') => self.array(depth),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.word("true", Value::Bool(true)),
            Some(b'f') => self.word("false", Value::Bool(false)),
            Some(b'n') => self.word("null", Value::Null),
            Some(c) if *c == b'-' || c.is_ascii_digit() => self.number(),
            _ => self.err("esperava um valor"),
        }
    }

    fn word(&mut self, w: &str, v: Value) -> Result<Value, String> {
        if self.s[self.i..].starts_with(w.as_bytes()) {
            self.i += w.len();
            Ok(v)
        } else {
            self.err("palavra inválida")
        }
    }

    fn number(&mut self) -> Result<Value, String> {
        let start = self.i;
        while self.i < self.s.len() && matches!(self.s[self.i], b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E') {
            self.i += 1;
        }
        let txt = std::str::from_utf8(&self.s[start..self.i]).unwrap_or("");
        txt.parse::<f64>().map(Value::Number).or_else(|_| self.err("número inválido"))
    }

    fn string(&mut self) -> Result<String, String> {
        self.i += 1; // aspas de abertura
        let mut out: Vec<u8> = Vec::new();
        loop {
            let Some(&c) = self.s.get(self.i) else { return self.err("texto sem fim") };
            self.i += 1;
            match c {
                b'"' => break,
                b'\\' => {
                    let Some(&e) = self.s.get(self.i) else { return self.err("escape sem fim") };
                    self.i += 1;
                    match e {
                        b'"' | b'\\' | b'/' => out.push(e),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'u' => {
                            let mut code = self.hex4()?;
                            // Par substituto (caracteres fora do plano básico).
                            if (0xD800..0xDC00).contains(&code) && self.s[self.i..].starts_with(b"\\u") {
                                self.i += 2;
                                let low = self.hex4()?;
                                code = 0x10000 + ((code - 0xD800) << 10) + (low.wrapping_sub(0xDC00) & 0x3FF);
                            }
                            let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
                            let mut buf = [0u8; 4];
                            out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        _ => return self.err("escape inválido"),
                    }
                }
                _ => out.push(c),
            }
        }
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let txt = self.s.get(self.i..self.i + 4).and_then(|b| std::str::from_utf8(b).ok());
        let Some(code) = txt.and_then(|t| u32::from_str_radix(t, 16).ok()) else {
            return self.err("\\u inválido");
        };
        self.i += 4;
        Ok(code)
    }

    fn array(&mut self, depth: usize) -> Result<Value, String> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.s.get(self.i) == Some(&b']') {
            self.i += 1;
            return Ok(Value::Array(items));
        }
        loop {
            self.ws();
            items.push(self.value(depth + 1)?);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b']') => {
                    self.i += 1;
                    return Ok(Value::Array(items));
                }
                _ => return self.err("esperava , ou ]"),
            }
        }
    }

    fn object(&mut self, depth: usize) -> Result<Value, String> {
        self.i += 1;
        let mut map = BTreeMap::new();
        self.ws();
        if self.s.get(self.i) == Some(&b'}') {
            self.i += 1;
            return Ok(Value::Object(map));
        }
        loop {
            self.ws();
            if self.s.get(self.i) != Some(&b'"') {
                return self.err("esperava o nome do campo");
            }
            let key = self.string()?;
            self.ws();
            if self.s.get(self.i) != Some(&b':') {
                return self.err("esperava :");
            }
            self.i += 1;
            self.ws();
            map.insert(key, self.value(depth + 1)?);
            self.ws();
            match self.s.get(self.i) {
                Some(b',') => self.i += 1,
                Some(b'}') => {
                    self.i += 1;
                    return Ok(Value::Object(map));
                }
                _ => return self.err("esperava , ou }"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_objetos_listas_e_valores() {
        let v = parse(r#"[{"id": 49, "info": {"params": {"Route": [{"index": 1, "name": "rear"}]}}, "ok": true, "x": null}]"#)
            .unwrap();
        let o = v.index(0);
        assert_eq!(o.get("id").as_i64(), Some(49));
        assert_eq!(o.get("info").get("params").get("Route").index(0).get("name").as_str(), Some("rear"));
        assert_eq!(o.get("ok"), &Value::Bool(true));
        assert_eq!(o.get("x"), &Value::Null);
        assert_eq!(o.get("falta").get("mais").as_str(), None);
    }

    #[test]
    fn textos_com_escape_e_acento() {
        let v = parse(r#""Microfone é \"bom\"\n😀 já""#).unwrap();
        assert_eq!(v.as_str(), Some("Microfone é \"bom\"\n😀 já"));
        // Número em texto (propriedades do PipeWire).
        assert_eq!(parse(r#""15""#).unwrap().as_i64(), Some(15));
        assert_eq!(parse("-2.5e1").unwrap(), Value::Number(-25.0));
    }

    #[test]
    fn recusa_json_quebrado_e_fundo_demais() {
        assert!(parse("{\"a\": }").is_err());
        assert!(parse("[1, 2").is_err());
        assert!(parse("\"sem fim").is_err());
        assert!(parse("[1] lixo").is_err());
        assert!(parse(&"[".repeat(500)).is_err());
    }
}
