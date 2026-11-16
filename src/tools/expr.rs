//! Avaliador de expressões da Calculadora (aba Calculadora e aba Programador).
//!
//! Uma gramática só, avaliada de dois jeitos:
//! - **real** (`f64`): `^` é potência, `%` depois de um número é porcentagem
//!   (`50%` = 0,5), `mod` é resto; funções (`sqrt`, `sin`, `log`...), `pi`,
//!   `e`, `ans` (último resultado), fatorial (`5!`), multiplicação implícita
//!   (`2pi`, `3(4+1)`); vírgula vale como ponto decimal (`3,5`).
//! - **inteiro** (`i128`, Programador): literais `0x`, `0b`, `0o`; `& | ~ << >>`,
//!   `^` é OU exclusivo (como em C), `**` é potência, `%` é resto.
//!
//! Erros voltam como texto curto em inglês e português (`tr`).

use crate::common::i18n::tr;

/// Uma peça da expressão.
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64, Option<i128>),
    Ident(String),
    Op(&'static str),
    LParen,
    RParen,
}

/// Árvore da expressão.
#[derive(Debug, Clone)]
enum Node {
    Num(f64, Option<i128>),
    Var(String),
    Unary(&'static str, Box<Node>),
    Binary(&'static str, Box<Node>, Box<Node>),
    Postfix(&'static str, Box<Node>),
    Call(String, Box<Node>),
}

fn err(en: &str, pt: &str) -> String {
    tr(en, pt).to_string()
}

fn tokenize(src: &str, integer: bool) -> Result<Vec<Tok>, String> {
    let s: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // Números: 0x.., 0b.., 0o.., decimais (com . ou ,) e expoente.
        if c.is_ascii_digit() || (c == '.' && s.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let radix = match (c, s.get(i + 1)) {
                ('0', Some('x' | 'X')) => 16,
                ('0', Some('b' | 'B')) => 2,
                ('0', Some('o' | 'O')) => 8,
                _ => 10,
            };
            if radix != 10 {
                let mut j = i + 2;
                let mut digits = String::new();
                while j < s.len() && (s[j].is_digit(radix) || s[j] == '_') {
                    if s[j] != '_' {
                        digits.push(s[j]);
                    }
                    j += 1;
                }
                let v = u128::from_str_radix(&digits, radix)
                    .map_err(|_| err("Invalid number", "Número inválido"))? as i128;
                out.push(Tok::Num(v as f64, Some(v)));
                i = j;
                continue;
            }
            let mut j = i;
            let mut text = String::new();
            while j < s.len() {
                let d = s[j];
                if d.is_ascii_digit() || d == '_' {
                    if d != '_' {
                        text.push(d);
                    }
                } else if (d == '.' || d == ',') && !integer && !text.contains('.') {
                    // Vírgula decimal só se vier um dígito depois.
                    if !s.get(j + 1).is_some_and(|n| n.is_ascii_digit()) {
                        break;
                    }
                    text.push('.');
                } else if (d == 'e' || d == 'E')
                    && !integer
                    && s.get(j + 1).is_some_and(|n| n.is_ascii_digit() || *n == '-' || *n == '+')
                    && !s.get(j + 1).is_some_and(|n| n.is_ascii_alphabetic())
                {
                    text.push('e');
                    if matches!(s.get(j + 1), Some('-' | '+')) {
                        j += 1;
                        text.push(s[j]);
                    }
                } else {
                    break;
                }
                j += 1;
            }
            let f: f64 = text.parse().map_err(|_| err("Invalid number", "Número inválido"))?;
            let int = text.parse::<i128>().ok();
            out.push(Tok::Num(f, int));
            i = j;
            continue;
        }
        if c.is_alphabetic() || c == 'π' {
            let mut j = i;
            let mut name = String::new();
            while j < s.len() && (s[j].is_alphanumeric() || s[j] == 'π') {
                name.push(s[j]);
                j += 1;
            }
            out.push(Tok::Ident(name.to_lowercase()));
            i = j;
            continue;
        }
        let two: String = s[i..(i + 2).min(s.len())].iter().collect();
        let op: &'static str = match two.as_str() {
            "**" => "**",
            "<<" => "<<",
            ">>" => ">>",
            _ => match c {
                '+' => "+",
                '-' | '−' => "-",
                '*' | '×' | '·' => "*",
                '/' | '÷' => "/",
                '^' => "^",
                '%' => "%",
                '!' => "!",
                '&' => "&",
                '|' => "|",
                '~' => "~",
                '√' => "√",
                '(' => {
                    out.push(Tok::LParen);
                    i += 1;
                    continue;
                }
                ')' => {
                    out.push(Tok::RParen);
                    i += 1;
                    continue;
                }
                _ => return Err(format!("{} '{c}'", tr("Unexpected", "Inesperado"))),
            },
        };
        out.push(Tok::Op(op));
        i += if matches!(op, "**" | "<<" | ">>") { 2 } else { 1 };
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    integer: bool,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn eat_op(&mut self, ops: &[&str]) -> Option<&'static str> {
        if let Some(Tok::Op(o)) = self.peek() {
            if ops.contains(o) {
                let o = *o;
                self.pos += 1;
                return Some(o);
            }
        }
        // "xor" e "mod" escritos por extenso.
        if let Some(Tok::Ident(w)) = self.peek() {
            let word = match w.as_str() {
                "xor" if ops.contains(&"xor") => "xor",
                "mod" if ops.contains(&"mod") => "mod",
                "and" if ops.contains(&"&") => "&",
                "or" if ops.contains(&"|") => "|",
                _ => return None,
            };
            self.pos += 1;
            return Some(word);
        }
        None
    }
    fn binary(&mut self, next: fn(&mut Parser) -> Result<Node, String>, ops: &[&str]) -> Result<Node, String> {
        let mut left = next(self)?;
        while let Some(op) = self.eat_op(ops) {
            let right = next(self)?;
            left = Node::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }
    fn expr(&mut self) -> Result<Node, String> {
        self.binary(Parser::xor, &["|"])
    }
    fn xor(&mut self) -> Result<Node, String> {
        if self.integer { self.binary(Parser::and, &["^", "xor"]) } else { self.binary(Parser::and, &["xor"]) }
    }
    fn and(&mut self) -> Result<Node, String> {
        self.binary(Parser::shift, &["&"])
    }
    fn shift(&mut self) -> Result<Node, String> {
        self.binary(Parser::add, &["<<", ">>"])
    }
    fn add(&mut self) -> Result<Node, String> {
        self.binary(Parser::mul, &["+", "-"])
    }
    fn mul(&mut self) -> Result<Node, String> {
        let mut left = self.unary()?;
        loop {
            let ops: &[&str] = if self.integer { &["*", "/", "%", "mod"] } else { &["*", "/", "mod"] };
            if let Some(op) = self.eat_op(ops) {
                let right = self.unary()?;
                left = Node::Binary(op, Box::new(left), Box::new(right));
                continue;
            }
            // Multiplicação implícita: 2pi, 3(4+1), (1+1)(2+2), 2sqrt(9).
            let implicit = match self.peek() {
                Some(Tok::LParen | Tok::Num(..)) => true,
                Some(Tok::Ident(w)) => !matches!(w.as_str(), "xor" | "mod" | "and" | "or"),
                Some(Tok::Op("√")) => true,
                _ => false,
            };
            if !implicit {
                return Ok(left);
            }
            let right = self.unary()?;
            left = Node::Binary("*", Box::new(left), Box::new(right));
        }
    }
    fn unary(&mut self) -> Result<Node, String> {
        if let Some(op) = self.eat_op(&["-", "+", "~", "√"]) {
            let inner = self.unary()?;
            return Ok(match op {
                "+" => inner,
                "√" => Node::Call("sqrt".into(), Box::new(inner)),
                _ => Node::Unary(op, Box::new(inner)),
            });
        }
        self.power()
    }
    fn power(&mut self) -> Result<Node, String> {
        let base = self.postfix()?;
        let ops: &[&str] = if self.integer { &["**"] } else { &["^", "**"] };
        if self.eat_op(ops).is_some() {
            // Potência associa à direita e aceita sinal: 2^-1, 2^3^2.
            let exp = self.unary()?;
            return Ok(Node::Binary("**", Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }
    fn postfix(&mut self) -> Result<Node, String> {
        let mut node = self.primary()?;
        loop {
            if self.eat_op(&["!"]).is_some() {
                node = Node::Postfix("!", Box::new(node));
            } else if !self.integer && matches!(self.peek(), Some(Tok::Op("%"))) {
                self.pos += 1;
                node = Node::Postfix("%", Box::new(node));
            } else {
                return Ok(node);
            }
        }
    }
    fn primary(&mut self) -> Result<Node, String> {
        match self.toks.get(self.pos).cloned() {
            Some(Tok::Num(f, i)) => {
                self.pos += 1;
                Ok(Node::Num(f, i))
            }
            Some(Tok::LParen) => {
                self.pos += 1;
                let e = self.expr()?;
                if self.peek() == Some(&Tok::RParen) {
                    self.pos += 1;
                }
                // Parêntese sem fechar no fim: aceita (o resultado ao vivo
                // aparece enquanto se digita).
                Ok(e)
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                if FUNCTIONS.contains(&name.as_str()) {
                    let arg = if self.peek() == Some(&Tok::LParen) { self.primary()? } else { self.unary()? };
                    Ok(Node::Call(name, Box::new(arg)))
                } else {
                    Ok(Node::Var(name))
                }
            }
            Some(_) => Err(err("Unexpected symbol", "Símbolo fora do lugar")),
            None => Err(err("Incomplete expression", "Expressão incompleta")),
        }
    }
}

/// Funções conhecidas (um argumento).
const FUNCTIONS: [&str; 18] = [
    "sqrt", "cbrt", "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "ln", "log", "log2", "exp",
    "abs", "round", "floor",
];

fn parse(src: &str, integer: bool) -> Result<Node, String> {
    let toks = tokenize(src, integer)?;
    if toks.is_empty() {
        return Err(String::new());
    }
    let mut p = Parser { toks, pos: 0, integer };
    let node = p.expr()?;
    if p.pos < p.toks.len() {
        return Err(err("Unexpected symbol", "Símbolo fora do lugar"));
    }
    Ok(node)
}

fn eval_f(node: &Node, ans: f64, degrees: bool) -> Result<f64, String> {
    let ang = |x: f64| if degrees { x.to_radians() } else { x };
    let inv = |x: f64| if degrees { x.to_degrees() } else { x };
    Ok(match node {
        Node::Num(f, _) => *f,
        Node::Var(v) => match v.as_str() {
            "pi" | "π" => std::f64::consts::PI,
            "e" => std::f64::consts::E,
            "tau" => std::f64::consts::TAU,
            "ans" => ans,
            other => return Err(format!("{} '{other}'", tr("Unknown name", "Nome desconhecido"))),
        },
        Node::Unary(_, a) => -eval_f(a, ans, degrees)?,
        Node::Postfix("%", a) => eval_f(a, ans, degrees)? / 100.0,
        Node::Postfix(_, a) => {
            let n = eval_f(a, ans, degrees)?;
            if n < 0.0 || n.fract() != 0.0 || n > 170.0 {
                return Err(err("Factorial needs a whole number 0–170", "Fatorial só de inteiro de 0 a 170"));
            }
            (1..=n as u64).map(|k| k as f64).product()
        }
        Node::Binary(op, a, b) => {
            let (x, y) = (eval_f(a, ans, degrees)?, eval_f(b, ans, degrees)?);
            match *op {
                "+" => x + y,
                "-" => x - y,
                "*" => x * y,
                "/" => {
                    if y == 0.0 {
                        return Err(err("Division by zero", "Divisão por zero"));
                    }
                    x / y
                }
                "mod" => x.rem_euclid(y),
                "**" => x.powf(y),
                _ => return Err(err("Use the Programmer tab for bitwise operations", "Operações de bits ficam na aba Programador")),
            }
        }
        Node::Call(f, a) => {
            let x = eval_f(a, ans, degrees)?;
            match f.as_str() {
                "sqrt" => x.sqrt(),
                "cbrt" => x.cbrt(),
                "sin" => ang(x).sin(),
                "cos" => ang(x).cos(),
                "tan" => ang(x).tan(),
                "asin" => inv(x.asin()),
                "acos" => inv(x.acos()),
                "atan" => inv(x.atan()),
                "sinh" => x.sinh(),
                "cosh" => x.cosh(),
                "tanh" => x.tanh(),
                "ln" => x.ln(),
                "log" => x.log10(),
                "log2" => x.log2(),
                "exp" => x.exp(),
                "abs" => x.abs(),
                "round" => x.round(),
                _ => x.floor(),
            }
        }
    })
}

/// Avalia uma expressão real. `ans` = último resultado; `degrees` = ângulos
/// em graus. Expressão vazia: `Err("")`.
pub fn eval(src: &str, ans: f64, degrees: bool) -> Result<f64, String> {
    let v = eval_f(&parse(src, false)?, ans, degrees)?;
    if v.is_nan() {
        return Err(err("Not a number", "Resultado indefinido"));
    }
    Ok(v)
}

/// Número real para mostrar: inteiro sem casas, até 12 algarismos
/// significativos, notação científica para muito grande ou pequeno.
pub fn format_f(v: f64) -> String {
    if v.is_infinite() {
        return if v > 0.0 { "∞".into() } else { "-∞".into() };
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if v.fract() == 0.0 && a < 1e15 {
        return format!("{}", v as i64);
    }
    if !(1e-6..1e15).contains(&a) {
        let s = format!("{v:.10e}");
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let m = m.trim_end_matches('0').trim_end_matches('.');
        return format!("{m}e{e}");
    }
    let digits = 12 - (a.log10().floor() as i32 + 1);
    let s = format!("{:.*}", digits.clamp(0, 15) as usize, v);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn eval_i(node: &Node, ans: i128) -> Result<i128, String> {
    let overflow = || err("Number too large", "Número grande demais");
    Ok(match node {
        Node::Num(f, i) => match i {
            Some(i) => *i,
            None if f.fract() == 0.0 => *f as i128,
            None => return Err(err("Only whole numbers here", "Aqui só números inteiros")),
        },
        Node::Var(v) if v == "ans" => ans,
        Node::Var(v) => return Err(format!("{} '{v}'", tr("Unknown name", "Nome desconhecido"))),
        Node::Unary("~", a) => !eval_i(a, ans)?,
        Node::Unary(_, a) => eval_i(a, ans)?.checked_neg().ok_or_else(overflow)?,
        Node::Postfix(_, a) => {
            let n = eval_i(a, ans)?;
            if !(0..=33).contains(&n) {
                return Err(err("Factorial needs 0–33", "Fatorial só de 0 a 33"));
            }
            (1..=n).product()
        }
        Node::Binary(op, a, b) => {
            let (x, y) = (eval_i(a, ans)?, eval_i(b, ans)?);
            match *op {
                "+" => x.checked_add(y).ok_or_else(overflow)?,
                "-" => x.checked_sub(y).ok_or_else(overflow)?,
                "*" => x.checked_mul(y).ok_or_else(overflow)?,
                "/" | "%" | "mod" => {
                    if y == 0 {
                        return Err(err("Division by zero", "Divisão por zero"));
                    }
                    if *op == "/" { x / y } else { x.rem_euclid(y) }
                }
                "**" => {
                    let e = u32::try_from(y).map_err(|_| err("Invalid exponent", "Expoente inválido"))?;
                    x.checked_pow(e).ok_or_else(overflow)?
                }
                "&" => x & y,
                "|" => x | y,
                "^" | "xor" => x ^ y,
                "<<" => x.checked_shl(u32::try_from(y).unwrap_or(200)).ok_or_else(overflow)?,
                _ => x.checked_shr(u32::try_from(y).unwrap_or(200)).unwrap_or(0),
            }
        }
        Node::Call(..) => return Err(err("Functions are in the Calculator tab", "Funções ficam na aba Calculadora")),
    })
}

/// Avalia uma expressão inteira (Programador). Expressão vazia: `Err("")`.
pub fn eval_int(src: &str, ans: i128) -> Result<i128, String> {
    eval_i(&parse(src, true)?, ans)
}

/// O valor cortado em `bits` (8–64): o padrão de bits e o número com sinal
/// (complemento de dois) ou sem.
pub fn wrap(v: i128, bits: u32, signed: bool) -> (u64, i128) {
    let mask: u128 = if bits >= 64 { u64::MAX as u128 } else { (1u128 << bits) - 1 };
    let raw = (v as u128 & mask) as u64;
    let shown = if signed && bits < 128 && raw >> (bits - 1) & 1 == 1 {
        raw as i128 - (1i128 << bits)
    } else {
        raw as i128
    };
    (raw, shown)
}

/// Dígitos de `raw` na base, agrupados (4 para binário e hexa, 3 para octal).
pub fn grouped(raw: u64, radix: u32, bits: u32) -> String {
    let digits = match radix {
        2 => format!("{raw:0width$b}", width = bits as usize),
        8 => format!("{raw:o}"),
        16 => format!("{raw:0width$X}", width = (bits as usize).div_ceil(4)),
        _ => raw.to_string(),
    };
    let group = match radix {
        2 | 16 => 4,
        8 => 3,
        _ => return digits,
    };
    let chars: Vec<char> = digits.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (chars.len() - i).is_multiple_of(group) {
            out.push(' ');
        }
        out.push(*c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(s: &str) -> String {
        eval(s, 0.0, false).map(format_f).unwrap_or_else(|e| format!("erro: {e}"))
    }

    #[test]
    fn contas_basicas() {
        assert_eq!(r("1 + 2 * 3"), "7");
        assert_eq!(r("(1 + 2) * 3"), "9");
        assert_eq!(r("2^3^2"), "512");
        assert_eq!(r("-2^2"), "-4");
        assert_eq!(r("2^-1"), "0.5");
        assert_eq!(r("10 / 4"), "2.5");
        assert_eq!(r("3,5 * 2"), "7");
        assert_eq!(r("50%"), "0.5");
        assert_eq!(r("200 * 15%"), "30");
        assert_eq!(r("17 mod 5"), "2");
        assert_eq!(r("5!"), "120");
        assert_eq!(r("2pi"), r("2*pi"));
        assert_eq!(r("3(4+1)"), "15");
        assert_eq!(r("sqrt(16) + √9"), "7");
        assert_eq!(r("1e3 + 1"), "1001");
        assert_eq!(r("0.1 + 0.2"), "0.3");
        assert_eq!(r("1/3"), "0.333333333333");
        assert_eq!(r("(1 + 2"), "3");
        assert_eq!(r("2 × 3 ÷ 4 − 1"), "0.5");
        assert!(r("1/0").starts_with("erro"));
        assert!(r("1 +").starts_with("erro"));
        assert_eq!(eval("", 0.0, false), Err(String::new()));
        assert_eq!(format_f(eval("ans * 2", 21.0, false).unwrap()), "42");
        assert_eq!(format_f(eval("sin(90)", 0.0, true).unwrap()), "1");
        assert_eq!(format_f(1e20), "1e20");
    }

    #[test]
    fn programador() {
        assert_eq!(eval_int("0xFF & 0b1010 << 2", 0), Ok(0xFF & (0b1010 << 2)));
        assert_eq!(eval_int("0x0F | 0xF0", 0), Ok(0xFF));
        assert_eq!(eval_int("5 ^ 3", 0), Ok(6));
        assert_eq!(eval_int("2 ** 10", 0), Ok(1024));
        assert_eq!(eval_int("17 % 5", 0), Ok(2));
        assert_eq!(eval_int("~0", 0), Ok(-1));
        assert_eq!(eval_int("1_000", 0), Ok(1000));
        assert!(eval_int("1.5", 0).is_err());
        assert_eq!(wrap(-1, 8, false), (0xFF, 255));
        assert_eq!(wrap(-1, 8, true), (0xFF, -1));
        assert_eq!(wrap(0x1FF, 8, false), (0xFF, 255));
        assert_eq!(wrap(-1, 64, true).0, u64::MAX);
        assert_eq!(grouped(0xABCD, 16, 16), "ABCD");
        assert_eq!(grouped(0x1234_5678, 16, 32), "1234 5678");
        assert_eq!(grouped(5, 2, 8), "0000 0101");
        assert_eq!(grouped(0o1234, 8, 16), "1 234");
    }
}
