//! Output as the Python version wrote it: text to stdout and stderr (a closed pipe ends the
//! program quietly), colours only on a terminal that takes them, and the terminal's width.

use std::cell::RefCell;
use std::io::{IsTerminal, Write};

thread_local! {
    /// While hatcheck serve runs a command: what it writes to stdout and to stderr.
    static CAPTURE: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// Run f with what it prints kept, not written: (its result, its stdout, its stderr).
/// Captures nest, as redirect_stdout does: move --json under hatcheck serve sets its text
/// aside and prints its report into serve's capture, not past it.
pub fn captured<T>(f: impl FnOnce() -> T) -> (T, String, String) {
    /// Puts the outer capture back, even when f panics.
    struct Outer(Option<Option<(String, String)>>);
    impl Drop for Outer {
        fn drop(&mut self) {
            if let Some(outer) = self.0.take() {
                CAPTURE.with(|c| *c.borrow_mut() = outer);
            }
        }
    }
    let mut outer =
        Outer(Some(CAPTURE.with(|c| {
            c.borrow_mut().replace((String::new(), String::new()))
        })));
    let result = f();
    let mine = CAPTURE.with(|c| std::mem::replace(&mut *c.borrow_mut(), outer.0.take().flatten()));
    let (out, err) = mine.unwrap_or_default();
    (result, out, err)
}

/// Keep text for the command hatcheck serve is running: true when there is one.
fn keep(text: &str, to_err: bool) -> bool {
    CAPTURE.with(|c| match c.borrow_mut().as_mut() {
        Some((out, err)) => {
            if to_err { err } else { out }.push_str(text);
            true
        }
        None => false,
    })
}

/// Whether what's printed is being kept for hatcheck serve, not written.
pub fn capturing() -> bool {
    CAPTURE.with(|c| c.borrow().is_some())
}

fn write_to(mut w: impl Write, text: &str) {
    if let Err(e) = w.write_all(text.as_bytes()).and_then(|()| w.flush()) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0); // the reader went away, as with | head
        }
    }
}

/// Write text to stdout as it is.
pub fn print(text: &str) {
    if !keep(text, false) {
        write_to(std::io::stdout().lock(), text);
    }
}

/// Python's print(text): the text and a newline.
pub fn println(text: &str) {
    let mut s = String::with_capacity(text.len() + 1);
    s.push_str(text);
    s.push('\n');
    print(&s);
}

pub fn eprint(text: &str) {
    if !keep(text, true) {
        write_to(std::io::stderr().lock(), text);
    }
}

/// An error on stderr, with its first line red on an error terminal that takes colours.
pub fn error(text: &str) {
    eprint(&Paint::stderr().error(text));
}

/// A question on stderr, bold on an error terminal that takes colours.
pub fn prompt(text: &str) {
    eprint(&Paint::stderr().prompt(text));
}

/// sys.exit("message"): the message on stderr, and exit code 1.
pub fn fail(message: &str) -> i32 {
    error(&format!("{message}\n"));
    1
}

/// A refusal or cancellation, not an operational failure: the message on stderr, and exit code 1.
pub fn cancel(message: &str) -> i32 {
    eprint(&format!("{}\n", Paint::stderr().muted(message)));
    1
}

/// shutil.get_terminal_size((fallback, 24)).columns: $COLUMNS, the terminal's, or fallback.
pub fn columns(fallback: usize) -> usize {
    if let Some(n) = std::env::var("COLUMNS")
        .ok()
        .and_then(|c| c.trim().parse::<i64>().ok())
    {
        if n > 0 {
            return n as usize;
        }
    }
    match terminal_size::terminal_size_of(std::io::stdout()) {
        Some((terminal_size::Width(w), _)) if w > 0 => w as usize,
        _ => fallback,
    }
}

/// On Windows, turn on one standard stream's handling of ANSI colours: whether colours will work.
/// Rust writes UTF-8 to the console already.
fn setup_stream(error: bool) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
        };
        unsafe {
            let handle = GetStdHandle(if error {
                STD_ERROR_HANDLE
            } else {
                STD_OUTPUT_HANDLE
            });
            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return false; // not a console: output is going to a file or pipe
            }
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }
    #[cfg(not(windows))]
    {
        let _ = error;
        true
    }
}

/// On Windows, turn on stdout's handling of ANSI colours: whether colours will work there.
pub fn setup_console() -> bool {
    setup_stream(false)
}

/// Colours for text on a terminal that shows them, with NO_COLOR and output capture respected.
#[derive(Clone, Copy)]
pub struct Paint {
    pub on: bool,
    /// The terminal shows 256 colours, not only the basic 16.
    pub wide: bool,
}

impl Paint {
    pub fn new() -> Paint {
        Paint::for_stream(false)
    }

    pub fn stderr() -> Paint {
        Paint::for_stream(true)
    }

    fn for_stream(error: bool) -> Paint {
        let terminal = if error {
            std::io::stderr().is_terminal()
        } else {
            std::io::stdout().is_terminal()
        };
        Paint {
            on: terminal
                && !capturing()
                && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
                && setup_stream(error),
            wide: crate::usage::has_256_colors(),
        }
    }

    /// The text in an SGR colour such as "1;36", or as it is without colours or a code.
    pub fn p(&self, code: &str, text: &str) -> String {
        if self.on && !code.is_empty() {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    fn first(&self, code: &str, text: &str) -> String {
        match text.split_once('\n') {
            Some((first, rest)) => format!("{}\n{rest}", self.p(code, first)),
            None => self.p(code, text),
        }
    }

    fn status(&self, symbol: &str, code: &str, text: &str) -> String {
        match text.split_once('\n') {
            Some((first, rest)) => {
                format!("{}\n{rest}", self.p(code, &format!("{symbol} {first}")))
            }
            None => self.p(code, &format!("{symbol} {text}")),
        }
    }

    pub fn heading(&self, text: &str) -> String {
        self.p("1", text)
    }

    pub fn accent(&self, text: &str) -> String {
        self.p("1;36", text)
    }

    pub fn command(&self, text: &str) -> String {
        self.p("36", text)
    }

    pub fn muted(&self, text: &str) -> String {
        self.p("2", text)
    }

    pub fn success(&self, text: &str) -> String {
        self.status("✓", "32", text)
    }

    pub fn preview(&self, text: &str) -> String {
        self.status("◇", "34", text)
    }

    pub fn warning(&self, text: &str) -> String {
        self.status("⚠", "33", text)
    }

    pub fn error(&self, text: &str) -> String {
        self.first("31", text)
    }

    pub fn prompt(&self, text: &str) -> String {
        self.p("1", text)
    }
}

impl Default for Paint {
    fn default() -> Self {
        Paint::new()
    }
}

pub fn heading(text: &str) {
    println(&Paint::new().heading(text));
}

pub fn success(text: &str) {
    println(&Paint::new().success(text));
}

pub fn preview(text: &str) {
    println(&Paint::new().preview(text));
}

pub fn warn(text: &str) {
    println(&Paint::new().warning(text));
}

pub fn command(text: &str) {
    println(&Paint::new().command(text));
}

pub fn note(text: &str) {
    println(&Paint::new().muted(text));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(on: bool) -> Paint {
        Paint { on, wide: true }
    }

    #[test]
    fn semantic_styles_keep_the_same_text_without_colour() {
        let on = paint(true);
        let off = paint(false);
        assert_eq!(on.success("Moved"), "\x1b[32m✓ Moved\x1b[0m");
        assert_eq!(on.preview("Would move"), "\x1b[34m◇ Would move\x1b[0m");
        assert_eq!(on.warning("Folder bound"), "\x1b[33m⚠ Folder bound\x1b[0m");
        assert_eq!(on.command("  cl a"), "\x1b[36m  cl a\x1b[0m");
        assert_eq!(on.heading("Sessions"), "\x1b[1mSessions\x1b[0m");
        assert_eq!(on.accent("Best account"), "\x1b[1;36mBest account\x1b[0m");
        assert_eq!(off.success("Moved"), "✓ Moved");
        assert_eq!(off.preview("Would move"), "◇ Would move");
        assert_eq!(off.warning("Folder bound"), "⚠ Folder bound");
        assert_eq!(off.command("  cl a"), "  cl a");
        assert_eq!(off.heading("Sessions"), "Sessions");
        assert_eq!(off.accent("Best account"), "Best account");
    }

    #[test]
    fn errors_colour_only_the_first_line() {
        assert_eq!(
            paint(true).error("Couldn't open it.\n  cd ~/x\n  cl a"),
            "\x1b[31mCouldn't open it.\x1b[0m\n  cd ~/x\n  cl a"
        );
        assert_eq!(
            paint(false).error("Couldn't open it.\n  cl a"),
            "Couldn't open it.\n  cl a"
        );
    }

    #[test]
    fn raw_output_stays_exactly_as_written() {
        let ((), out, err) = captured(|| {
            print("cl a --resume 1\n");
            eprint("usage: cl [profile]\n");
        });
        assert_eq!(out, "cl a --resume 1\n");
        assert_eq!(err, "usage: cl [profile]\n");
    }
}
