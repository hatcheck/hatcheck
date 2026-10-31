//! hatcheck desktop on Windows: what docs/desktop-instances.md, "On Windows", describes. An
//! instance's launcher is a Start menu shortcut, `Claude DISPLAY.lnk`, that runs Claude Desktop
//! with the instance's `--user-data-dir`; its icon is an `.ico` of Claude's own artwork,
//! recoloured, kept in hatcheck's config folder so a Claude update, which moves the package's
//! folder, doesn't take it away.
//!
//! Claude Desktop on Windows keeps one process per data folder itself: a second launch on a
//! folder that has one starts nothing (checked with the Microsoft Store package 4.4212), so the
//! shortcut needs none of the macOS launcher's lock or focus script.
//!
//! The readers below (shortcuts, command lines, the Store app's alias, icons) are plain
//! functions, built and tested on every system; only what asks Windows itself is cfg(windows).

use crate::ospath;

/// The Description a shortcut of ours carries, which names its instance.
const DESCRIPTION_PREFIX: &str = "Claude Desktop instance ";
const DESCRIPTION_SUFFIX: &str = ", made by hatcheck";

pub fn description(name: &str) -> String {
    format!("{DESCRIPTION_PREFIX}{name}{DESCRIPTION_SUFFIX}")
}

/// The instance a shortcut's Description names, when hatcheck made it.
pub fn instance_of(description: &str) -> Option<&str> {
    description
        .strip_prefix(DESCRIPTION_PREFIX)?
        .strip_suffix(DESCRIPTION_SUFFIX)
        .filter(|n| !n.is_empty())
}

/// What a shortcut (.lnk) holds that hatcheck reads back: its Description, Arguments and icon.
#[derive(Debug, Default, PartialEq)]
pub struct Shortcut {
    pub description: Option<String>,
    pub arguments: Option<String>,
    pub icon: Option<String>,
}

/// A shortcut's strings, read as MS-SHLLINK lays them out: the 76-byte header, then the target's
/// ID list and link info when its flags say they're there, then the strings in a fixed order,
/// each a count of characters and the characters.
pub fn read_shortcut(data: &[u8]) -> Option<Shortcut> {
    const HAS_ID_LIST: u32 = 0x1;
    const HAS_LINK_INFO: u32 = 0x2;
    const IS_UNICODE: u32 = 0x80;
    let u16_at = |at: usize| Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?));
    if u32_at(0)? != 0x4C {
        return None;
    }
    let flags = u32_at(20)?;
    let mut at = 0x4C;
    if flags & HAS_ID_LIST != 0 {
        at += 2 + usize::from(u16_at(at)?);
    }
    if flags & HAS_LINK_INFO != 0 {
        at += u32_at(at)? as usize;
    }
    let mut strings: [Option<String>; 5] = Default::default();
    // NAME, RELATIVE_PATH, WORKING_DIR, COMMAND_LINE_ARGUMENTS, ICON_LOCATION: flags 0x4 to 0x40.
    for (i, slot) in strings.iter_mut().enumerate() {
        if flags & (0x4 << i) == 0 {
            continue;
        }
        let count = usize::from(u16_at(at)?);
        at += 2;
        *slot = Some(if flags & IS_UNICODE != 0 {
            let bytes = data.get(at..at + count * 2)?;
            at += count * 2;
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes(*b))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            let bytes = data.get(at..at + count)?;
            at += count;
            bytes.iter().map(|b| char::from(*b)).collect()
        });
    }
    let [name, _, _, arguments, icon] = strings;
    Some(Shortcut {
        description: name,
        arguments,
        icon,
    })
}

/// A command line split into its arguments as CommandLineToArgvW and the C runtime split one:
/// spaces separate outside quotes; 2n backslashes before a quote are n and the quote opens or
/// closes, 2n+1 are n and a literal quote; other backslashes are themselves.
pub fn split_command_line(line: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let (mut quoted, mut started) = (false, false);
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            let run = chars[i..].iter().take_while(|c| **c == '\\').count();
            if chars.get(i + run) == Some(&'"') {
                current.extend(std::iter::repeat_n('\\', run / 2));
                if run % 2 == 1 {
                    current.push('"');
                    i += run + 1;
                } else {
                    i += run; // the quote is handled next, as a quote
                }
            } else {
                current.extend(std::iter::repeat_n('\\', run));
                i += run;
            }
            started = true;
            continue;
        }
        if c == '"' {
            if quoted && chars.get(i + 1) == Some(&'"') {
                current.push('"'); // "" inside quotes is a literal quote
                i += 2;
                continue;
            }
            quoted = !quoted;
            started = true;
        } else if (c == ' ' || c == '\t') && !quoted {
            if started {
                args.push(std::mem::take(&mut current));
                started = false;
            }
        } else {
            current.push(c);
            started = true;
        }
        i += 1;
    }
    if started {
        args.push(current);
    }
    args
}

/// The folder a command line's `--user-data-dir=` gives, quoted or not.
pub fn user_data_dir(args: &[String]) -> Option<String> {
    args.iter()
        .find_map(|a| a.strip_prefix("--user-data-dir="))
        .map(String::from)
}

/// A shortcut's Arguments for a data folder: the option, the folder quoted.
pub fn arguments_for(data_dir: Option<&str>) -> String {
    data_dir.map_or_else(String::new, |d| format!("--user-data-dir=\"{d}\""))
}

/// Whether a process's image is Claude Desktop's: claude.exe in the Microsoft Store package
/// (`…\WindowsApps\Claude_…\app\claude.exe`) or the older installer's folder
/// (`…\AnthropicClaude\app-…\claude.exe`). Claude Code is a claude.exe too, elsewhere.
pub fn is_claude_desktop(image: &str) -> bool {
    let lower = image.to_lowercase().replace('/', "\\");
    lower.ends_with("\\claude.exe")
        && (lower.contains("\\windowsapps\\claude_") || lower.contains("\\anthropicclaude\\"))
}

/// The data folders Claude Desktop's main processes run on, from (image, command line) pairs:
/// Some(folder), or None for one on Claude's built-in folder. Electron's helpers carry
/// `--type=` and aren't counted.
pub fn running_folders(processes: &[(String, String)]) -> Vec<Option<String>> {
    processes
        .iter()
        .filter(|(image, _)| is_claude_desktop(image))
        .map(|(_, line)| split_command_line(line))
        .filter(|args| !args.iter().any(|a| a.starts_with("--type=")))
        .map(|args| user_data_dir(&args))
        .collect()
}

/// Whether two folders are one, as Windows compares them: case and separators aside.
pub fn same_dir(a: &str, b: &str) -> bool {
    let key = |p: &str| {
        let n = ospath::normpath(&p.replace('/', "\\"));
        n.trim_end_matches(['\\', '/']).to_lowercase()
    };
    key(a) == key(b)
}

/// The target an app execution alias runs, from its reparse data (IO_REPARSE_TAG_APPEXECLINK):
/// the tag, the data's length and a reserved word, a version, then NUL-terminated UTF-16 strings
/// of which the third is the program's path.
pub fn appexeclink_target(buffer: &[u8]) -> Option<String> {
    const APPEXECLINK: u32 = 0x8000_001B;
    if u32::from_le_bytes(buffer.get(0..4)?.try_into().ok()?) != APPEXECLINK {
        return None;
    }
    let units: Vec<u16> = buffer
        .get(12..)?
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| u16::from_le_bytes(*b))
        .collect();
    let target = units.split(|u| *u == 0).nth(2)?;
    (!target.is_empty()).then(|| String::from_utf16_lossy(target))
}

/// The sizes an .ico gets, and the artwork for each in the Store package's assets folder: the
/// unplated app-list icons, which have no tile behind them.
pub const ICON_SIZES: [u32; 5] = [16, 24, 32, 48, 256];

pub fn asset_name(size: u32) -> String {
    format!("Square44x44Logo.targetsize-{size}_altform-unplated.png")
}

/// An .ico holding each image as a PNG, which Windows reads since Vista: a 6-byte header, a
/// 16-byte entry per image (256 is written as 0), then the images.
pub fn ico(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(images.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * images.len();
    for (size, png) in images {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(png.len() as u32).to_le_bytes());
        out.extend_from_slice(&(offset as u32).to_le_bytes());
        offset += png.len();
    }
    for (_, png) in images {
        out.extend_from_slice(png);
    }
    out
}

/// The program an alias at path runs, or None when path isn't an app execution alias.
#[cfg(windows)]
pub fn alias_target(path: &str) -> Option<String> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::IO::DeviceIoControl;
    const FSCTL_GET_REPARSE_POINT: u32 = 0x0009_00A8;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    let file = std::fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .ok()?;
    let mut buffer = vec![0u8; 16 * 1024];
    let mut got = 0u32;
    // SAFETY: the handle is the open file's, and the buffer's length is the size given.
    let ok = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_GET_REPARSE_POINT,
            std::ptr::null(),
            0,
            buffer.as_mut_ptr().cast(),
            buffer.len() as u32,
            &mut got,
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        return None;
    }
    appexeclink_target(&buffer[..got as usize])
}

#[cfg(not(windows))]
pub fn alias_target(_path: &str) -> Option<String> {
    None
}

/// Every claude.exe running, as (image path, command line); one that can't be read is left out.
#[cfg(windows)]
pub fn claude_processes() -> Vec<(String, String)> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    let mut found = Vec::new();
    // SAFETY: the snapshot handle is checked and closed; the entry is a PROCESSENTRY32W with its
    // size set, as Process32FirstW asks.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return found;
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry);
        while more != 0 {
            let end = entry.szExeFile.iter().position(|u| *u == 0).unwrap_or(260);
            let name = String::from_utf16_lossy(&entry.szExeFile[..end]);
            if name.eq_ignore_ascii_case("claude.exe") {
                if let Some(p) = process_details(entry.th32ProcessID) {
                    found.push(p);
                }
            }
            more = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
    }
    found
}

/// A process's image path and command line, through a handle that may only query.
#[cfg(windows)]
fn process_details(pid: u32) -> Option<(String, String)> {
    use windows_sys::Wdk::System::Threading::{
        NtQueryInformationProcess, ProcessCommandLineInformation,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, UNICODE_STRING};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    // SAFETY: the process handle is checked and closed; each buffer's size is the one given, and
    // the command line's UNICODE_STRING points into the buffer it was written to, which is
    // 8-byte aligned (a Vec<u64>) for it.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut image = vec![0u16; 1024];
        let mut len = image.len() as u32;
        let image =
            (QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, image.as_mut_ptr(), &mut len)
                != 0)
                .then(|| String::from_utf16_lossy(&image[..len as usize]));
        // A command line is at most 32767 characters, after its 16-byte header.
        let mut buffer = vec![0u64; (65536 + 64) / 8];
        let mut got = 0u32;
        let status = NtQueryInformationProcess(
            handle,
            ProcessCommandLineInformation,
            buffer.as_mut_ptr().cast(),
            (buffer.len() * 8) as u32,
            &mut got,
        );
        CloseHandle(handle);
        if status < 0 {
            return None;
        }
        let text = &*(buffer.as_ptr() as *const UNICODE_STRING);
        let line = if text.Buffer.is_null() {
            String::new()
        } else {
            String::from_utf16_lossy(std::slice::from_raw_parts(
                text.Buffer,
                usize::from(text.Length) / 2,
            ))
        };
        Some((image?, line))
    }
}

#[cfg(not(windows))]
pub fn claude_processes() -> Vec<(String, String)> {
    Vec::new()
}

/// Make the shortcut at lnk, through Windows' own WScript.Shell, run by Windows PowerShell with
/// the values in its environment so nothing is quoted into the script. True once the file is
/// there and reads back as made: the exit status alone isn't trusted.
pub fn write_shortcut(
    lnk: &str,
    target: &str,
    arguments: &str,
    description: &str,
    icon: Option<&str>,
) -> Result<(), String> {
    const SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
        $s = (New-Object -ComObject WScript.Shell).CreateShortcut($env:HATCHECK_LNK); \
        $s.TargetPath = $env:HATCHECK_LNK_TARGET; \
        $s.Arguments = $env:HATCHECK_LNK_ARGUMENTS; \
        $s.Description = $env:HATCHECK_LNK_DESCRIPTION; \
        if ($env:HATCHECK_LNK_ICON) { $s.IconLocation = $env:HATCHECK_LNK_ICON + ',0' }; \
        $s.Save()";
    let powershell = std::env::var("SystemRoot")
        .ok()
        .map(|root| {
            ospath::join_all(
                &root,
                &["System32", "WindowsPowerShell", "v1.0", "powershell.exe"],
            )
        })
        .filter(|p| ospath::isfile(p))
        .unwrap_or_else(|| "powershell.exe".into());
    let out = std::process::Command::new(&powershell)
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("HATCHECK_LNK", lnk)
        .env("HATCHECK_LNK_TARGET", target)
        .env("HATCHECK_LNK_ARGUMENTS", arguments)
        .env("HATCHECK_LNK_DESCRIPTION", description)
        .env("HATCHECK_LNK_ICON", icon.unwrap_or(""))
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| format!("Couldn't run Windows PowerShell to make the shortcut: {e}"))?;
    let made = std::fs::read(lnk).ok().and_then(|d| read_shortcut(&d));
    match made {
        Some(s) if s.description.as_deref() == Some(description) => Ok(()),
        _ => {
            let why = String::from_utf8_lossy(&out.stderr);
            let why = why.trim();
            Err(if why.is_empty() {
                format!("The shortcut {lnk} wasn't made")
            } else {
                format!("The shortcut {lnk} wasn't made: {why}")
            })
        }
    }
}

/// Open a shortcut as a click on it does, through the shell. Not with Command: a process it
/// starts inherits every inheritable handle of hatcheck's, its stdout pipe among them, so
/// whoever reads hatcheck's output (a terminal's caller, the app's engine) waits until Claude
/// Desktop quits. On a folder with a process already, Claude brings that one forward.
#[cfg(windows)]
pub fn open_shortcut(lnk: &str) -> Result<(), String> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let (verb, file) = (wide("open"), wide(lnk));
    // SAFETY: both strings are NUL-terminated and outlive the call; the rest may be null.
    let code = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    // Above 32 is success; at or below, one of ShellExecute's own error codes.
    if code > 32 {
        Ok(())
    } else {
        Err(format!(
            "Windows couldn't open {lnk} (ShellExecute error {code})"
        ))
    }
}

#[cfg(not(windows))]
pub fn open_shortcut(lnk: &str) -> Result<(), String> {
    Err(format!("{lnk} opens on Windows only"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    /// A shortcut as WScript.Shell lays one out: header, an ID list, link info, then the name,
    /// a relative path, the arguments and the icon.
    fn shortcut_bytes(flags: u32, strings: &[&str]) -> Vec<u8> {
        let mut data = vec![0u8; 0x4C];
        data[0..4].copy_from_slice(&0x4Cu32.to_le_bytes());
        data[20..24].copy_from_slice(&flags.to_le_bytes());
        data.extend_from_slice(&4u16.to_le_bytes());
        data.extend_from_slice(&[9, 9, 9, 9]); // the ID list, skipped
        data.extend_from_slice(&8u32.to_le_bytes());
        data.extend_from_slice(&[7, 7, 7, 7]); // the link info, skipped by its size
        for s in strings {
            data.extend_from_slice(&(s.encode_utf16().count() as u16).to_le_bytes());
            data.extend_from_slice(&utf16(s));
        }
        data
    }

    #[test]
    fn a_shortcut_reads_back() {
        // ID list, link info, name, relative path, arguments, icon, Unicode.
        let flags = 0x1 | 0x2 | 0x4 | 0x8 | 0x20 | 0x40 | 0x80;
        let data = shortcut_bytes(
            flags,
            &[
                &description("work"),
                "..\\x",
                "--user-data-dir=\"C:\\Users\\é\\.claude-instances\\work\"",
                "C:\\i\\Claude Work.ico,0",
            ],
        );
        let s = read_shortcut(&data).unwrap();
        assert_eq!(instance_of(s.description.as_deref().unwrap()), Some("work"));
        assert_eq!(
            user_data_dir(&split_command_line(s.arguments.as_deref().unwrap())).as_deref(),
            Some("C:\\Users\\é\\.claude-instances\\work")
        );
        assert_eq!(s.icon.as_deref(), Some("C:\\i\\Claude Work.ico,0"));
        assert!(read_shortcut(&data[..data.len() - 3]).is_none()); // cut short
        assert!(read_shortcut(b"not a shortcut").is_none());
        assert_eq!(
            instance_of("Claude Desktop instance , made by hatcheck"),
            None
        );
        assert_eq!(instance_of("something else"), None);
    }

    #[test]
    fn command_lines_split_as_windows_splits_them() {
        let split = |l: &str| split_command_line(l);
        assert_eq!(
            split(r#""C:\Program Files\x\claude-desktop.exe" --user-data-dir="C:\a b\c""#),
            [
                "C:\\Program Files\\x\\claude-desktop.exe",
                "--user-data-dir=C:\\a b\\c"
            ]
        );
        assert_eq!(
            split(r#"claude.exe "--user-data-dir=C:\a b" --type=renderer"#),
            ["claude.exe", "--user-data-dir=C:\\a b", "--type=renderer"]
        );
        assert_eq!(split(r#"a\\\"b c\\"d e" f"#), ["a\\\"b", "c\\d e", "f"]);
        assert_eq!(split(r#"a "" b"#), ["a", "", "b"]);
        assert_eq!(split("  "), Vec::<String>::new());
    }

    #[test]
    fn only_claude_desktops_main_processes_count() {
        let store =
            r"C:\Program Files\WindowsApps\Claude_4.4212.0.0_x64__pzs8sxrjxfjjc\app\claude.exe";
        let processes: Vec<(String, String)> = [
            (store, r#""C:\Users\e\AppData\Local\Microsoft\WindowsApps\claude-desktop.exe" --user-data-dir="C:\h\.claude-instances\a""#),
            (store, r#""C:\…\app\claude.exe" --type=renderer --user-data-dir="C:\h\.claude-instances\a""#),
            (store, r#""C:\Program Files\WindowsApps\Claude_4\app\claude.exe" "#),
            (r"C:\Users\e\.local\bin\claude.exe", r#""C:\Users\e\.local\bin\claude.EXE""#),
            (r"C:\Users\e\AppData\Local\AnthropicClaude\app-1.0.0\claude.exe", "claude.exe --user-data-dir=C:\\b"),
        ]
        .iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        assert_eq!(
            running_folders(&processes),
            [
                Some("C:\\h\\.claude-instances\\a".to_string()),
                None,
                Some("C:\\b".to_string())
            ]
        );
        assert!(same_dir(
            "C:\\H\\.claude-instances\\A\\",
            "c:/h/.claude-instances/a"
        ));
        assert!(!same_dir("C:\\h\\a", "C:\\h\\a-b"));
    }

    #[test]
    fn an_alias_names_its_program() {
        let mut data = 0x8000_001Bu32.to_le_bytes().to_vec();
        data.extend_from_slice(&[0, 0, 0, 0]); // length and reserved
        data.extend_from_slice(&3u32.to_le_bytes());
        for s in [
            "Claude_pzs8sxrjxfjjc",
            "Claude_pzs8sxrjxfjjc!Claude",
            r"C:\P\Claude_4\app\Claude.exe",
            "0",
        ] {
            data.extend_from_slice(&utf16(s));
            data.extend_from_slice(&[0, 0]);
        }
        assert_eq!(
            appexeclink_target(&data).as_deref(),
            Some(r"C:\P\Claude_4\app\Claude.exe")
        );
        data[0] = 0x0C; // a symbolic link's tag
        assert_eq!(appexeclink_target(&data), None);
    }

    #[test]
    fn an_ico_holds_its_pngs() {
        let ico = ico(&[(16, vec![1, 2, 3]), (256, vec![4, 5])]);
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 2, 0]);
        assert_eq!(&ico[6..10], &[16, 16, 0, 0]);
        assert_eq!(u32::from_le_bytes(ico[14..18].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(ico[18..22].try_into().unwrap()), 38);
        assert_eq!(&ico[22..26], &[0, 0, 0, 0]); // 256 is written as 0
        assert_eq!(u32::from_le_bytes(ico[34..38].try_into().unwrap()), 41);
        assert_eq!(&ico[38..], &[1, 2, 3, 4, 5]);
    }
}
