//! The command lines Python's argparse read, read the same way: long options and unique
//! prefixes of them, --opt=value, -n1, -abc, --, positionals among the options, exclusive
//! groups, subcommands, -h and --help, and argparse's usage line, help layout and error
//! messages (exit code 2), with text wrapped as textwrap wraps it.

use std::collections::HashMap;

/// What follows an option.
#[derive(Clone, Copy, PartialEq)]
pub enum Takes {
    Nothing,
    Value,
    /// nargs="?": the next word if there's one that isn't an option, else this.
    Maybe(&'static str),
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
    pub choices: Vec<&'static str>,
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
    groups: Vec<Vec<&'static str>>,
    subs: Option<(&'static str, Vec<(&'static str, String)>)>,
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
    /// The words after a subcommand, for its own parser.
    pub rest: Vec<String>,
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

/// argparse's width: the terminal's, less two.
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
                choices: Vec::new(),
            }],
            pos: Vec::new(),
            groups: Vec::new(),
            subs: None,
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
            choices: Vec::new(),
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
            choices: Vec::new(),
        });
        self
    }

    /// An option argparse lists nowhere (help=SUPPRESS) that takes one of choices, or const
    /// when no word follows it (nargs="?").
    pub fn hidden_choice(
        mut self,
        flags: &[&'static str],
        dest: &'static str,
        constant: &'static str,
        choices: &[&'static str],
    ) -> Parser {
        self.opts.push(Opt {
            flags: flags.to_vec(),
            dest,
            takes: Takes::Maybe(constant),
            metavar: "",
            help: String::new(),
            hidden: true,
            choices: choices.to_vec(),
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
            choices: Vec::new(),
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

    /// Options that can't be given together (add_mutually_exclusive_group), by dest.
    pub fn exclusive(mut self, dests: &[&'static str]) -> Parser {
        self.groups.push(dests.to_vec());
        self
    }

    /// Subcommands (add_subparsers): the first word names one, under dest, and the words after
    /// it are left for its own parser, in Args::rest.
    pub fn subcommands(mut self, dest: &'static str, commands: &[(&'static str, &str)]) -> Parser {
        self.subs = Some((
            dest,
            commands.iter().map(|(n, h)| (*n, h.to_string())).collect(),
        ));
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

    fn find(&self, flag: &str) -> Option<&Opt> {
        self.opts.iter().find(|o| o.flags.contains(&flag))
    }

    /// The flag of o that a word names.
    fn flag_of(&self, o: &Opt, name: &str) -> &'static str {
        o.flags
            .iter()
            .copied()
            .find(|f| *f == name)
            .unwrap_or(o.flags[0])
    }

    /// What a word is to argparse's _parse_optional: a word for the positionals, an option it
    /// doesn't know, or an option with what came attached to it (`=value` or `-n5`).
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
        // -5 or -.5 is a negative number, so a word, as no option here looks like one; and a
        // word with a space in it was meant as a word.
        let mut chars = arg.chars().skip(1).peekable();
        if chars.peek() == Some(&'.') {
            chars.next();
        }
        if chars.next().is_some_and(|c| c.is_ascii_digit()) || arg.contains(' ') {
            return Ok(Word::Positional);
        }
        Ok(Word::Unknown)
    }

    /// argparse's "not allowed with" for a second option from the same exclusive group.
    fn conflicts(&self, opt: &Opt, seen: &mut Vec<(usize, &'static str)>) -> Result<(), Stop> {
        for (g, group) in self.groups.iter().enumerate() {
            if !group.contains(&opt.dest) {
                continue;
            }
            if let Some((_, other)) = seen.iter().find(|(k, d)| *k == g && *d != opt.dest) {
                let other = self
                    .opts
                    .iter()
                    .find(|o| o.dest == *other)
                    .map_or(String::new(), |o| o.flags.join("/"));
                return Err(self.error(&format!(
                    "argument {}: not allowed with argument {other}",
                    opt.flags.join("/")
                )));
            }
            seen.push((g, opt.dest));
        }
        Ok(())
    }

    pub fn parse(&self, argv: &[String]) -> Result<Args, Stop> {
        let (args, extras) = self.parse_known(argv)?;
        if !extras.is_empty() {
            return Err(self.error(&format!("unrecognized arguments: {}", extras.join(" "))));
        }
        Ok(args)
    }

    /// parse_known_args: what the words said, and the words it didn't know, which a
    /// subcommand's parser leaves to its parent to report.
    pub fn parse_known(&self, argv: &[String]) -> Result<(Args, Vec<String>), Stop> {
        let mut seen_in_group: Vec<(usize, &'static str)> = Vec::new();
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
            let word = self.classify(&arg).map_err(|m| self.error(&m))?;
            let (mut opt, mut flag, mut explicit, mut sep) = match word {
                Word::Positional => {
                    if let Some((dest, commands)) = &self.subs {
                        if !commands.iter().any(|c| c.0 == arg) {
                            let choices = commands
                                .iter()
                                .map(|c| format!("'{}'", c.0))
                                .collect::<Vec<_>>()
                                .join(", ");
                            return Err(self.error(&format!(
                                "argument {dest}: invalid choice: '{arg}' (choose from {choices})"
                            )));
                        }
                        args.values.entry(dest).or_default().push(arg);
                        args.rest = argv[i..].to_vec();
                        break;
                    }
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
                self.conflicts(opt, &mut seen_in_group)?;
                let names = opt.flags.join("/");
                let choose = |value: String| -> Result<String, Stop> {
                    if opt.choices.is_empty() || opt.choices.contains(&value.as_str()) {
                        return Ok(value);
                    }
                    let choices = opt
                        .choices
                        .iter()
                        .map(|c| format!("'{c}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    Err(self.error(&format!(
                        "argument {}: invalid choice: '{value}' (choose from {choices})",
                        opt.flags.join("/")
                    )))
                };
                if let Takes::Maybe(constant) = opt.takes {
                    let value = match explicit.take() {
                        Some(exp) => exp,
                        None if i < argv.len()
                            && argv[i] != "--"
                            && matches!(self.classify(&argv[i]), Ok(Word::Positional)) =>
                        {
                            i += 1;
                            argv[i - 1].clone()
                        }
                        None => constant.to_string(),
                    };
                    let value = choose(value)?;
                    args.values.entry(opt.dest).or_default().push(value);
                    break;
                }
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
                        let w = words.next().unwrap_or_default();
                        args.values.entry(p.name).or_default().push(w);
                    }
                }
                Count::Any => {
                    for _ in 0..left.saturating_sub(needed[k]) {
                        let w = words.next().unwrap_or_default();
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
        Ok((args, extras))
    }

    fn opt_usage(o: &Opt) -> String {
        let flag = o.flags[0];
        match o.takes {
            Takes::Nothing => format!("[{flag}]"),
            Takes::Value => format!("[{flag} {}]", o.metavar),
            Takes::Maybe(_) => format!("[{flag} [{}]]", o.metavar),
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

    /// The usage parts of the options: one each, and for an exclusive group, where its first
    /// option comes, one per member, "[--a |" and "--b]", as Python 3.14 has them, so a line
    /// may break inside the group.
    fn opt_parts(&self) -> Vec<String> {
        let mut parts = Vec::new();
        let mut done: Vec<usize> = Vec::new();
        for o in self.opts.iter().filter(|o| !o.hidden) {
            match self.groups.iter().position(|g| g.contains(&o.dest)) {
                Some(g) if done.contains(&g) => {}
                Some(g) => {
                    done.push(g);
                    let members: Vec<String> = self.groups[g]
                        .iter()
                        .filter_map(|d| self.opts.iter().find(|o| o.dest == *d))
                        .map(|o| {
                            let u = Parser::opt_usage(o);
                            u[1..u.len() - 1].to_string()
                        })
                        .collect();
                    let last = members.len().saturating_sub(1);
                    for (i, m) in members.into_iter().enumerate() {
                        let open = if i == 0 { "[" } else { "" };
                        let close = if i == last { "]" } else { " |" };
                        parts.push(format!("{open}{m}{close}"));
                    }
                }
                None => parts.push(Parser::opt_usage(o)),
            }
        }
        parts
    }

    fn pos_parts(&self) -> Vec<String> {
        let mut parts: Vec<String> = self.pos.iter().map(Parser::pos_usage).collect();
        if let Some((_, commands)) = &self.subs {
            parts.push(format!(
                "{{{}}} ...",
                commands.iter().map(|c| c.0).collect::<Vec<_>>().join(",")
            ));
        }
        parts
    }

    /// argparse's usage line, wrapped as it wraps it.
    pub fn usage(&self) -> String {
        let (opt_parts, pos_parts) = (self.opt_parts(), self.pos_parts());
        let width = width();
        let prefix = "usage: ";
        let usage = std::iter::once(self.prog.clone())
            .chain(opt_parts.iter().cloned())
            .chain(pos_parts.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ");
        if prefix.len() + usage.chars().count() <= width {
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
                if line_len + 1 + part.chars().count() > width && !line.is_empty() {
                    lines.push(format!("{indent}{}", line.join(" ")));
                    line.clear();
                    line_len = indent.len() - 1;
                }
                line.push(part);
                line_len += part.chars().count() + 1;
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
            Takes::Maybe(_) => format!("{} [{}]", o.flags.join(", "), o.metavar),
        }
    }

    /// argparse's --help: usage, description, the positionals and the options, then the epilog,
    /// laid out as its HelpFormatter lays them out: one help column for all of them.
    pub fn help(&self) -> String {
        let width = width();
        let mut out = self.usage() + "\n";
        if !self.description.is_empty() {
            out += &wrap(&self.description, width, "", "").join("\n");
            out += "\n\n";
        }
        // (indent, invocation, help) for every entry; subcommands sit two further in.
        let mut positional: Vec<(usize, String, &str)> = Vec::new();
        if let Some((_, commands)) = &self.subs {
            positional.push((
                2,
                format!(
                    "{{{}}}",
                    commands.iter().map(|c| c.0).collect::<Vec<_>>().join(",")
                ),
                "",
            ));
            positional.extend(
                commands
                    .iter()
                    .map(|(name, help)| (4, name.to_string(), help.as_str())),
            );
        }
        positional.extend(
            self.pos
                .iter()
                .map(|p| (2, p.metavar.unwrap_or(p.name).to_string(), p.help.as_str())),
        );
        let options: Vec<(usize, String, &str)> = self
            .opts
            .iter()
            .filter(|o| !o.hidden)
            .map(|o| (2, Parser::invocation(o), o.help.as_str()))
            .collect();
        let longest = positional
            .iter()
            .chain(&options)
            .map(|(indent, inv, _)| inv.chars().count() + indent)
            .max()
            .unwrap_or(0);
        let max_help = 24.min(width.saturating_sub(20).max(4));
        let help_at = (longest + 2).min(max_help);
        let help_width = width.saturating_sub(help_at).max(11);
        let entries = |items: &[(usize, String, &str)]| -> String {
            let mut text = String::new();
            for (indent, inv, help) in items {
                let pad = " ".repeat(*indent);
                if help.is_empty() {
                    text += &format!("{pad}{inv}\n");
                    continue;
                }
                let lines = wrap(help, help_width, "", "");
                let action_width = help_at.saturating_sub(indent + 2);
                if inv.chars().count() <= action_width {
                    text += &format!("{pad}{inv:<action_width$}  {}\n", lines[0]);
                    for l in &lines[1..] {
                        text += &format!("{}{l}\n", " ".repeat(help_at));
                    }
                } else {
                    text += &format!("{pad}{inv}\n");
                    for l in &lines {
                        text += &format!("{}{l}\n", " ".repeat(help_at));
                    }
                }
            }
            text
        };
        if !positional.is_empty() {
            out += "positional arguments:\n";
            out += &entries(&positional);
            out += "\n";
        }
        out += "options:\n";
        out += &entries(&options);
        if !self.epilog.is_empty() {
            out += "\n";
            out += &wrap(&self.epilog, width, "", "").join("\n");
            out += "\n";
        }
        out
    }
}

/// \w in a Python str pattern: letters, digits and the underscore.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// textwrap's letter, [^\d\W]: a word character that isn't a digit.
fn is_letter(c: char) -> bool {
    is_word(c) && !c.is_numeric()
}

/// textwrap's word punctuation, [\w!"'&.,?].
fn is_word_punct(c: char) -> bool {
    is_word(c) || "!\"'&.,?".contains(c)
}

/// The chunks textwrap.TextWrapper splits text into: runs of spaces, words, the parts of a
/// hyphenated word (after each hyphen it may break at), and em-dashes between words.
fn chunks(text: &str) -> Vec<String> {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let at = |i: usize| -> Option<char> { c.get(i).copied() };
    let space = |i: usize| at(i).is_some_and(|ch| ch == ' ');
    // An em-dash at i: two or more hyphens after word punctuation, before a word character.
    let em_dash = |i: usize| -> Option<usize> {
        if i == 0 || !is_word_punct(c[i - 1]) {
            return None;
        }
        let mut j = i;
        while at(j) == Some('-') {
            j += 1;
        }
        (j - i >= 2 && at(j).is_some_and(is_word)).then_some(j)
    };
    let mut out = Vec::new();
    let mut p = 0;
    while p < n {
        if space(p) {
            let mut q = p;
            while space(q) {
                q += 1;
            }
            out.push(c[p..q].iter().collect());
            p = q;
            continue;
        }
        if let Some(q) = em_dash(p) {
            out.push(c[p..q].iter().collect());
            p = q;
            continue;
        }
        // A word, as short as it can be: up to a hyphen it may break after, the end of the
        // word, or an em-dash.
        let mut k = p + 1;
        let end = loop {
            if k >= n || space(k) {
                break k;
            }
            if c[k] == '-' {
                let behind = (k >= 2 && is_letter(c[k - 2]) && is_letter(c[k - 1]))
                    || (k >= 3 && is_letter(c[k - 3]) && c[k - 2] == '-' && is_letter(c[k - 1]));
                let ahead = at(k + 1).is_some_and(is_letter)
                    && (at(k + 2).is_some_and(is_letter)
                        || (at(k + 2) == Some('-') && at(k + 3).is_some_and(is_letter)));
                if behind && ahead {
                    break k + 1;
                }
                if em_dash(k).is_some() {
                    break k;
                }
            }
            k += 1;
        };
        out.push(c[p..end].iter().collect());
        p = end;
    }
    out
}

/// textwrap.wrap(text, width) after argparse folds the text's whitespace: the first line
/// starting with first and the rest with rest.
pub fn wrap(text: &str, width: usize, first: &str, rest: &str) -> Vec<String> {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut pending: Vec<String> = chunks(&folded);
    pending.reverse();
    let mut lines: Vec<String> = Vec::new();
    let len = |s: &str| s.chars().count();
    while !pending.is_empty() {
        let indent = if lines.is_empty() { first } else { rest };
        let room = width.saturating_sub(len(indent)).max(1);
        if !lines.is_empty() && pending.last().is_some_and(|c| c.trim().is_empty()) {
            pending.pop();
        }
        let mut line: Vec<String> = Vec::new();
        let mut used = 0;
        while let Some(chunk) = pending.last() {
            if used + len(chunk) <= room {
                used += len(chunk);
                line.push(pending.pop().unwrap_or_default());
            } else {
                break;
            }
        }
        if let Some(chunk) = pending.last().cloned().filter(|c| len(c) > room) {
            // A word longer than a line: broken, after a hyphen if there's one in reach.
            let space_left = room.saturating_sub(used).max(1);
            let chars: Vec<char> = chunk.chars().collect();
            let mut cut = space_left.min(chars.len());
            if chars.len() > space_left {
                if let Some(h) = chars[..space_left].iter().rposition(|ch| *ch == '-') {
                    if h > 0 && chars[..h].iter().any(|ch| *ch != '-') {
                        cut = h + 1;
                    }
                }
            }
            line.push(chars[..cut].iter().collect());
            *pending.last_mut().unwrap() = chars[cut..].iter().collect();
            if pending.last().is_some_and(|c| c.is_empty()) {
                pending.pop();
            }
        }
        if line.last().is_some_and(|c| c.trim().is_empty()) {
            line.pop();
        }
        if !line.is_empty() {
            lines.push(format!("{indent}{}", line.concat()));
        }
    }
    if lines.is_empty() {
        lines.push(first.to_string());
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

    #[test]
    fn wraps_as_textwrap_does() {
        assert_eq!(
            chunks("a well-known --all-folders e.g. 5-hour"),
            vec!["a", " ", "well-", "known", " ", "--all-", "folders", " ", "e.g.", " ", "5-hour"]
        );
        assert_eq!(chunks("word--word"), vec!["word", "--", "word"]);
        assert_eq!(
            wrap("the per-model limits are listed", 12, "", ""),
            vec!["the per-", "model limits", "are listed"]
        );
        assert_eq!(
            wrap("abcdefghijkl", 5, "", ""),
            vec!["abcde", "fghij", "kl"]
        );
    }
}
