//! Python's ways with values, where the port has to behave exactly as the Python code it
//! replaces: JSON read the way Python reads it (an int stays an int, so 63 and 63.0 print
//! differently), truthiness and `or`, round() rounding half to even, `%d` cutting a float
//! toward zero, `%g`, a float's repr, str.split() and str.strip() with Python's idea of
//! whitespace, and json.dumps(indent=2, ensure_ascii=False).

use serde_json::{Map, Number, Value};

pub type Dict = Map<String, Value>;

static NULL: Value = Value::Null;

/// A Python number. JSON's ints and floats are kept apart, as Python keeps them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    /// A JSON value as Python sees a number: an int, a float, or a bool, which is an int in
    /// Python; None for anything else.
    pub fn of(v: &Value) -> Option<Num> {
        match v {
            Value::Number(n) => match n.as_i64() {
                Some(i) => Some(Num::Int(i)),
                None => n.as_f64().map(Num::Float),
            },
            Value::Bool(b) => Some(Num::Int(i64::from(*b))),
            _ => None,
        }
    }

    pub fn f(self) -> f64 {
        match self {
            Num::Int(i) => i as f64,
            Num::Float(x) => x,
        }
    }

    pub fn value(self) -> Value {
        match self {
            Num::Int(i) => Value::from(i),
            Num::Float(x) => Number::from_f64(x)
                .map(Value::Number)
                .unwrap_or(Value::Null),
        }
    }

    /// a - b: an int when both are.
    pub fn minus(a: Num, b: Num) -> Num {
        match (a, b) {
            (Num::Int(x), Num::Int(y)) => Num::Int(x - y),
            _ => Num::Float(a.f() - b.f()),
        }
    }

    /// Python's min(a, b): b when it's smaller, else a, keeping its type.
    pub fn min(a: Num, b: Num) -> Num {
        if b.f() < a.f() {
            b
        } else {
            a
        }
    }

    /// Python's max(a, b): b when it's bigger, else a, keeping its type.
    pub fn max(a: Num, b: Num) -> Num {
        if b.f() > a.f() {
            b
        } else {
            a
        }
    }

    /// `"%d" % n`: a float cut toward zero.
    pub fn d(self) -> String {
        match self {
            Num::Int(i) => i.to_string(),
            Num::Float(x) => format!("{}", x.trunc() as i64),
        }
    }

    /// str(n), as Python prints it.
    pub fn repr(self) -> String {
        match self {
            Num::Int(i) => i.to_string(),
            Num::Float(x) => float_repr(x),
        }
    }
}

/// Python's bool(v).
pub fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_none_or(|x| x != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(a) => !a.is_empty(),
        Value::Object(o) => !o.is_empty(),
    }
}

/// Python's `a or b`.
pub fn or<'a>(a: &'a Value, b: &'a Value) -> &'a Value {
    if truthy(a) {
        a
    } else {
        b
    }
}

/// d.get(key) on a JSON value: None (null) when it's missing, or when d isn't a dict.
pub fn get<'a>(d: &'a Value, key: &str) -> &'a Value {
    d.as_object().and_then(|o| o.get(key)).unwrap_or(&NULL)
}

/// The value as a str, when it's one.
pub fn s(v: &Value) -> Option<&str> {
    v.as_str()
}

/// isinstance(v, dict).
pub fn is_dict(v: &Value) -> bool {
    v.is_object()
}

/// The null value, to hand out where a reference is wanted.
pub fn null() -> &'static Value {
    &NULL
}

/// Python's str(v) for the values JSON holds.
pub fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => repr(other),
    }
}

/// Python's repr(v) for the values JSON holds.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(_) => Num::of(v).map(Num::repr).unwrap_or_default(),
        Value::String(s) => {
            let quote = if s.contains('\'') && !s.contains('"') {
                '"'
            } else {
                '\''
            };
            let mut out = String::from(quote);
            for c in s.chars() {
                match c {
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if c == quote => {
                        out.push('\\');
                        out.push(c);
                    }
                    c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                        out.push_str(&format!("\\x{:02x}", c as u32))
                    }
                    c => out.push(c),
                }
            }
            out.push(quote);
            out
        }
        Value::Array(a) => format!("[{}]", a.iter().map(repr).collect::<Vec<_>>().join(", ")),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{}: {}", repr(&Value::String(k.clone())), repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// The digits and decimal exponent of x's shortest round-trip form: 62.5 is ("625", 1).
fn shortest(x: f64) -> (bool, String, i32) {
    let e = format!("{:e}", x.abs());
    let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    (x.is_sign_negative(), digits, exp.parse().unwrap_or(0))
}

/// repr(x) for a float, as Python prints it: 62.5, 100.0, 1e-05, 1.5e+16.
pub fn float_repr(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0" } else { "0.0" }.into();
    }
    let (neg, digits, exp) = shortest(x);
    let sign = if neg { "-" } else { "" };
    if !(-4..16).contains(&exp) {
        let mantissa = if digits.len() > 1 {
            format!("{}.{}", &digits[..1], &digits[1..])
        } else {
            digits
        };
        return format!(
            "{sign}{mantissa}e{}{:02}",
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        );
    }
    if exp >= 0 {
        let point = exp as usize + 1;
        if digits.len() <= point {
            format!("{sign}{digits}{}.0", "0".repeat(point - digits.len()))
        } else {
            format!("{sign}{}.{}", &digits[..point], &digits[point..])
        }
    } else {
        format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
    }
}

/// `"%g" % x`: six significant digits, trailing zeros dropped, an exponent outside 1e-4 to 1e6.
pub fn g(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.into();
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let e = format!("{x:.5e}");
    let (mantissa, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    if !(-4..6).contains(&exp) {
        let mantissa = mantissa.trim_end_matches('0').trim_end_matches('.');
        return format!(
            "{mantissa}e{}{:02}",
            if exp < 0 { '-' } else { '+' },
            exp.abs()
        );
    }
    let fixed = format!("{:.*}", (5 - exp) as usize, x);
    if fixed.contains('.') {
        fixed
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    } else {
        fixed
    }
}

/// Python's round(x) for a float: to the nearest whole number, halves to even.
pub fn round(x: f64) -> f64 {
    x.round_ties_even()
}

/// Whether Python's str.isspace() holds for c.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

/// " ".join(text.split()): runs of whitespace made one space, none at the ends.
pub fn collapse(text: &str) -> String {
    text.split(is_space)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn strip(text: &str) -> &str {
    text.trim_matches(is_space)
}

pub fn lstrip(text: &str) -> &str {
    text.trim_start_matches(is_space)
}

/// str.splitlines(): lines split at every line boundary Python knows, the ends dropped.
pub fn splitlines(text: &str) -> Vec<&str> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let end = match c {
            '\r' => {
                if let Some(&(_, '\n')) = chars.peek() {
                    chars.next();
                    i + 2
                } else {
                    i + 1
                }
            }
            '\n' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}'
            | '\u{2029}' => i + c.len_utf8(),
            _ => continue,
        };
        lines.push(&text[start..i]);
        start = end;
    }
    if start < text.len() {
        lines.push(&text[start..]);
    }
    lines
}

/// The first n characters of text, as text[:n] gives them.
pub fn head(text: &str, n: usize) -> &str {
    match text.char_indices().nth(n) {
        Some((i, _)) => &text[..i],
        None => text,
    }
}

/// Python's int(text) for a str: an optional sign, digits with single underscores between
/// them, whitespace around.
pub fn int(text: &str) -> Option<i64> {
    let t = strip(text);
    let (sign, digits) = match t.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
    {
        return None;
    }
    let clean: String = digits.chars().filter(|c| *c != '_').collect();
    if !clean.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    clean.parse::<i64>().ok().map(|n| sign * n)
}

/// Python's float(text) for a str: digits with single underscores between them, an exponent,
/// inf and nan, whitespace around.
pub fn float(text: &str) -> Option<f64> {
    let t = strip(text);
    if t.contains('_') {
        let c: Vec<char> = t.chars().collect();
        for (i, ch) in c.iter().enumerate() {
            if *ch == '_'
                && !(i > 0
                    && c[i - 1].is_ascii_digit()
                    && c.get(i + 1).is_some_and(char::is_ascii_digit))
            {
                return None;
            }
        }
    }
    let clean: String = t.chars().filter(|c| *c != '_').collect();
    let lower = clean.to_lowercase();
    let body = lower.trim_start_matches(['+', '-']);
    if matches!(body, "inf" | "infinity" | "nan") || clean.chars().any(|c| c.is_ascii_digit()) {
        return clean.parse::<f64>().ok();
    }
    None
}

/// json.dumps(v, ensure_ascii=False): on one line, with Python's ", " and ": ".
pub fn dumps_compact(v: &Value) -> String {
    let mut out = String::new();
    write_compact(&mut out, v);
    out
}

/// json.dumps(v): on one line, and everything beyond ASCII written as \uXXXX, as Python's
/// default ensure_ascii has it.
pub fn dumps_ascii(v: &Value) -> String {
    let mut out = String::new();
    for c in dumps_compact(v).chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{:04x}", unit));
            }
        }
    }
    out
}

/// json.dumps(v, ensure_ascii=False, separators=(",", ":")): as Claude Code and Codex write
/// their lines, and as the Python version rewrote a line it changed.
pub fn dumps_tight(v: &Value) -> String {
    let mut out = String::new();
    write_tight(&mut out, v);
    out
}

fn write_tight(out: &mut String, v: &Value) {
    match v {
        Value::Array(a) => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_tight(out, item);
            }
            out.push(']');
        }
        Value::Object(o) => {
            out.push('{');
            for (i, (k, item)) in o.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_str(out, k);
                out.push(':');
                write_tight(out, item);
            }
            out.push('}');
        }
        other => write_json(out, other, 0),
    }
}

/// Python's == for JSON values: numbers by value (1, 1.0 and true alike), dicts whatever
/// their order, lists in order.
pub fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Null, Value::Null) => true,
        _ => match (Num::of(a), Num::of(b)) {
            (Some(x), Some(y)) => x.f() == y.f(),
            _ => false,
        },
    }
}

fn write_compact(out: &mut String, v: &Value) {
    match v {
        Value::Array(a) => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_compact(out, item);
            }
            out.push(']');
        }
        Value::Object(o) => {
            out.push('{');
            for (i, (k, item)) in o.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_str(out, k);
                out.push_str(": ");
                write_compact(out, item);
            }
            out.push('}');
        }
        other => write_json(out, other, 0),
    }
}

/// json.dumps(v, ensure_ascii=False, indent=2), as Python writes it.
pub fn dumps(v: &Value) -> String {
    let mut out = String::new();
    write_json(&mut out, v, 0);
    out
}

fn write_json(out: &mut String, v: &Value, depth: usize) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(_) => match Num::of(v) {
            Some(Num::Float(x)) if x.is_nan() => out.push_str("NaN"),
            Some(Num::Float(x)) if x.is_infinite() => {
                out.push_str(if x > 0.0 { "Infinity" } else { "-Infinity" })
            }
            Some(n) => out.push_str(&n.repr()),
            None => out.push_str(&v.to_string()),
        },
        Value::String(s) => write_str(out, s),
        Value::Array(a) if a.is_empty() => out.push_str("[]"),
        Value::Array(a) => {
            out.push('[');
            for (i, item) in a.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.push_str(&"  ".repeat(depth + 1));
                write_json(out, item, depth + 1);
            }
            out.push('\n');
            out.push_str(&"  ".repeat(depth));
            out.push(']');
        }
        Value::Object(o) if o.is_empty() => out.push_str("{}"),
        Value::Object(o) => {
            out.push('{');
            for (i, (k, item)) in o.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.push_str(&"  ".repeat(depth + 1));
                write_str(out, k);
                out.push_str(": ");
                write_json(out, item, depth + 1);
            }
            out.push('\n');
            out.push_str(&"  ".repeat(depth));
            out.push('}');
        }
    }
}

fn write_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn floats_print_as_python_prints_them() {
        for (x, want) in [
            (62.5, "62.5"),
            (100.0, "100.0"),
            (0.001, "0.001"),
            (0.0001, "0.0001"),
            (0.00001, "1e-05"),
            (1e16, "1e+16"),
            (1.5e16, "1.5e+16"),
            (123456789.0, "123456789.0"),
            (-2.25, "-2.25"),
            (0.1 + 0.2, "0.30000000000000004"),
        ] {
            assert_eq!(float_repr(x), want, "{x}");
        }
    }

    #[test]
    fn percent_g() {
        for (x, want) in [
            (100.0, "100"),
            (0.0001, "0.0001"),
            (1e-5, "1e-05"),
            (123456789.0, "1.23457e+08"),
            (106.08, "106.08"),
            (2.5, "2.5"),
        ] {
            assert_eq!(g(x), want, "{x}");
        }
    }

    #[test]
    fn numbers_keep_their_type() {
        let used = Num::of(&json!(40)).unwrap();
        assert_eq!(
            Num::max(
                Num::Int(0),
                Num::min(Num::Int(100), Num::minus(Num::Int(100), used))
            ),
            Num::Int(60)
        );
        let used = Num::of(&json!(37.5)).unwrap();
        assert_eq!(Num::minus(Num::Int(100), used).repr(), "62.5");
        assert_eq!(Num::Float(62.9).d(), "62");
        assert_eq!(Num::min(Num::Int(100), Num::Float(120.0)), Num::Int(100));
    }

    #[test]
    fn json_as_python_dumps_it() {
        let v = json!({"a": [1, 2.0, "é\n"], "b": {}, "c": [], "d": null});
        assert_eq!(dumps(&v), "{\n  \"a\": [\n    1,\n    2.0,\n    \"é\\n\"\n  ],\n  \"b\": {},\n  \"c\": [],\n  \"d\": null\n}");
    }

    #[test]
    fn lines_and_ints() {
        assert_eq!(
            splitlines("a\nb\r\nc\rd\x0ce\u{2028}f\n"),
            vec!["a", "b", "c", "d", "e", "f"]
        );
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert_eq!(splitlines("\n\n"), vec!["", ""]);
        assert_eq!(
            (
                int(" 5 "),
                int("+5"),
                int("-3"),
                int("1_000"),
                int("1__0"),
                int("x"),
                int("")
            ),
            (Some(5), Some(5), Some(-3), Some(1000), None, None, None)
        );
        assert_eq!(
            dumps_compact(&json!({"a": [1, 2.5], "b": {}})),
            "{\"a\": [1, 2.5], \"b\": {}}"
        );
        assert_eq!(head("héllo", 2), "hé");
    }

    #[test]
    fn tight_json_and_equality() {
        assert_eq!(
            dumps_tight(&json!({"a": [1, 2.5, "x\ny"], "b": {}, "é": null})),
            "{\"a\":[1,2.5,\"x\\ny\"],\"b\":{},\"é\":null}"
        );
        assert!(same(
            &json!({"a": 1, "b": [true]}),
            &json!({"b": [1], "a": 1.0})
        ));
        assert!(!same(&json!([1, 2]), &json!([2, 1])));
    }

    #[test]
    fn whitespace() {
        assert_eq!(collapse("  a \t b\x1c c  "), "a b c");
        assert_eq!(round(2.5), 2.0);
        assert_eq!(round(3.5), 4.0);
    }
}
