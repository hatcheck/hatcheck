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

/// sys.exit("message"): the message on stderr, and exit code 1.
pub fn fail(message: &str) -> i32 {
    eprint(&format!("{message}\n"));
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

/// On Windows, turn on the console's handling of ANSI colours: whether colours will work.
/// Rust writes UTF-8 to the console already.
pub fn setup_console() -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Console::{
            GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
            STD_OUTPUT_HANDLE,
        };
        unsafe {
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            let mut mode = 0;
            if GetConsoleMode(handle, &mut mode) == 0 {
                return false; // not a console: output is going to a file or pipe
            }
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
        }
    }
    #[cfg(not(windows))]
    {
        true
    }
}

/// Colours for the text, when stdout is a terminal that shows them and NO_COLOR isn't set.
#[derive(Clone, Copy)]
pub struct Paint {
    pub on: bool,
    /// The terminal shows 256 colours, not only the basic 16.
    pub wide: bool,
}

impl Paint {
    pub fn new() -> Paint {
        let ansi = setup_console();
        Paint {
            on: std::io::stdout().is_terminal()
                && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
                && ansi,
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
}

impl Default for Paint {
    fn default() -> Self {
        Paint::new()
    }
}
