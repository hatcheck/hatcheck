//! The command lines Python's argparse read, read the same way: long options and unique
//! prefixes of them, --opt=value, -n1, --, positionals among the options, -h and --help, and
//! argparse's usage line, help layout and error messages (exit code 2).

use std::collections::HashMap;

/// What follows an option.
#[derive(Clone, Copy, PartialEq)]
pub enum Takes {
    Nothing,
    Value,
}

/// How many words a positional takes.
#[derive(Clone, Copy, PartialEq)]
pub enum Count {
    One,
    Optional,
    Any,
}

pub struct Opt {
    pub flags: Vec<&'static str>,
    pub dest: &'static str,
    pub takes: Takes,
    pub metavar: &'static str,
    pub help: String,
    pub hidden: bool,
}

pub struct Pos {
    pub name: &'static str,
    pub count: Count,
    pub help: String,
    pub metavar: Option<&'static str>,
}

pub struct Parser {
    pub prog: String,
    pub description: String,
    pub epilog: String,
    opts: Vec<Opt>,
    pos: Vec<Pos>,
}

/// A word on the command line, as argparse sorts them.
enum Word<'a> {
    Positional,
    Unknown,
    Opt(&'a Opt, &'static str, Option<String>, &'static str),
}

/// What a command line said.
#[derive(Default, Debug)]
pub struct Args {
    values: HashMap<&'static str, Vec<String>>,
    flags: HashMap<&'static str, usize>,
}

impl Args {
    pub fn flag(&self, dest: &str) -> bool {
        self.flags.get(dest).is_some_and(|n| *n > 0)
    }
    /// The last value given for dest.
    pub fn value(&self, dest: &str) -> Option<&str> {
        self.values
            .get(dest)
            .and_then(|v| v.last())
            .map(String::as_str)
    }
    pub fn values(&self, dest: &str) -> &[String] {
        self.values.get(dest).map_or(&[], Vec::as_slice)
    }
}

/// A command line that couldn't be read: argparse's message, or help asked for.
pub enum Stop {
    Help(String),
    Error(String),
}

impl Stop {
    /// Print it as argparse does and say what to exit with.
    pub fn exit_code(self) -> i32 {
        match self {
            Stop::Help(text) => {
                crate::out::print(&text);
                0
            }
            Stop::Error(text) => {
                crate::out::eprint(&text);
                2
            }
        }
    }
}

fn width() -> usize {
    crate::out::columns(80).saturating_sub(2).max(11)
}

impl Parser {
    pub fn new(prog: &str, description: &str) -> Parser {
        Parser {
            prog: prog.to_string(),
            description: description.to_string(),
            epilog: String::new(),
            opts: vec![Opt {
                flags: vec!["-h", "--help"],
                dest: "help",
                takes: Takes::Nothing,
                metavar: "",
                help: "show this help message and exit".into(),
                hidden: false,
            }],
            pos: Vec::new(),
        }
    }

    pub fn flag(mut self, flags: &[&'static str], dest: &'static str, help: &str) -> Parser {
        self.opts.push(Opt {
            flags: flags.to_vec(),
            dest,
            takes: Takes::Nothing,
            metavar: "",
            help: help.into(),
            hidden: false,
        });
        self
    }

    pub fn option(
        mut self,
        flags: &[&'static str],
        dest: &'static str,
        metavar: &'static str,
        help: &str,
    ) -> Parser {
        self.opts.push(Opt {
            flags: flags.to_vec(),
            dest,
            takes: Takes::Value,
            metavar,
            help: help.into(),
            hidden: false,
        });
        self
    }

    /// An option argparse lists nowhere (help=SUPPRESS).
    pub fn hidden(mut self, flags: &[&'static str], dest: &'static str, takes: Takes) -> Parser {
        self.opts.push(Opt {
            flags: flags.to_vec(),
            dest,
            takes,
            metavar: "",
            help: String::new(),
            hidden: true,
        });
        self
    }

    pub fn positional(mut self, name: &'static str, count: Count, help: &str) -> Parser {
        self.pos.push(Pos {
            name,
            count,
            help: help.into(),
            metavar: None,
        });
        self
    }

    pub fn positional_as(
        mut self,
        name: &'static str,
        metavar: &'static str,
        count: Count,
        help: &str,
    ) -> Parser {
        self.pos.push(Pos {
            name,
            count,
            help: help.into(),
            metavar: Some(metavar),
        });
        self
    }

    pub fn epilog(mut self, text: &str) -> Parser {
        self.epilog = text.into();
        self
    }

    /// argparse's error: the usage line, then `prog: error: message`.
    pub fn error(&self, message: &str) -> Stop {
        Stop::Error(format!(
            "{}{}: error: {}\n",
            self.usage(),
            self.prog,
            message
        ))
    }

    /// What a word is to argparse's _parse_optional: a word for the positionals, an option
    /// it doesn't know, or an option with what came attached to it (`=value` or `-n5`).
    fn classify<'a>(&'a self, arg: &str) -> Result<Word<'a>, String> {
        if arg.is_empty() || !arg.starts_with('-') || arg == "-" {
            return Ok(Word::Positional);
        }
        if let Some(o) = self.find(arg) {
            return Ok(Word::Opt(o, self.flag_of(o, arg), None, ""));
        }
        if let Some((name, value)) = arg.split_once('=') {
            if let Some(o) = self.find(name) {
                return Ok(Word::Opt(
                    o,
                    self.flag_of(o, name),
                    Some(value.to_string()),
                    "=",
                ));
            }
        }
        // Prefixes: --no-u for --no-usage, and -n5 for -n with 5.
        let mut tuples: Vec<(&Opt, &'static str, Option<String>, &'static str)> = Vec::new();
        if arg.starts_with("--") {
            let (prefix, explicit, sep) = match arg.split_once('=') {
                Some((p, v)) => (p, Some(v.to_string()), "="),
                None => (arg, None, ""),
            };
            for o in &self.opts {
                for f in o.flags.iter().filter(|f| f.starts_with(prefix)) {
                    tuples.push((o, f, explicit.clone(), sep));
                }
            }
        } else {
            let (short, rest) =
                arg.split_at(arg.char_indices().nth(2).map_or(arg.len(), |(i, _)| i));
            for o in &self.opts {
                for f in &o.flags {
                    if *f == short {
                        tuples.push((o, f, Some(rest.to_string()), ""));
                    } else if f.starts_with(arg) {
                        tuples.push((o, f, None, ""));
                    }
                }
            }
        }
        match tuples.len() {
            0 => {}
            1 => {
                let (o, f, explicit, sep) = tuples.remove(0);
                return Ok(Word::Opt(o, f, explicit, sep));
            }
            _ => {
                let options = tuples.iter().map(|t| t.1).collect::<Vec<_>>().join(", ");
                return Err(format!("ambiguous option: {arg} could match {options}"));
            }
        }
        // -5 or -.5: a negative number, so a word, as no option here looks like one.
        let mut chars = arg.chars().skip(1).peekable();
        if chars.peek() == Some(&'.') {
            chars.next();
        }
        if chars.next().is_some_and(|c| c.is_ascii_digit()) || arg.contains(' ') {
            return Ok(Word::Positional);
        }
        Ok(Word::Unknown)
    }

    /// The flag of o that a word names.
    fn flag_of(&self, o: &Opt, name: &str) -> &'static str {
        o.flags
            .iter()
            .copied()
            .find(|f| *f == name)
            .unwrap_or(o.flags[0])
    }

    pub fn parse(&self, argv: &[String]) -> Result<Args, Stop> {
        let mut args = Args::default();
        let mut words: Vec<String> = Vec::new();
        let mut extras: Vec<String> = Vec::new();
        let mut i = 0;
        let mut only_words = false;
        while i < argv.len() {
            let arg = argv[i].clone();
            i += 1;
            if only_words {
                words.push(arg);
                continue;
            }
            if arg == "--" {
                only_words = true;
                continue;
            }
            let (mut opt, mut flag, mut explicit, mut sep) =
                match self.classify(&arg).map_err(|m| self.error(&m))? {
                    Word::Positional => {
                        words.push(arg);
                        continue;
                    }
                    Word::Unknown => {
                        extras.push(arg);
                        continue;
                    }
                    Word::Opt(o, f, e, s) => (o, f, e, s),
                };
            // As consume_optional: -xyz is -x -y -z when they take nothing.
            loop {
                if opt.dest == "help" {
                    return Err(Stop::Help(self.help()));
                }
                let names = opt.flags.join("/");
                match explicit.take() {
                    Some(exp) if opt.takes == Takes::Nothing => {
                        if flag.starts_with("--")
                            || exp.is_empty()
                            || sep == "="
                            || exp.starts_with('-')
                        {
                            return Err(self.error(&format!(
                                "argument {names}: ignored explicit argument '{exp}'"
                            )));
                        }
                        *args.flags.entry(opt.dest).or_default() += 1;
                        let first: String = exp.chars().take(1).collect();
                        let next = format!("-{first}");
                        match self.find(&next) {
                            Some(o) => {
                                opt = o;
                                flag = self.flag_of(o, &next);
                                let rest = &exp[first.len()..];
                                (explicit, sep) = if rest.is_empty() {
                                    (None, "")
                                } else if let Some(r) = rest.strip_prefix('=') {
                                    (Some(r.to_string()), "=")
                                } else {
                                    (Some(rest.to_string()), "")
                                };
                                continue;
                            }
                            None => {
                                extras.push(format!("-{exp}"));
                                break;
                            }
                        }
                    }
                    Some(exp) => {
                        args.values.entry(opt.dest).or_default().push(exp);
                        break;
                    }
                    None if opt.takes == Takes::Nothing => {
                        *args.flags.entry(opt.dest).or_default() += 1;
                        break;
                    }
                    None => {
                        let next_is_word = i < argv.len()
                            && !only_words
                            && argv[i] != "--"
                            && matches!(self.classify(&argv[i]), Ok(Word::Positional));
                        if !next_is_word {
                            return Err(
                                self.error(&format!("argument {names}: expected one argument"))
                            );
                        }
                        args.values
                            .entry(opt.dest)
                            .or_default()
                            .push(argv[i].clone());
                        i += 1;
                        break;
                    }
                }
            }
        }
        // Hand the words to the positionals in order.
        let mut words = words.into_iter().peekable();
        let needed: Vec<usize> = (0..self.pos.len())
            .map(|k| {
                self.pos[k + 1..]
                    .iter()
                    .filter(|p| p.count == Count::One)
                    .count()
            })
            .collect();
        let mut missing = Vec::new();
        for (k, p) in self.pos.iter().enumerate() {
            let left = words.len();
            match p.count {
                Count::One => match words.next() {
                    Some(w) => args.values.entry(p.name).or_default().push(w),
                    None => missing.push(p.name),
                },
                Count::Optional => {
                    if left > needed[k] {
                        let w = words.next().unwrap();
                        args.values.entry(p.name).or_default().push(w);
                    }
                }
                Count::Any => {
                    let n = left.saturating_sub(needed[k]);
                    for _ in 0..n {
                        let w = words.next().unwrap();
                        args.values.entry(p.name).or_default().push(w);
                    }
                }
            }
        }
        if !missing.is_empty() {
            return Err(self.error(&format!(
                "the following arguments are required: {}",
                missing.join(", ")
            )));
        }
        extras.extend(words);
        if !extras.is_empty() {
            return Err(self.error(&format!("unrecognized arguments: {}", extras.join(" "))));
        }
        Ok(args)
    }

    fn find(&self, flag: &str) -> Option<&Opt> {
        self.opts.iter().find(|o| o.flags.contains(&flag))
    }

    fn opt_usage(o: &Opt) -> String {
        let flag = o.flags[0];
        match o.takes {
            Takes::Nothing => format!("[{flag}]"),
            Takes::Value => format!("[{flag} {}]", o.metavar),
        }
    }

    fn pos_usage(p: &Pos) -> String {
        let name = p.metavar.unwrap_or(p.name);
        match p.count {
            Count::One => name.to_string(),
            Count::Optional => format!("[{name}]"),
            Count::Any => format!("[{name} ...]"),
        }
    }

    /// argparse's usage line, wrapped as it wraps it.
    pub fn usage(&self) -> String {
        let opt_parts: Vec<String> = self
            .opts
            .iter()
            .filter(|o| !o.hidden)
            .map(Parser::opt_usage)
            .collect();
        let pos_parts: Vec<String> = self.pos.iter().map(Parser::pos_usage).collect();
        let width = width();
        let prefix = "usage: ";
        let usage = std::iter::once(self.prog.clone())
            .chain(opt_parts.iter().cloned())
            .chain(pos_parts.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        if prefix.len() + usage.len() <= width {
            return format!("{prefix}{usage}\n");
        }
        // As argparse's get_lines: parts packed into lines under the first one's words.
        let get_lines = |parts: &[String], indent: &str, with_prefix: bool| -> Vec<String> {
            let mut lines: Vec<String> = Vec::new();
            let mut line: Vec<&str> = Vec::new();
            let mut line_len = if with_prefix {
                prefix.len() - 1
            } else {
                indent.len() - 1
            };
            for part in parts {
                if line_len + 1 + part.len() > width && !line.is_empty() {
                    lines.push(format!("{indent}{}", line.join(" ")));
                    line.clear();
                    line_len = indent.len() - 1;
                }
                line.push(part);
                line_len += part.len() + 1;
            }
            if !line.is_empty() {
                lines.push(format!("{indent}{}", line.join(" ")));
            }
            if with_prefix && !lines.is_empty() {
                lines[0] = lines[0][indent.len()..].to_string();
            }
            lines
        };
        let mut lines: Vec<String>;
        if (prefix.len() + self.prog.len()) as f64 <= 0.75 * width as f64 {
            let indent = " ".repeat(prefix.len() + self.prog.len() + 1);
            let first: Vec<String> = std::iter::once(self.prog.clone())
                .chain(if opt_parts.is_empty() {
                    pos_parts.clone()
                } else {
                    opt_parts.clone()
                })
                .collect();
            lines = get_lines(&first, &indent, true);
            if !opt_parts.is_empty() {
                lines.extend(get_lines(&pos_parts, &indent, false));
            }
        } else {
            let indent = " ".repeat(prefix.len());
            lines = vec![self.prog.clone()];
            lines.extend(get_lines(&opt_parts, &indent, false));
            lines.extend(get_lines(&pos_parts, &indent, false));
        }
        format!("{prefix}{}\n", lines.join("\n"))
    }

    fn invocation(o: &Opt) -> String {
        match o.takes {
            Takes::Nothing => o.flags.join(", "),
            Takes::Value => format!("{} {}", o.flags.join(", "), o.metavar),
        }
    }

    /// argparse's --help: usage, description, the positionals and the options, then the epilog.
    pub fn help(&self) -> String {
        let width = width();
        let mut out = self.usage() + "\n";
        if !self.description.is_empty() {
            out += &wrap(&self.description, width, "", "").join("\n");
            out += "\n\n";
        }
        let entries = |items: Vec<(String, &str)>| -> String {
            let longest = items
                .iter()
                .map(|(inv, _)| inv.len() + 2)
                .max()
                .unwrap_or(0);
            let help_at = (longest + 2).min(24.min(width.saturating_sub(20).max(4)));
            let help_width = width.saturating_sub(help_at).max(11);
            let mut text = String::new();
            for (inv, help) in items {
                let lines = wrap(help, help_width, "", "");
                if help.is_empty() {
                    text += &format!("  {inv}\n");
                } else if inv.len() + 2 <= help_at - 2 {
                    text += &format!("  {inv:<w$}  {}\n", lines[0], w = help_at - 4);
                    for l in &lines[1..] {
                        text += &format!("{}{l}\n", " ".repeat(help_at));
                    }
                } else {
                    text += &format!("  {inv}\n");
                    for l in &lines {
                        text += &format!("{}{l}\n", " ".repeat(help_at));
                    }
                }
            }
            text
        };
        if !self.pos.is_empty() {
            out += "positional arguments:\n";
            out += &entries(
                self.pos
                    .iter()
                    .map(|p| (p.metavar.unwrap_or(p.name).to_string(), p.help.as_str()))
                    .collect(),
            );
            out += "\n";
        }
        out += "options:\n";
        out += &entries(
            self.opts
                .iter()
                .filter(|o| !o.hidden)
                .map(|o| (Parser::invocation(o), o.help.as_str()))
                .collect(),
        );
        if !self.epilog.is_empty() {
            out += "\n";
            out += &wrap(&self.epilog, width, "", "").join("\n");
            out += "\n";
        }
        out
    }
}

/// textwrap.wrap(text, width) after argparse folds the whitespace: words packed greedily.
pub fn wrap(text: &str, width: usize, first: &str, rest: &str) -> Vec<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut lines = Vec::new();
    let mut line = first.to_string();
    let mut empty = true;
    for w in words {
        let w_len = w.chars().count();
        if !empty && line.chars().count() + 1 + w_len > width {
            lines.push(line);
            line = rest.to_string();
            empty = true;
        }
        if !empty {
            line.push(' ');
        }
        line.push_str(w);
        empty = false;
    }
    if !empty || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|s| s.to_string()).collect()
    }

    fn parser() -> Parser {
        Parser::new("hatcheck", "List recent sessions.")
            .positional("hours", Count::Optional, "how far back to look")
            .option(
                &["-n", "--per-folder"],
                "per_folder",
                "N",
                "show only the newest N",
            )
            .flag(&["--all"], "all", "also list Desktop sessions")
            .flag(&["--all-folders"], "all_folders", "list every folder")
            .flag(&["--no-usage"], "no_usage", "leave plan usage out")
    }

    #[test]
    fn reads_what_argparse_reads() {
        let a = parser()
            .parse(&argv(&["--no-u", "48", "-n2", "--all-folders"]))
            .ok()
            .unwrap();
        assert!(a.flag("no_usage") && a.flag("all_folders") && !a.flag("all"));
        assert_eq!(
            (a.value("hours"), a.value("per_folder")),
            (Some("48"), Some("2"))
        );
        let a = parser().parse(&argv(&["--per-folder=3"])).ok().unwrap();
        assert_eq!(a.value("per_folder"), Some("3"));
        assert!(
            matches!(parser().parse(&argv(&["--al"])), Err(Stop::Error(m)) if m.contains("ambiguous option: --al could match --all, --all-folders"))
        );
        assert!(
            matches!(parser().parse(&argv(&["1", "2"])), Err(Stop::Error(m)) if m.contains("unrecognized arguments: 2"))
        );
        assert!(
            matches!(parser().parse(&argv(&["-n"])), Err(Stop::Error(m)) if m.contains("argument -n/--per-folder: expected one argument"))
        );
        assert!(
            matches!(parser().parse(&argv(&["--help"])), Err(Stop::Help(h)) if h.starts_with("usage: hatcheck [-h]"))
        );
    }
}
