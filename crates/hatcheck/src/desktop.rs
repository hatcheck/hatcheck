//! hatcheck desktop ...: Claude Desktop instances on macOS, one per account, each with its own
//! data folder and login, behind a launcher app of its own colour, as docs/desktop-instances.md
//! describes them.
//!
//! The launcher's icon is Claude's with its hues turned and its saturation scaled, done here
//! with Pillow's own arithmetic for RGB and HSV, so it comes out as the Python version, which
//! used Pillow, made it.

use crate::args::{Count, Parser};
use crate::ospath::{self, Part};
use crate::out;
use crate::paths;
use crate::py;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

const LAUNCHER: &str = r#"#!/bin/sh
# The launcher of the Claude Desktop instance %(name)s, made by hatcheck desktop new. It starts
# Claude Desktop with this instance's data folder, or brings the one already running forward:
# a second process on a folder that has one would come up signed out, so it never starts one.
DATA_DIR=%(data_dir)s
CLAUDE_APP=%(claude_app)s

# Process IDs of Claude Desktop's processes on this data folder: its main process, or with
# "window", its windows (Electron's renderer helpers, which get the folder on their command
# line too); with "any", every instance's main process. An empty DATA_DIR means Claude's
# built-in folder: the processes with no folder.
processes() {
    ps -axww -o pid=,args= | awk -v app="$CLAUDE_APP/Contents/MacOS/" -v dir="$DATA_DIR" -v want="$1" '
        function has(line, option) {
            return index(line, option " ") > 0 || substr(line, length(line) - length(option) + 1) == option
        }
        index($0, app) == 0 { next }
        want == "any" { if (index($0, "--type=") == 0) print $1; next }
        dir != "" && !has($0, "--user-data-dir=" dir) { next }
        dir == "" && index($0, "--user-data-dir=") > 0 { next }
        want == "main" && index($0, "--type=") > 0 { next }
        want == "window" && index($0, "--type=renderer") == 0 { next }
        { print $1 }'
}

# One launch at a time: two at once would both find no process and both start one. mkdir makes
# the lock or fails, at once; one left behind by a launcher that was stopped is waited out.
LOCK="${TMPDIR:-/tmp}/hatcheck-claude-%(name)s.lock"
tries=0
until mkdir "$LOCK" 2>/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -lt 50 ] || break
    sleep 0.2
done
trap 'rmdir "$LOCK" 2>/dev/null' EXIT

# Start Claude Desktop on this folder, keeping the lock until its process is listed, so a
# launch meanwhile brings it forward instead of starting another.
start() {
    if [ -n "$DATA_DIR" ]; then
        mkdir -p "$DATA_DIR" || exit 1
        open -n -a "$CLAUDE_APP" --args --user-data-dir="$DATA_DIR" || exit 1
    else
        open -n -a "$CLAUDE_APP" || exit 1
    fi
    tries=0
    while [ -z "$(processes main)" ] && [ "$tries" -lt 50 ]; do
        tries=$((tries + 1))
        sleep 0.2
    done
    exit 0
}

main=$(processes main | head -n 1)
[ -n "$main" ] || start
# To macOS every instance is the same app, so it's picked out by its process ID. The first time,
# macOS asks to let this launcher control System Events.
osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $main) to true" >/dev/null 2>&1
# Closed with Cmd-W, the window is gone but the process lives on, and coming forward usually
# brings the window back. When it hasn't after a moment, it's asked for one: open -a without -n
# does that and never starts a process, but with other instances running it could ask any of
# them, so then a note says to click this one's own Dock icon. The windowless process is never
# quit: it may be in the middle of a reply.
if [ -z "$(processes window)" ]; then
    sleep 1.5
    if [ -z "$(processes window)" ]; then
        if processes any | grep -qvx "$main"; then
            osascript -e 'display notification "It is running without a window: click its icon in the Dock to open one." with title "Claude Desktop instance %(name)s"' >/dev/null 2>&1
        else
            open -a "$CLAUDE_APP"
        fi
    fi
fi
exit 0
"#;

fn instances_dir() -> String {
    ospath::join(&paths::home(), ".claude-instances")
}

/// Where Claude Desktop is and where launchers go; the tests point both at a folder of their own.
fn claude_app() -> String {
    std::env::var("HATCHECK_CLAUDE_APP")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/Applications/Claude.app".into())
}

fn apps_dir() -> String {
    std::env::var("HATCHECK_APPS_DIR")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "/Applications".into())
}

fn read_plist(path: &str) -> Option<plist::Dictionary> {
    plist::Value::from_file(path).ok()?.into_dictionary()
}

fn plist_str<'a>(d: &'a plist::Dictionary, key: &str) -> Option<&'a str> {
    d.get(key).and_then(plist::Value::as_string)
}

/// The instance name a launcher made by make-claude-instance.sh keeps, in its config.sh.
fn legacy_name(app: &str) -> Option<String> {
    let text =
        fs::read_to_string(ospath::join_all(app, &["Contents", "MacOS", "config.sh"])).ok()?;
    let line = text.lines().find(|l| l.starts_with("INSTANCE_NAME="))?;
    let name = py::strip(line.split_once('=')?.1).trim_matches(['"', '\'']);
    (!name.is_empty()).then(|| name.to_string())
}

/// An instance's launcher: its name, display name, app, data folder and whether the old
/// script made it.
#[derive(Clone, Debug)]
pub struct Instance {
    pub name: String,
    pub display: String,
    pub app: String,
    pub data_dir: Option<String>,
    pub legacy: bool,
    pub running: bool,
}

/// The launcher apps: ours, and those the old script made.
pub fn instances() -> Vec<Instance> {
    let folders = [apps_dir(), ospath::join(&paths::home(), "Applications")];
    let mut apps: Vec<String> = folders
        .iter()
        .flat_map(|f| ospath::glob(f, &[Part::Wild("Claude ", ".app")]))
        .collect();
    apps.sort();
    let mut found = Vec::new();
    for app in apps {
        let plist =
            read_plist(&ospath::join_all(&app, &["Contents", "Info.plist"])).unwrap_or_default();
        let (name, legacy) = match plist_str(&plist, "HatcheckInstance").filter(|n| !n.is_empty()) {
            Some(n) => (Some(n.to_string()), false),
            None => (legacy_name(&app), true),
        };
        let Some(name) = name else { continue }; // another app that starts with Claude
        let data_dir = if legacy {
            None
        } else {
            plist_str(&plist, "HatcheckDataDir")
                .filter(|d| !d.is_empty())
                .map(String::from)
        };
        let data_dir =
            data_dir.or_else(|| (name != "default").then(|| ospath::join(&instances_dir(), &name)));
        let base = ospath::basename(&app);
        let display = base
            .strip_prefix("Claude ")
            .and_then(|b| b.strip_suffix(".app"))
            .unwrap_or(&base)
            .to_string();
        found.push(Instance {
            name,
            display,
            app,
            data_dir,
            legacy,
            running: false,
        });
    }
    found
}

/// Mark each instance running or not, from its main process: Claude Desktop's own process with
/// that --user-data-dir (none for the default instance), not one of its helpers.
fn mark_running(found: &mut [Instance]) {
    let out = Command::new("ps")
        .args(["-axww", "-o", "command="])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let mains: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("/Claude.app/Contents/MacOS/") && !l.contains("--type="))
        .collect();
    for inst in found.iter_mut() {
        inst.running = match &inst.data_dir {
            Some(dir) => {
                let option = format!("--user-data-dir={dir}");
                mains
                    .iter()
                    .any(|l| l.contains(&format!("{option} ")) || l.ends_with(&option))
            }
            None => mains.iter().any(|l| !l.contains("--user-data-dir=")),
        };
    }
}

fn find_instance(name: &str, found: &[Instance]) -> Result<Instance, String> {
    found
        .iter()
        .find(|i| i.name == name || i.display == name)
        .cloned()
        .ok_or_else(|| {
            format!("No Claude Desktop instance named {name}; hatcheck desktop lists them")
        })
}

fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Claude Desktop's own icon file, from its Info.plist.
fn stock_icon(claude: &str) -> Option<String> {
    let plist =
        read_plist(&ospath::join_all(claude, &["Contents", "Info.plist"])).unwrap_or_default();
    let mut name = plist_str(&plist, "CFBundleIconFile")
        .filter(|n| !n.is_empty())
        .unwrap_or("icon.icns")
        .to_string();
    if !name.ends_with(".icns") {
        name += ".icns";
    }
    let path = ospath::join_all(claude, &["Contents", "Resources", &name]);
    ospath::isfile(&path).then_some(path)
}

/// Pillow's RGB to HSV for one pixel (rgb2hsv_row in libImaging/Convert.c), float for float.
pub fn rgb2hsv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    if minc == maxc {
        return (0, 0, maxc);
    }
    let cr = f32::from(maxc - minc);
    let s = cr / f32::from(maxc);
    let rc = f32::from(maxc - r) / cr;
    let gc = f32::from(maxc - g) / cr;
    let bc = f32::from(maxc - b) / cr;
    let h: f32 = if r == maxc {
        bc - gc
    } else if g == maxc {
        (2.0f64 + f64::from(rc) - f64::from(bc)) as f32
    } else {
        (4.0f64 + f64::from(gc) - f64::from(rc)) as f32
    };
    let h = ((f64::from(h) / 6.0 + 1.0) % 1.0) as f32;
    let clip = |x: f64| (x as i32).clamp(0, 255) as u8;
    (clip(f64::from(h) * 255.0), clip(f64::from(s) * 255.0), maxc)
}

/// Pillow's HSV to RGB for one pixel (hsv2rgb in libImaging/Convert.c).
pub fn hsv2rgb(h: u8, s: u8, v: u8) -> (u8, u8, u8) {
    if s == 0 {
        return (v, v, v);
    }
    let hf = f64::from(h as f32) * 6.0 / 255.0;
    let i = hf.floor() as i32;
    let f = (hf - f64::from(i as f32)) as f32;
    let fs = (f64::from(s as f32) / 255.0) as f32;
    let vd = f64::from(v as f32);
    let clip = |x: f64| (x.round() as i32).clamp(0, 255) as u8;
    let p = clip(vd * (1.0 - f64::from(fs)));
    let q = clip(vd * (1.0 - f64::from(fs * f)));
    let t = clip(vd * (1.0 - f64::from(fs) * (1.0 - f64::from(f))));
    match i % 6 {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// A PNG as 8-bit RGBA, as Pillow's Image.open(...).convert("RGBA") gives it.
fn read_png(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let file = fs::File::open(path).ok()?;
    let mut decoder = png::Decoder::new(std::io::BufReader::new(file));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let data = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|l| [*l, *l, *l, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    Some((info.width, info.height, rgba))
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> bool {
    let Ok(file) = fs::File::create(path) else {
        return false;
    };
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .is_ok()
}

/// A folder of its own under the system's temporary folder, deleted when dropped.
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new(prefix: &str) -> std::io::Result<TempDir> {
        let base = std::env::temp_dir();
        for n in 0..1000u32 {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            let path = base.join(format!("{prefix}{}-{stamp:x}{n}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(TempDir(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(std::io::Error::other("no temporary folder"))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn quiet(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Write the icon at src to dst with every hue turned by hue degrees and the saturation scaled
/// by saturation. False, with nothing written, when that can't be done here: no iconutil, or
/// an icon iconutil can't take apart.
pub fn recolour(src: &str, dst: &str, hue: f64, saturation: f64) -> bool {
    if ospath::which("iconutil").is_none() {
        return false;
    }
    let shift = (py::round(hue / 360.0 * 256.0) as i64).rem_euclid(256) as u16;
    let Ok(tmp) = TempDir::new("hatcheck-icon-") else {
        return false;
    };
    let iconset = tmp.0.join("icon.iconset");
    let iconset_s = iconset.to_string_lossy().into_owned();
    if !quiet("iconutil", &["-c", "iconset", src, "-o", &iconset_s]) {
        return false;
    }
    let pngs = ospath::glob(&iconset_s, &[Part::Wild("", ".png")]);
    for png in pngs {
        let path = Path::new(&png);
        let Some((w, h, mut rgba)) = read_png(path) else {
            continue;
        };
        for px in rgba.as_chunks_mut::<4>().0 {
            let (hh, ss, vv) = rgb2hsv(px[0], px[1], px[2]);
            let hh = ((u16::from(hh) + shift) % 256) as u8;
            let ss = (py::round(f64::from(ss) * saturation).clamp(0.0, 255.0)) as u8;
            let (r, g, b) = hsv2rgb(hh, ss, vv);
            px[0] = r;
            px[1] = g;
            px[2] = b;
        }
        write_png(path, w, h, &rgba);
    }
    quiet("iconutil", &["-c", "icns", &iconset_s, "-o", dst]) && ospath::isfile(dst)
}

/// What make_instance made: the launcher, its data folder and whether the icon is recoloured.
pub struct Made {
    pub app: String,
    pub data_dir: Option<String>,
    pub recoloured: bool,
}

fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entry in fs::read_dir(src)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dst.join(entry.file_name()))?;
        }
        Ok(())
    } else {
        fs::copy(src, dst).map(|_| ())
    }
}

/// Make the launcher app for an instance. Errors say what's wrong: a bad name or display,
/// no Claude Desktop, or a launcher there already without replace.
pub fn make_instance(
    name: &str,
    display: &str,
    hue: f64,
    saturation: f64,
    replace: bool,
) -> Result<Made, String> {
    let display = py::collapse(display);
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err(format!(
            "The name can have letters, digits, dots, underscores and dashes: not {}",
            py::repr(&json!(name))
        ));
    }
    if name == "." || name == ".." {
        return Err(
            "The name can't be '.' or '..': the instance's data folder is ~/.claude-instances/NAME"
                .into(),
        );
    }
    if display.is_empty() || display.contains('/') || display.starts_with('.') {
        return Err(format!(
            "The display name can't be empty, start with a dot or hold a slash: not {}",
            py::repr(&json!(display))
        ));
    }
    let claude = claude_app();
    if !ospath::isdir(&claude) {
        return Err(format!(
            "Claude Desktop isn't at {claude}; install it from https://claude.ai/download"
        ));
    }
    let apps = apps_dir();
    let app = ospath::join(&apps, &format!("Claude {display}.app"));
    if ospath::lexists(&app) && !replace {
        return Err(format!("{app} exists already; --replace rebuilds it"));
    }
    let data_dir = (name != "default").then(|| ospath::join(&instances_dir(), name));
    let stock = stock_icon(&claude);
    let tmp = TempDir::new("hatcheck-instance-").map_err(|e| e.to_string())?;
    let built = tmp.0.join(format!("Claude {display}.app"));
    let (macos, resources) = (
        built.join("Contents").join("MacOS"),
        built.join("Contents").join("Resources"),
    );
    fs::create_dir_all(&macos)
        .and_then(|()| fs::create_dir_all(&resources))
        .map_err(|e| e.to_string())?;
    let launcher = LAUNCHER
        .replace("%(name)s", name)
        .replace(
            "%(data_dir)s",
            &data_dir.as_deref().map_or("\"\"".to_string(), sh_quote),
        )
        .replace("%(claude_app)s", &sh_quote(&claude));
    let launch = macos.join("launch");
    fs::write(&launch, launcher).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&launch, fs::Permissions::from_mode(0o755));
    }
    let icon = resources.join("icon.icns");
    let icon_s = icon.to_string_lossy().into_owned();
    let turned = hue.rem_euclid(360.0) != 0.0 || saturation != 1.0;
    let recoloured = stock
        .as_deref()
        .is_some_and(|s| turned && recolour(s, &icon_s, hue, saturation));
    if let Some(stock) = &stock {
        if !icon.is_file() {
            fs::copy(stock, &icon).map_err(|e| e.to_string())?;
        }
    }
    let version = crate::cli::version();
    let mut plist = plist::Dictionary::new();
    // As plistlib writes them: the keys in order.
    let entries: Vec<(&str, plist::Value)> = vec![
        ("CFBundleDisplayName", format!("Claude {display}").into()),
        ("CFBundleExecutable", "launch".into()),
        ("CFBundleIconFile", "icon".into()),
        (
            "CFBundleIdentifier",
            format!("dev.hatcheck.hatcheck.claude-{}", name.replace('_', "-")).into(),
        ),
        ("CFBundleInfoDictionaryVersion", "6.0".into()),
        ("CFBundleName", format!("Claude {display}").into()),
        ("CFBundlePackageType", "APPL".into()),
        ("CFBundleShortVersionString", version.into()),
        ("CFBundleVersion", version.into()),
        (
            "HatcheckDataDir",
            data_dir.clone().unwrap_or_default().into(),
        ),
        ("HatcheckInstance", name.into()),
        ("LSMinimumSystemVersion", "11.0".into()),
        ("NSHighResolutionCapable", true.into()),
    ];
    for (k, v) in entries {
        plist.insert(k.to_string(), v);
    }
    plist::Value::Dictionary(plist)
        .to_file_xml(built.join("Contents").join("Info.plist"))
        .map_err(|e| e.to_string())?;
    if ospath::lexists(&app) {
        fs::remove_dir_all(&app).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&apps).map_err(|e| e.to_string())?;
    if fs::rename(&built, &app).is_err() {
        // Another volume: copy it over, as shutil.move does.
        copy_tree(&built, Path::new(&app)).map_err(|e| e.to_string())?;
    }
    Ok(Made {
        app,
        data_dir,
        recoloured,
    })
}

/// Whether dir is a folder of its own in instances, the only kind of data folder remove --data
/// deletes. A launcher can name any folder: one hatcheck 0.5.0 made for the name .. had the
/// home folder, and the old script's take their name from a config.sh.
fn own_folder(dir: &str, instances: &str) -> bool {
    let (parent, name) = ospath::split(dir);
    !matches!(name.as_str(), "" | "." | "..") && paths::same_folder(&parent, instances)
}

/// Delete an instance's launcher app, and with data its data folder: whether that went too.
/// Nothing is deleted when data is asked for and the data folder isn't one of its own.
pub fn remove_instance(inst: &Instance, data: bool) -> Result<bool, String> {
    let dir = inst.data_dir.as_deref().filter(|_| data);
    let instances = instances_dir();
    if let Some(dir) = dir.filter(|d| !own_folder(d, &instances)) {
        return Err(format!(
            "Nothing was removed: its data folder, {dir}, isn't a folder of its own in {instances}, so --data won't delete it. Without --data, only the launcher is removed."
        ));
    }
    fs::remove_dir_all(&inst.app).map_err(|e| e.to_string())?;
    if let Some(dir) = dir.filter(|d| ospath::isdir(d)) {
        fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
        return Ok(true);
    }
    Ok(false)
}

fn as_json(inst: &Instance) -> Value {
    json!({"name": inst.name, "display": inst.display, "app": inst.app, "data_dir": inst.data_dir, "legacy": inst.legacy, "running": inst.running})
}

/// Python's float(text) for --hue and --saturation.
fn float_arg(parser: &Parser, name: &str, text: Option<&str>, default: f64) -> Result<f64, i32> {
    match text {
        None => Ok(default),
        Some(t) => py::float(t).ok_or_else(|| {
            parser
                .error(&format!("argument {name}: invalid float value: '{t}'"))
                .exit_code()
        }),
    }
}

pub fn main(argv: &[String], prog: &str) -> i32 {
    let parser = Parser::new(prog, "Claude Desktop instances on macOS: one per account, each with its own data folder and a launcher of its own colour.")
        .flag(&["--json"], "json", "list the instances as JSON")
        .subcommands("action", &[
            ("new", "make an instance's launcher, /Applications/Claude DISPLAY.app"),
            ("open", "start an instance, or bring it forward"),
            ("remove", "delete an instance's launcher"),
        ]);
    let args = match parser.parse(argv) {
        Ok(a) => a,
        Err(stop) => return stop.exit_code(),
    };
    let action = args.value("action").map(String::from);
    let sub = |name: &str| format!("{prog} {name}");
    // Read the subcommand's words before the platform check, as argparse does.
    let (new_parser, open_parser, remove_parser) = (
        Parser::new(&sub("new"), "")
            .positional("name", Count::One, "the instance: letters, digits, dots, underscores and dashes; default for Claude's built-in data folder")
            .positional("display", Count::One, "what the launcher is called, as in Claude Work.app")
            .option(&["--hue"], "hue", "DEG", "turn the icon's hues by this many degrees: about 110 green, 190 blue, 240 purple, -10 red")
            .option(&["--saturation"], "saturation", "X", "scale the icon's saturation (1.0 keeps it)")
            .flag(&["--replace"], "replace", "rebuild a launcher that exists"),
        Parser::new(&sub("open"), "").positional("name", Count::One, "the instance's name, or its display name"),
        Parser::new(&sub("remove"), "")
            .positional("name", Count::One, "the instance's name, or its display name")
            .flag(&["--data"], "data", "delete its data folder too: the login and everything in it"),
    );
    let sub_args = match action.as_deref() {
        Some("new") => new_parser.parse_known(&args.rest),
        Some("open") => open_parser.parse_known(&args.rest),
        Some("remove") => remove_parser.parse_known(&args.rest),
        _ => Ok(Default::default()),
    };
    let sub_args = match sub_args {
        // What the subcommand didn't know is reported by this parser, as argparse does.
        Ok((_, extras)) if !extras.is_empty() => {
            return parser
                .error(&format!("unrecognized arguments: {}", extras.join(" ")))
                .exit_code()
        }
        Ok((a, _)) => a,
        Err(stop) => return stop.exit_code(),
    };
    let (hue, saturation) = if action.as_deref() == Some("new") {
        let hue = match float_arg(&new_parser, "--hue", sub_args.value("hue"), 0.0) {
            Ok(h) => h,
            Err(code) => return code,
        };
        match float_arg(
            &new_parser,
            "--saturation",
            sub_args.value("saturation"),
            1.0,
        ) {
            Ok(s) => (hue, s),
            Err(code) => return code,
        }
    } else {
        (0.0, 1.0)
    };
    if !cfg!(target_os = "macos") {
        return out::fail("Claude Desktop instances are macOS only for now");
    }
    match action.as_deref() {
        Some("new") => {
            let name = sub_args.value("name").unwrap_or("");
            let made = match make_instance(
                name,
                sub_args.value("display").unwrap_or(""),
                hue,
                saturation,
                sub_args.flag("replace"),
            ) {
                Ok(m) => m,
                Err(message) => return out::fail(&message),
            };
            let paint = out::Paint::new();
            let how = if made.recoloured {
                ", with a recoloured icon"
            } else if hue.rem_euclid(360.0) == 0.0 && saturation == 1.0 {
                ""
            } else {
                ", with the stock icon (it couldn't be recoloured)"
            };
            out::success(&format!("Made {}{how}.", made.app));
            match &made.data_dir {
                Some(dir) => out::println(&format!(
                    "Its data folder is {dir}: sign in there once, on first launch."
                )),
                None => out::warn(
                    "It runs Claude's built-in data folder; don't also use the stock Claude icon.",
                ),
            }
            out::println(&format!(
                "Open it from Launchpad, or with: {}",
                paint.command(&format!("hatcheck desktop open {name}"))
            ));
            0
        }
        Some("open") => match find_instance(sub_args.value("name").unwrap_or(""), &instances()) {
            // The launcher brings a running instance forward instead of starting another.
            Ok(inst) => Command::new("open")
                .args(["-a", &inst.app])
                .status()
                .map_or(1, |s| s.code().unwrap_or(1)),
            Err(message) => out::fail(&message),
        },
        Some("remove") => {
            let inst = match find_instance(sub_args.value("name").unwrap_or(""), &instances()) {
                Ok(i) => i,
                Err(message) => return out::fail(&message),
            };
            let data_gone = match remove_instance(&inst, sub_args.flag("data")) {
                Ok(gone) => gone,
                Err(message) => return out::fail(&message),
            };
            out::success(&format!(
                "Removed {}{}.",
                inst.app,
                if data_gone {
                    ", and its data folder"
                } else {
                    ""
                }
            ));
            if let Some(dir) = inst
                .data_dir
                .as_deref()
                .filter(|d| !data_gone && own_folder(d, &instances_dir()) && ospath::isdir(d))
            {
                out::println(&format!(
                    "Its data folder, {dir}, is kept; --data deletes it too."
                ));
            }
            0
        }
        _ => {
            let mut found = instances();
            mark_running(&mut found);
            if args.flag("json") {
                out::println(&py::dumps(
                    &json!({"version": 1, "instances": found.iter().map(as_json).collect::<Vec<_>>()}),
                ));
                return 0;
            }
            if found.is_empty() {
                out::println(&format!(
                    "No Claude Desktop instances yet. Make one with: {}",
                    out::Paint::new().command("hatcheck desktop new a A --hue 190")
                ));
                return 0;
            }
            let paint = out::Paint::new();
            for inst in &found {
                let running = if inst.running {
                    paint.p("32", "  ● running")
                } else {
                    String::new()
                };
                let legacy = if inst.legacy {
                    format!(
                        "  {}",
                        paint.warning("(made by the old script; --replace rebuilds it)")
                    )
                } else {
                    String::new()
                };
                out::println(&format!(
                    "  {:<20} {:<12} {}{running}{legacy}",
                    inst.display, inst.name, inst.app
                ));
            }
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_folder_of_its_own_is_deleted() {
        let root = if ospath::WINDOWS { "C:\\h" } else { "/h" };
        let instances = ospath::join(root, ".claude-instances");
        let at = |rest: &[&str]| ospath::join_all(&instances, rest);
        assert!(own_folder(&at(&["a"]), &instances));
        assert!(own_folder(&at(&[".x"]), &instances));
        for not_own in [
            at(&[".."]),
            at(&["."]),
            at(&["a", "b"]),
            at(&["..", "x"]),
            at(&["a", "..", ".."]),
            instances.clone(),
            root.to_string(),
        ] {
            assert!(!own_folder(&not_own, &instances), "{not_own}");
        }
    }

    #[test]
    fn hsv_round_trips_greys_and_primaries() {
        assert_eq!(rgb2hsv(0, 0, 0), (0, 0, 0));
        assert_eq!(rgb2hsv(255, 0, 0), (0, 255, 255));
        assert_eq!(hsv2rgb(0, 255, 255), (255, 0, 0));
        assert_eq!(hsv2rgb(0, 0, 77), (77, 77, 77));
    }
}
