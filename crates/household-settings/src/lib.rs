//! The household's shared settings, from household-ui's
//! `reference/household_settings.rs` (commit 514be8f, the 15 Oct follow-up:
//! seeding and Open makes the shared file). Every app uses this crate rather
//! than a copy of its own.
//!
//! One folder holds `shared.toml`, read by every household app, and one file
//! per app (`terminal.toml`, `cerea.toml`, …). A setting is looked up in the
//! app's file, then in the shared file, then in the app's default: a key in
//! the app's file is that setting "for this app only", and taking it out
//! returns the app to the shared value.
//!
//! The folder is `$XDG_CONFIG_HOME/household/` unless another place has been
//! chosen. The choice is kept in that folder's `settings-location`, which
//! every app reads: the shared file's place, and each app's file's place
//! where it is not the default.
//!
//! - Writes take the folder's lock, go to a temporary file and are renamed
//!   into place, and keep the file's comments and layout (`toml_edit`).
//! - [`Store::watch`] keeps one inotify watch per folder (the `notify`
//!   crate) on its own thread and calls back when a file changes; the app
//!   turns that into a message and reloads off the UI thread.
//! - A file that cannot be read keeps the values last read from it, and
//!   [`Store::problems`] says what is wrong, in words for the page.
//! - [`Store::move_file`] moves a file to a new place and keeps the old one
//!   beside where it was, as a backup.
//!
//! Every function here that reads or writes a file blocks: call it from a
//! background task (household rule 40), never from `update` or `view`.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use toml_edit::{DocumentMut, Item};

/// The folder's name under the configuration folder
pub const DIR_NAME: &str = "household";
/// The file every app reads
pub const SHARED_FILE: &str = "shared.toml";
/// Where the files are, if not in the default folder; in the default folder
pub const POINTER_FILE: &str = "settings-location";
/// The lock writers take, in each folder written to
pub const LOCK_FILE: &str = ".lock";
/// The app file's own key: whether the app reads the shared file at all
pub const USE_SHARED: &str = "use-shared-settings";
/// The pointer's key for the shared file's folder
const POINTER_SHARED: &str = "shared";
/// How long a write waits for another app's write to finish (rule 41)
pub const LOCK_DEADLINE: Duration = Duration::from_secs(2);

/// A setting's value
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl Value {
    fn kind(&self) -> Kind {
        match self {
            Value::Bool(_) => Kind::Bool,
            Value::Int(_) => Kind::Int,
            Value::Float(_) => Kind::Float,
            Value::Text(_) => Kind::Text,
        }
    }

    fn to_item(&self) -> Item {
        match self {
            Value::Bool(b) => toml_edit::value(*b),
            Value::Int(n) => toml_edit::value(*n),
            Value::Float(x) => toml_edit::value(*x),
            Value::Text(s) => toml_edit::value(s.as_str()),
        }
    }

    /// `item` as a value of `kind`: an integer is taken where a number with
    /// a fraction is wanted, and a whole number written with a fraction
    /// where an integer is
    fn from_item(item: &Item, kind: Kind) -> Option<Value> {
        let v = item.as_value()?;
        match kind {
            Kind::Bool => v.as_bool().map(Value::Bool),
            Kind::Int => v.as_integer().or_else(|| v.as_float().filter(|x| x.fract() == 0.0).map(|x| x as i64)).map(Value::Int),
            Kind::Float => v.as_float().or_else(|| v.as_integer().map(|n| n as f64)).map(Value::Float),
            Kind::Text => v.as_str().map(|s| Value::Text(s.to_string())),
        }
    }
}

/// What kind of value a key holds
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    Bool,
    Int,
    Float,
    Text,
}

impl Kind {
    /// The kind in words, for a problem's sentence
    fn words(self) -> &'static str {
        match self {
            Kind::Bool => "true or false",
            Kind::Int => "a whole number",
            Kind::Float => "a number",
            Kind::Text => "text in quotes",
        }
    }
}

/// A default, as a constant
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fixed {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(&'static str),
}

impl Fixed {
    pub fn value(self) -> Value {
        match self {
            Fixed::Bool(b) => Value::Bool(b),
            Fixed::Int(n) => Value::Int(n),
            Fixed::Float(x) => Value::Float(x),
            Fixed::Text(s) => Value::Text(s.to_string()),
        }
    }
}

/// A shared key: its name in the files, its household default, and what it
/// holds
#[derive(Clone, Copy, Debug)]
pub struct Key {
    pub name: &'static str,
    pub default: Fixed,
    pub about: &'static str,
}

impl Key {
    pub fn kind(&self) -> Kind {
        self.default.value().kind()
    }
}

/// The shared keys (decided by Juliet and Tom, 15 Oct): text sizes and line
/// spacings (household rule 23a), the code font, light or dark, density,
/// spelling language, date and time format, tooltip delay and reduced
/// motion. Accent colour and the system fonts stay with COSMIC. An app shows
/// rows only for the keys it has a setting for.
pub const KEYS: &[Key] = &[
    Key { name: "theme", default: Fixed::Text("system"), about: "Light or dark: system, light or dark" },
    Key { name: "interface-size", default: Fixed::Int(14), about: "Interface text: labels, buttons, menus, settings, in pixels" },
    Key { name: "interface-line-spacing", default: Fixed::Float(1.2), about: "Interface text's line spacing" },
    Key { name: "prose-size", default: Fixed::Int(16), about: "Prose an app sets itself (the Text view, Cerea, messages), in pixels" },
    Key { name: "prose-line-spacing", default: Fixed::Float(1.5), about: "Prose's line spacing" },
    Key { name: "paper-size", default: Fixed::Int(20), about: "The Paper view's prose, in pixels" },
    Key { name: "paper-line-spacing", default: Fixed::Float(1.5), about: "The Paper view's line spacing" },
    Key { name: "caption-size", default: Fixed::Int(12), about: "Captions and subtext, in pixels" },
    Key { name: "command-size", default: Fixed::Int(14), about: "Commands and code the agent runs, monospace, in pixels" },
    Key { name: "command-line-spacing", default: Fixed::Float(1.2), about: "Commands' line spacing" },
    Key { name: "code-font", default: Fixed::Text("Noto Sans Mono"), about: "The monospace font for code and terminals" },
    Key { name: "density", default: Fixed::Text("standard"), about: "Buttons and spacing: compact, standard or spacious" },
    Key { name: "spelling-language", default: Fixed::Text("en-GB"), about: "The spelling to expect, as a language tag" },
    Key { name: "date-format", default: Fixed::Text("system"), about: "Dates: system, iso or day-month-year" },
    Key { name: "time-format", default: Fixed::Text("system"), about: "Times: system, 24-hour or 12-hour" },
    Key { name: "tooltip-delay", default: Fixed::Int(500), about: "How long the pointer rests before a tooltip shows, in milliseconds" },
    Key { name: "reduce-motion", default: Fixed::Bool(false), about: "Fewer animations" },
];

/// A shared key by name
pub fn key(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.name == name)
}

/// A household app: its id (its file is `<id>.toml`) and its name
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HouseholdApp {
    pub id: &'static str,
    pub name: &'static str,
}

/// The household apps that can read the shared file, in Juliet's order
pub const APPS: &[HouseholdApp] = &[
    HouseholdApp { id: "terminal", name: "Terminal" },
    HouseholdApp { id: "cerea", name: "Cerea" },
    HouseholdApp { id: "lollypop", name: "Lollypop" },
    HouseholdApp { id: "discord", name: "Discord" },
    HouseholdApp { id: "screenpush", name: "ScreenPush" },
    HouseholdApp { id: "ai-usage", name: "AI usage" },
];

/// Which of the two files
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub enum Which {
    /// This app's own file
    App,
    /// The file every app reads
    Shared,
}

/// Where a setting's value came from
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    /// This app's file: set for this app only
    App,
    Shared,
    Default,
}

/// A shared row's chain
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Chain {
    /// The shared value: a change goes to the shared file
    Linked,
    /// Set for this app only
    Own,
}

/// What to do with the file when its place changes
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Move {
    /// Take the current file to the new place (a file already there is kept
    /// beside it as a backup)
    Carry,
    /// Start with an empty file in the new place
    StartEmpty,
    /// Use the file already in the new place
    UseExisting,
}

/// Something wrong with a file, kept until the file is read again
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub which: Option<Which>,
    pub path: PathBuf,
    pub kind: ProblemKind,
    /// The raw detail, for Show details and the log (rule 20)
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProblemKind {
    /// Not TOML from this line
    Syntax { line: Option<usize> },
    /// A shared key holding the wrong kind of value
    WrongKind { key: &'static str, line: Option<usize>, wanted: Kind },
    /// The file is there but cannot be read
    Unreadable,
    /// The chosen folder is not there (an unplugged disk, a renamed folder)
    FolderMissing,
}

impl Problem {
    /// The problem in words (rule 20: what happened and what to do)
    pub fn sentence(&self, app_name: &str) -> String {
        self.sentence_in(app_name, std::env::var_os("HOME").map(PathBuf::from).as_deref())
    }

    /// [`Problem::sentence`] with `home` as the home folder, for `~/…`
    pub fn sentence_in(&self, app_name: &str, home: Option<&Path>) -> String {
        let file = match self.which {
            Some(Which::Shared) => "the shared file".to_string(),
            Some(Which::App) => format!("{app_name}'s file"),
            None => "the settings location".to_string(),
        };
        let keeps = match self.which {
            Some(Which::Shared) => "The apps keep the settings they had until it is fixed.".to_string(),
            _ => format!("{app_name} keeps the settings it had until it is fixed."),
        };
        let what = match &self.kind {
            ProblemKind::Syntax { line: Some(line) } => format!("line {line} is not written as a setting"),
            ProblemKind::Syntax { line: None } => "it is not written as settings".to_string(),
            ProblemKind::WrongKind { key, line, wanted } => match line {
                Some(line) => format!("line {line} gives {key} something other than {}", wanted.words()),
                None => format!("{key} should be {}", wanted.words()),
            },
            ProblemKind::Unreadable => "it could not be opened".to_string(),
            ProblemKind::FolderMissing => {
                return format!("Could not find {}: is its disk connected? {keeps}", display_path_in(self.path.parent().unwrap_or(&self.path), home));
            }
        };
        format!("Could not read {file}: {what}. {keeps}")
    }
}

/// A write that failed
#[derive(Debug)]
pub enum WriteError {
    /// Another app held the lock past [`LOCK_DEADLINE`]
    Busy,
    /// The file there cannot be read, so writing would lose what is in it
    Unreadable(Problem),
    Io(io::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::Busy => write!(f, "another app is saving its settings: try again in a moment"),
            WriteError::Unreadable(p) => write!(f, "the file cannot be read ({}): fix it first", p.detail),
            WriteError::Io(e) if e.kind() == io::ErrorKind::PermissionDenied => write!(f, "the folder cannot be written to"),
            WriteError::Io(e) if e.kind() == io::ErrorKind::NotFound => write!(f, "the folder is not there"),
            WriteError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WriteError {}

impl From<io::Error> for WriteError {
    fn from(e: io::Error) -> Self {
        WriteError::Io(e)
    }
}

/// The default folder, `$XDG_CONFIG_HOME/household`. In a Flatpak the
/// sandbox's `XDG_CONFIG_HOME` is the app's own; the host's is used, as
/// `HOST_XDG_CONFIG_HOME` or else `~/.config`, which the manifest grants
/// (`--filesystem=xdg-config/household:create`)
pub fn default_dir() -> PathBuf {
    default_dir_from(|name| std::env::var_os(name), Path::new("/.flatpak-info").exists())
}

/// [`default_dir`] from the variables `env` gives
pub fn default_dir_from(env: impl Fn(&str) -> Option<OsString>, flatpak: bool) -> PathBuf {
    let absolute = |name: &str| env(name).map(PathBuf::from).filter(|p| p.is_absolute());
    let home = || PathBuf::from(env("HOME").unwrap_or_default()).join(".config");
    let config = if flatpak {
        absolute("HOST_XDG_CONFIG_HOME").unwrap_or_else(home)
    } else {
        absolute("XDG_CONFIG_HOME").unwrap_or_else(home)
    };
    config.join(DIR_NAME)
}

/// A path to show: under the home folder as `~/…`
pub fn display_path(path: &Path) -> String {
    display_path_in(path, std::env::var_os("HOME").map(PathBuf::from).as_deref())
}

/// [`display_path`] with `home` as the home folder
pub fn display_path_in(path: &Path, home: Option<&Path>) -> String {
    if let Some(rest) = home.filter(|h| !h.as_os_str().is_empty()).and_then(|h| path.strip_prefix(h).ok()) {
        if rest.as_os_str().is_empty() {
            return "~".into();
        }
        return format!("~/{}", rest.display());
    }
    path.display().to_string()
}

/// `~/…` made absolute against `home`
fn expand(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ if path == "~" => home.map(Path::to_path_buf).unwrap_or_default(),
        _ => PathBuf::from(path),
    }
}

/// One file as last read
#[derive(Clone, Debug, Default)]
struct FileState {
    /// The shared keys it sets, as last read without a problem
    values: BTreeMap<&'static str, Value>,
    /// Whether this app reads the shared file (the app file only)
    use_shared: Option<bool>,
    exists: bool,
    problem: Option<Problem>,
}

/// The settings, as read from the folder: what an app holds and asks
#[derive(Clone, Debug)]
pub struct Store {
    /// This app's id: its file is `<id>.toml`
    app: &'static str,
    /// The default folder, where the pointer is
    default_dir: PathBuf,
    /// The home folder, for `~/` in the pointer
    home: Option<PathBuf>,
    shared_dir: PathBuf,
    app_dir: PathBuf,
    shared: FileState,
    own: FileState,
    pointer_problem: Option<Problem>,
}

impl Store {
    /// The settings for app `app` (see [`APPS`]), from the folder
    /// [`default_dir`] or wherever its pointer says
    pub fn open(app: &'static str) -> Store {
        Store::open_in(default_dir(), std::env::var_os("HOME").map(PathBuf::from), app)
    }

    /// [`Store::open`] with the default folder `default_dir` (tests give a
    /// temporary one) and the home folder `home`
    pub fn open_in(default_dir: PathBuf, home: Option<PathBuf>, app: &'static str) -> Store {
        let mut store = Store {
            app,
            shared_dir: default_dir.clone(),
            app_dir: default_dir.clone(),
            default_dir,
            home,
            shared: FileState::default(),
            own: FileState::default(),
            pointer_problem: None,
        };
        store.reload();
        store
    }

    /// Read the pointer and both files again. A file that cannot be read
    /// keeps the values it had
    pub fn reload(&mut self) {
        let _ = fs::create_dir_all(&self.default_dir);
        let (shared_dir, app_dir, problem) = self.read_pointer();
        self.shared_dir = shared_dir;
        self.app_dir = app_dir;
        self.pointer_problem = problem;
        let shared = self.path(Which::Shared);
        let own = self.path(Which::App);
        read_into(&mut self.shared, &shared, Which::Shared, false);
        read_into(&mut self.own, &own, Which::App, true);
    }

    fn pointer_path(&self) -> PathBuf {
        self.default_dir.join(POINTER_FILE)
    }

    fn read_pointer(&self) -> (PathBuf, PathBuf, Option<Problem>) {
        let mut shared = self.default_dir.clone();
        let mut own = self.default_dir.clone();
        let path = self.pointer_path();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (shared, own, None),
            Err(e) => {
                let problem = Problem { which: None, path, kind: ProblemKind::Unreadable, detail: e.to_string() };
                return (shared, own, Some(problem));
            }
        };
        match text.parse::<DocumentMut>() {
            Ok(doc) => {
                let place = |name: &str| doc.get(name).and_then(Item::as_str).filter(|s| !s.is_empty()).map(|s| expand(s, self.home.as_deref()));
                if let Some(dir) = place(POINTER_SHARED) {
                    shared = dir;
                }
                if let Some(dir) = place(self.app) {
                    own = dir;
                }
                (shared, own, None)
            }
            Err(e) => {
                let line = e.span().map(|s| line_of(&text, s.start));
                (shared, own, Some(Problem { which: None, path, kind: ProblemKind::Syntax { line }, detail: e.to_string() }))
            }
        }
    }

    /// This app's id
    pub fn app(&self) -> &'static str {
        self.app
    }

    /// A file's path
    pub fn path(&self, which: Which) -> PathBuf {
        match which {
            Which::Shared => self.shared_dir.join(SHARED_FILE),
            Which::App => self.app_dir.join(format!("{}.toml", self.app)),
        }
    }

    /// A file's folder
    pub fn dir(&self, which: Which) -> &Path {
        match which {
            Which::Shared => &self.shared_dir,
            Which::App => &self.app_dir,
        }
    }

    /// The default folder
    pub fn default_dir(&self) -> &Path {
        &self.default_dir
    }

    /// Whether a file is in the default folder
    pub fn is_default(&self, which: Which) -> bool {
        self.dir(which) == self.default_dir
    }

    /// Whether a file exists yet
    pub fn exists(&self, which: Which) -> bool {
        match which {
            Which::Shared => self.shared.exists,
            Which::App => self.own.exists,
        }
    }

    /// Whether this app reads the shared file ("Use shared settings")
    pub fn use_shared(&self) -> bool {
        self.own.use_shared.unwrap_or(true)
    }

    /// What is wrong with the files, if anything: the pointer, then the
    /// shared file, then this app's
    pub fn problems(&self) -> Vec<&Problem> {
        [&self.pointer_problem, &self.shared.problem, &self.own.problem].into_iter().flatten().collect()
    }

    /// The problem with one file
    pub fn problem(&self, which: Which) -> Option<&Problem> {
        match which {
            Which::Shared => self.shared.problem.as_ref(),
            Which::App => self.own.problem.as_ref(),
        }
    }

    /// A problem in words, with paths shown under this store's home
    pub fn sentence(&self, problem: &Problem, app_name: &str) -> String {
        problem.sentence_in(app_name, self.home.as_deref())
    }

    /// The problem with the pointer, `settings-location`
    pub fn pointer_problem(&self) -> Option<&Problem> {
        self.pointer_problem.as_ref()
    }

    /// The household apps that use the shared folder: those whose file is
    /// in its default place or where the pointer says
    pub fn users(&self) -> Vec<&'static str> {
        let pointer = fs::read_to_string(self.pointer_path()).ok().and_then(|t| t.parse::<DocumentMut>().ok());
        APPS.iter()
            .filter(|a| {
                if a.id == self.app {
                    return true;
                }
                let dir = pointer
                    .as_ref()
                    .and_then(|d| d.get(a.id).and_then(Item::as_str).map(|s| expand(s, self.home.as_deref())))
                    .unwrap_or_else(|| self.default_dir.clone());
                dir.join(format!("{}.toml", a.id)).exists()
            })
            .map(|a| a.name)
            .collect()
    }

    /// A shared key's value: this app's, else the shared file's (while this
    /// app uses it), else the household default
    pub fn get(&self, name: &str) -> Value {
        self.resolve(name).0
    }

    /// Where [`Store::get`]'s value comes from
    pub fn source(&self, name: &str) -> Source {
        self.resolve(name).1
    }

    fn resolve(&self, name: &str) -> (Value, Source) {
        let Some(key) = key(name) else {
            debug_assert!(false, "{name} is not a shared key");
            return (Value::Bool(false), Source::Default);
        };
        if let Some(v) = self.own.values.get(key.name) {
            return (v.clone(), Source::App);
        }
        if self.use_shared()
            && let Some(v) = self.shared.values.get(key.name)
        {
            return (v.clone(), Source::Shared);
        }
        (key.default.value(), Source::Default)
    }

    /// The value the shared file gives, or the default: what "Use shared"
    /// returns to
    pub fn shared_value(&self, name: &str) -> Value {
        match (self.shared.values.get(name), key(name)) {
            (Some(v), _) => v.clone(),
            (None, Some(k)) => k.default.value(),
            (None, None) => Value::Bool(false),
        }
    }

    pub fn int(&self, name: &str) -> i64 {
        match self.get(name) {
            Value::Int(n) => n,
            Value::Float(x) => x.round() as i64,
            _ => 0,
        }
    }

    pub fn float(&self, name: &str) -> f64 {
        match self.get(name) {
            Value::Float(x) => x,
            Value::Int(n) => n as f64,
            _ => 0.0,
        }
    }

    pub fn bool(&self, name: &str) -> bool {
        matches!(self.get(name), Value::Bool(true))
    }

    pub fn text(&self, name: &str) -> String {
        match self.get(name) {
            Value::Text(s) => s,
            other => format!("{other:?}"),
        }
    }

    /// A row's chain: none while the app does not use the shared file
    pub fn chain(&self, name: &str) -> Option<Chain> {
        self.use_shared().then(|| if self.own.values.contains_key(name) { Chain::Own } else { Chain::Linked })
    }

    /// Whether any shared key is set for this app only, out of `names`
    /// (what "Reset this app to shared" would take away)
    pub fn any_own(&self, names: &[&str]) -> bool {
        names.iter().any(|n| self.own.values.contains_key(n))
    }

    /// Set a shared key where its row says: the shared file while it is
    /// linked, else this app's file
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), WriteError> {
        let which = if self.chain(name) == Some(Chain::Linked) { Which::Shared } else { Which::App };
        self.set_in(which, name, value)
    }

    /// Set a key in one file
    pub fn set_in(&mut self, which: Which, name: &str, value: Value) -> Result<(), WriteError> {
        let item = value.to_item();
        self.edit(which, |doc| {
            set_item(doc, name, item);
        })
    }

    /// "Use shared": take this app's value of a key away
    pub fn remove_own(&mut self, name: &str) -> Result<(), WriteError> {
        self.edit(Which::App, |doc| {
            doc.remove(name);
        })
    }

    /// A linked chain clicked: the value kept in this app's file, so the
    /// row is this app's own from now on with nothing changed on screen
    pub fn make_own(&mut self, name: &str) -> Result<(), WriteError> {
        let value = self.get(name);
        self.set_in(Which::App, name, value)
    }

    /// "Reset this app to shared": every one of `names` taken out of this
    /// app's file
    pub fn reset_to_shared(&mut self, names: &[&str]) -> Result<(), WriteError> {
        self.edit(Which::App, |doc| {
            for name in names {
                doc.remove(name);
            }
        })
    }

    /// "Use shared settings" switched. Off: each of `names` (the keys the
    /// app has rows for) that came from the shared file is kept in this
    /// app's file, so nothing changes on screen. On: the values in this
    /// app's file that are the shared ones are taken out again, so a
    /// round trip leaves the file as it was
    pub fn set_use_shared(&mut self, on: bool, names: &[&str]) -> Result<(), WriteError> {
        if on == self.use_shared() {
            return Ok(());
        }
        let copies: Vec<(String, Value)> = if on {
            Vec::new()
        } else {
            names.iter().filter(|n| self.source(n) == Source::Shared).map(|n| (n.to_string(), self.get(n))).collect()
        };
        let same: Vec<String> = if on {
            names.iter().filter(|n| self.own.values.get(**n).is_some_and(|v| *v == self.shared_value(n))).map(|n| n.to_string()).collect()
        } else {
            Vec::new()
        };
        self.edit(Which::App, |doc| {
            for (name, value) in copies {
                set_item(doc, &name, value.to_item());
            }
            for name in same {
                doc.remove(&name);
            }
            if on {
                doc.remove(USE_SHARED);
            } else {
                set_item(doc, USE_SHARED, toml_edit::value(false));
            }
        })
    }

    /// First run: the app's settings from before (`current`, read from
    /// wherever it kept them) become this app's own only where they differ
    /// from what the shared file gives, so nothing changes on screen. Does
    /// nothing once this app's file exists; returns whether it ran
    pub fn migrate(&mut self, current: &[(&str, Value)]) -> Result<bool, WriteError> {
        if self.own.exists || self.own.problem.is_some() {
            return Ok(false);
        }
        // The first app to come fills the shared file with its own values:
        // they are shared, not this app's own (Juliet, 15 Oct)
        if !self.shared.exists && self.shared.problem.is_none() && self.use_shared() {
            let values: BTreeMap<&'static str, Value> = current.iter().filter_map(|(n, v)| key(n).map(|k| (k.name, v.clone()))).collect();
            self.create_shared(&values)?;
        }
        let differ: Vec<(String, Value)> =
            current.iter().filter(|(n, v)| key(n).is_some() && *v != self.shared_value(n)).map(|(n, v)| (n.to_string(), v.clone())).collect();
        let app = self.app;
        self.edit(Which::App, |doc| {
            if doc.is_empty() && doc.decor().prefix().is_none() {
                doc.decor_mut().set_prefix(format!(
                    "# {app}'s own settings: the household's shared keys set for this app only.\n# Taking a key out returns it to shared.toml's value.\n"
                ));
            }
            for (name, value) in differ {
                set_item(doc, &name, value.to_item());
            }
        })?;
        Ok(true)
    }

    /// "Open" on a shared file that is not there yet: it is made first,
    /// listing every shared key with its current value, commented where
    /// that is the default. Returns its path
    pub fn create_shared_file(&mut self) -> Result<PathBuf, WriteError> {
        if !self.shared.exists {
            let values = self.shared.values.clone();
            self.create_shared(&values)?;
        }
        Ok(self.path(Which::Shared))
    }

    /// The shared file written as [`listing`] of `values`, unless another
    /// app made it first (checked under the lock); the state taken up from
    /// what is there afterwards
    fn create_shared(&mut self, values: &BTreeMap<&'static str, Value>) -> Result<(), WriteError> {
        let path = self.path(Which::Shared);
        let dir = self.dir(Which::Shared).to_path_buf();
        fs::create_dir_all(&dir)?;
        {
            let _lock = lock(&dir)?;
            if !path.exists() {
                write_atomic(&path, &listing(values))?;
            }
        }
        read_into(&mut self.shared, &path, Which::Shared, false);
        Ok(())
    }

    /// Whether a folder holds a file of this kind already (the Change…
    /// dialog asks what to do then)
    pub fn has_file_at(&self, which: Which, dir: &Path) -> bool {
        dir.join(self.file_name(which)).exists()
    }

    fn file_name(&self, which: Which) -> String {
        match which {
            Which::Shared => SHARED_FILE.to_string(),
            Which::App => format!("{}.toml", self.app),
        }
    }

    /// Move a file to the folder `to` (the default folder for "Use
    /// default"). The old file stays where it was, renamed as a backup;
    /// a file already at `to` is backed up too unless it is used. Moving
    /// the shared file moves it for every app, through the pointer. Returns
    /// the backups made
    pub fn move_file(&mut self, which: Which, to: &Path, how: Move) -> Result<Vec<PathBuf>, WriteError> {
        let from = self.path(which);
        let name = self.file_name(which);
        let target = to.join(&name);
        if target == from {
            return Ok(Vec::new());
        }
        if self.problem(which).is_some() && how == Move::Carry {
            return Err(WriteError::Unreadable(self.problem(which).cloned().expect("checked")));
        }
        fs::create_dir_all(to)?;
        let mut backups = Vec::new();
        {
            // Both folders locked, in one order, so two apps moving at once
            // cannot wait on each other
            let mut dirs = [self.dir(which).to_path_buf(), to.to_path_buf()];
            dirs.sort();
            let _first = lock(&dirs[0])?;
            let _second = if dirs[0] != dirs[1] { Some(lock(&dirs[1])?) } else { None };
            match how {
                Move::Carry => {
                    let text = match fs::read_to_string(&from) {
                        Ok(text) => Some(text),
                        Err(e) if e.kind() == io::ErrorKind::NotFound => None,
                        Err(e) => return Err(e.into()),
                    };
                    if target.exists() {
                        backups.push(back_up(&target)?);
                    }
                    if let Some(text) = text {
                        write_atomic(&target, &text)?;
                    }
                }
                Move::StartEmpty => {
                    if target.exists() {
                        backups.push(back_up(&target)?);
                    }
                    let mut keep = String::new();
                    // This app's own switch survives an empty start
                    if which == Which::App && !self.use_shared() {
                        keep = format!("{USE_SHARED} = false\n");
                    }
                    write_atomic(&target, &keep)?;
                }
                Move::UseExisting => {}
            }
            if from.exists() {
                backups.push(back_up(&from)?);
            }
        }
        self.point(which, to)?;
        self.reload();
        Ok(backups)
    }

    /// The pointer set to `to` for a file, or that file's entry taken out
    /// when `to` is the default folder
    fn point(&mut self, which: Which, to: &Path) -> Result<(), WriteError> {
        let name = match which {
            Which::Shared => POINTER_SHARED,
            Which::App => self.app,
        };
        let default = to == self.default_dir;
        let place = display_path_in(to, self.home.as_deref());
        let path = self.pointer_path();
        let _lock = lock(&self.default_dir)?;
        let mut doc = read_doc(&path).map_err(|p| WriteError::Unreadable(*p))?;
        if doc.is_empty() && doc.decor().prefix().is_none() {
            doc.decor_mut().set_prefix(
                "# Where the household apps keep their settings files, when not in this folder:\n# `shared` for shared.toml, and an app's id for its own file. Settings → Settings files changes it.\n",
            );
        }
        if default {
            doc.remove(name);
        } else {
            set_item(&mut doc, name, toml_edit::value(place));
        }
        write_atomic(&path, &doc.to_string())?;
        Ok(())
    }

    /// Read-modify-write one file under its folder's lock, from what is on
    /// disk now (another app may have written since), then take up what
    /// was written
    fn edit(&mut self, which: Which, change: impl FnOnce(&mut DocumentMut)) -> Result<(), WriteError> {
        let path = self.path(which);
        let dir = self.dir(which).to_path_buf();
        fs::create_dir_all(&dir)?;
        let text = {
            let _lock = lock(&dir)?;
            let mut doc = read_doc(&path).map_err(|p| WriteError::Unreadable(*p))?;
            change(&mut doc);
            let text = doc.to_string();
            write_atomic(&path, &text)?;
            text
        };
        let state = match which {
            Which::Shared => &mut self.shared,
            Which::App => &mut self.own,
        };
        take_text(state, &path, Which::App == which, which, &text);
        Ok(())
    }

    /// Watch the folders for changes to the pointer and both files, on
    /// `notify`'s own thread: `changed` is called (often several times for
    /// one save; the app should wait a moment, then reload once). Keep the
    /// watch for as long as the app should hear; watch again after a file
    /// has moved
    pub fn watch(&self, changed: impl Fn() + Send + 'static) -> notify::Result<Watch> {
        use notify::{RecursiveMode, Watcher};
        let names: Vec<OsString> =
            [POINTER_FILE.to_string(), SHARED_FILE.to_string(), self.file_name(Which::App)].into_iter().map(OsString::from).collect();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            if event.kind.is_access() {
                return;
            }
            if event.paths.iter().any(|p| p.file_name().is_some_and(|n| names.iter().any(|w| w == n))) {
                changed();
            }
        })?;
        let mut dirs = vec![self.default_dir.clone(), self.shared_dir.clone(), self.app_dir.clone()];
        dirs.sort();
        dirs.dedup();
        for dir in &dirs {
            // A missing folder is reported by the reload; the others are
            // still watched
            let _ = watcher.watch(dir, RecursiveMode::NonRecursive);
        }
        Ok(Watch { _watcher: watcher, dirs })
    }
}

/// A running watch: dropping it stops it
pub struct Watch {
    _watcher: notify::RecommendedWatcher,
    dirs: Vec<PathBuf>,
}

impl Watch {
    /// The folders watched, one inotify watch each
    pub fn dirs(&self) -> &[PathBuf] {
        &self.dirs
    }
}

impl std::fmt::Debug for Watch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Watch").field("dirs", &self.dirs).finish()
    }
}

/// A whole shared file: every key with what it holds, its value, and the
/// line commented where the value is the default
pub fn listing(values: &BTreeMap<&'static str, Value>) -> String {
    let mut text = String::from(
        "# The household's shared settings, read by every household app.\n# A line starting with # is the default: take the # away to change it.\n",
    );
    for key in KEYS {
        let default = key.default.value();
        let value = values.get(key.name).cloned().unwrap_or_else(|| default.clone());
        let mut doc = DocumentMut::new();
        doc.insert(key.name, value.to_item());
        let line = doc.to_string();
        let mark = if value == default { "# " } else { "" };
        text.push_str(&format!("\n# {}\n{mark}{}", key.about, line));
    }
    text
}

/// `name = item` in `doc`, keeping the line's comments when it was there
fn set_item(doc: &mut DocumentMut, name: &str, item: Item) {
    match doc.get_mut(name) {
        Some(old) if old.is_value() => {
            let decor = old.as_value().map(|v| v.decor().clone());
            *old = item;
            if let (Some(decor), Some(v)) = (decor, old.as_value_mut()) {
                *v.decor_mut() = decor;
            }
        }
        _ => {
            doc.insert(name, item);
        }
    }
}

/// The 1-based line of byte `at` in `text`
fn line_of(text: &str, at: usize) -> usize {
    text[..at.min(text.len())].bytes().filter(|b| *b == b'\n').count() + 1
}

/// The line a top-level key is written on
fn line_of_key(text: &str, name: &str) -> Option<usize> {
    text.lines().position(|l| {
        let l = l.trim_start();
        let l = l.strip_prefix('"').and_then(|r| r.strip_prefix(name)).and_then(|r| r.strip_prefix('"')).or_else(|| l.strip_prefix(name));
        l.is_some_and(|r| r.trim_start().starts_with('='))
    })
    .map(|i| i + 1)
}

/// A file's document, empty if there is none yet
fn read_doc(path: &Path) -> Result<DocumentMut, Box<Problem>> {
    match fs::read_to_string(path) {
        Ok(text) => text.parse::<DocumentMut>().map_err(|e| {
            let line = e.span().map(|s| line_of(&text, s.start));
            Box::new(Problem { which: None, path: path.to_path_buf(), kind: ProblemKind::Syntax { line }, detail: e.to_string() })
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(DocumentMut::new()),
        Err(e) => Err(Box::new(Problem { which: None, path: path.to_path_buf(), kind: ProblemKind::Unreadable, detail: e.to_string() })),
    }
}

/// Read one file into `state`, keeping its values when it cannot be read
fn read_into(state: &mut FileState, path: &Path, which: Which, app_file: bool) {
    match fs::read_to_string(path) {
        Ok(text) => take_text(state, path, app_file, which, &text),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            let folder_gone = path.parent().is_some_and(|d| !d.exists());
            state.exists = false;
            if folder_gone {
                state.problem = Some(Problem { which: Some(which), path: path.to_path_buf(), kind: ProblemKind::FolderMissing, detail: e.to_string() });
            } else {
                // No file yet: every key is the one below
                state.values.clear();
                state.use_shared = None;
                state.problem = None;
            }
        }
        Err(e) => {
            state.exists = true;
            state.problem = Some(Problem { which: Some(which), path: path.to_path_buf(), kind: ProblemKind::Unreadable, detail: e.to_string() });
        }
    }
}

/// A file's text taken into `state`, or its problem noted with the values
/// kept as they were
fn take_text(state: &mut FileState, path: &Path, app_file: bool, which: Which, text: &str) {
    state.exists = true;
    let doc = match text.parse::<DocumentMut>() {
        Ok(doc) => doc,
        Err(e) => {
            let line = e.span().map(|s| line_of(text, s.start));
            state.problem = Some(Problem { which: Some(which), path: path.to_path_buf(), kind: ProblemKind::Syntax { line }, detail: e.to_string() });
            return;
        }
    };
    let mut values = BTreeMap::new();
    for key in KEYS {
        let Some(item) = doc.get(key.name) else { continue };
        match Value::from_item(item, key.kind()) {
            Some(v) => {
                values.insert(key.name, v);
            }
            None => {
                let line = line_of_key(text, key.name);
                state.problem = Some(Problem {
                    which: Some(which),
                    path: path.to_path_buf(),
                    kind: ProblemKind::WrongKind { key: key.name, line, wanted: key.kind() },
                    detail: format!("{} = {}", key.name, item.to_string().trim()),
                });
                return;
            }
        }
    }
    // Keys the household does not share (yet) are left in the file and
    // ignored, for newer apps that know them
    state.values = values;
    state.use_shared = if app_file { doc.get(USE_SHARED).and_then(Item::as_bool) } else { None };
    state.problem = None;
}

/// The folder's lock, held until dropped; waits at most [`LOCK_DEADLINE`]
fn lock(dir: &Path) -> Result<File, WriteError> {
    let file = OpenOptions::new().create(true).truncate(false).write(true).open(dir.join(LOCK_FILE))?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(file),
            Err(fs::TryLockError::WouldBlock) if start.elapsed() < LOCK_DEADLINE => std::thread::sleep(Duration::from_millis(20)),
            Err(fs::TryLockError::WouldBlock) => return Err(WriteError::Busy),
            Err(fs::TryLockError::Error(e)) => return Err(e.into()),
        }
    }
}

/// `text` written beside `path` and renamed over it, so a reader sees the
/// old file or the new one and never half of one
fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = path.parent().unwrap_or(Path::new("."));
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.tmp-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let result = (|| {
        let mut file = File::create(&temp)?;
        file.write_all(text.as_bytes())?;
        file.sync_all()?;
        // The new file keeps the old one's permissions
        if let Ok(meta) = fs::metadata(path) {
            fs::set_permissions(&temp, meta.permissions())?;
        }
        fs::rename(&temp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

/// `path` renamed beside itself as `<name>.bak` (or `.bak-2`, `.bak-3` …
/// when there is one already); returns the backup's path
fn back_up(path: &Path) -> io::Result<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut backup = dir.join(format!("{name}.bak"));
    let mut n = 2;
    while backup.exists() {
        backup = dir.join(format!("{name}.bak-{n}"));
        n += 1;
    }
    fs::rename(path, &backup)?;
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc;

    /// A home of the test's own, never the real one
    struct Home(PathBuf);

    impl Home {
        fn new(name: &str) -> Home {
            static N: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!("household-settings-{name}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(dir.join(".config")).unwrap();
            Home(dir)
        }

        fn default_dir(&self) -> PathBuf {
            self.0.join(".config").join(DIR_NAME)
        }

        fn store(&self, app: &'static str) -> Store {
            Store::open_in(self.default_dir(), Some(self.0.clone()), app)
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_folder_is_xdg_config_home_s_or_the_host_s_in_a_flatpak() {
        let env = |pairs: &'static [(&'static str, &'static str)]| move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| OsString::from(v));
        let plain = env(&[("HOME", "/home/j"), ("XDG_CONFIG_HOME", "/home/j/.cfg")]);
        assert_eq!(default_dir_from(plain, false), PathBuf::from("/home/j/.cfg/household"));
        let relative = env(&[("HOME", "/home/j"), ("XDG_CONFIG_HOME", "cfg")]);
        assert_eq!(default_dir_from(relative, false), PathBuf::from("/home/j/.config/household"));
        // In a Flatpak, XDG_CONFIG_HOME is the sandbox's own
        let sandbox = env(&[("HOME", "/home/j"), ("XDG_CONFIG_HOME", "/home/j/.var/app/dev.twright.Cerea/config")]);
        assert_eq!(default_dir_from(sandbox, true), PathBuf::from("/home/j/.config/household"));
        let host = env(&[("HOME", "/home/j"), ("XDG_CONFIG_HOME", "/home/j/.var/app/x/config"), ("HOST_XDG_CONFIG_HOME", "/home/j/.cfg")]);
        assert_eq!(default_dir_from(host, true), PathBuf::from("/home/j/.cfg/household"));
        assert_eq!(display_path_in(Path::new("/home/j/.config/household"), Some(Path::new("/home/j"))), "~/.config/household");
        assert_eq!(display_path_in(Path::new("/mnt/x"), Some(Path::new("/home/j"))), "/mnt/x");
    }

    #[test]
    fn a_key_is_this_app_s_then_shared_then_the_default() {
        let home = Home::new("lookup");
        let mut s = home.store("terminal");
        assert_eq!((s.int("prose-size"), s.source("prose-size")), (16, Source::Default));
        assert_eq!(s.chain("prose-size"), Some(Chain::Linked));
        s.set("prose-size", Value::Int(17)).unwrap();
        assert_eq!((s.int("prose-size"), s.source("prose-size")), (17, Source::Shared));
        // Cerea sees the shared value
        let cerea = home.store("cerea");
        assert_eq!(cerea.int("prose-size"), 17);
        // This app only: kept in terminal.toml, Cerea unchanged
        s.make_own("prose-size").unwrap();
        assert_eq!((s.source("prose-size"), s.chain("prose-size")), (Source::App, Some(Chain::Own)));
        s.set("prose-size", Value::Int(18)).unwrap();
        assert_eq!(s.int("prose-size"), 18);
        assert_eq!(home.store("cerea").int("prose-size"), 17);
        // Use shared: back to 17
        s.remove_own("prose-size").unwrap();
        assert_eq!((s.int("prose-size"), s.source("prose-size")), (17, Source::Shared));
        // Typed accessors
        s.set("paper-line-spacing", Value::Float(1.6)).unwrap();
        s.set("theme", Value::Text("dark".into())).unwrap();
        s.set("reduce-motion", Value::Bool(true)).unwrap();
        assert_eq!((s.float("paper-line-spacing"), s.text("theme"), s.bool("reduce-motion")), (1.6, "dark".to_string(), true));
        assert!(fs::read_to_string(home.default_dir().join("shared.toml")).unwrap().contains("paper-line-spacing = 1.6"));
    }

    #[test]
    fn writes_keep_comments_and_layout_and_unknown_keys() {
        let home = Home::new("comments");
        let dir = home.default_dir();
        fs::create_dir_all(&dir).unwrap();
        let text = "# Juliet's household settings\n\n# Bigger for the laptop\nprose-size = 17  # was 16\n\nsomething-new = \"kept\"\n";
        fs::write(dir.join(SHARED_FILE), text).unwrap();
        let mut s = home.store("terminal");
        s.set("prose-size", Value::Int(18)).unwrap();
        s.set("paper-size", Value::Int(22)).unwrap();
        let after = fs::read_to_string(dir.join(SHARED_FILE)).unwrap();
        assert!(after.starts_with("# Juliet's household settings\n\n# Bigger for the laptop\nprose-size = 18  # was 16\n"), "{after}");
        assert!(after.contains("something-new = \"kept\""));
        assert!(after.contains("paper-size = 22"));
        // No temporary files left; the lock file is the only other
        let mut names: Vec<_> = fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        names.sort();
        assert_eq!(names, [".lock", "shared.toml"]);
    }

    #[test]
    fn an_unreadable_file_keeps_the_last_good_values_and_says_why() {
        let home = Home::new("unreadable");
        let mut s = home.store("terminal");
        s.set("prose-size", Value::Int(18)).unwrap();
        let path = home.default_dir().join(SHARED_FILE);
        fs::write(&path, "# one\nprose-size = 18\nthis is not toml\n").unwrap();
        s.reload();
        assert_eq!(s.int("prose-size"), 18, "the last good value");
        let p = s.problem(Which::Shared).expect("a problem");
        assert_eq!(p.kind, ProblemKind::Syntax { line: Some(3) });
        assert_eq!(
            p.sentence("Terminal"),
            "Could not read the shared file: line 3 is not written as a setting. The apps keep the settings they had until it is fixed."
        );
        // A write would lose the file's text, so it is refused
        assert!(matches!(s.set("paper-size", Value::Int(22)), Err(WriteError::Unreadable(_))));
        assert_eq!(fs::read_to_string(&path).unwrap(), "# one\nprose-size = 18\nthis is not toml\n");
        // The wrong kind of value, named
        fs::write(&path, "prose-size = 18\npaper-size = \"big\"\n").unwrap();
        s.reload();
        let p = s.problem(Which::Shared).unwrap();
        assert_eq!(p.kind, ProblemKind::WrongKind { key: "paper-size", line: Some(2), wanted: Kind::Int });
        assert!(p.sentence("Terminal").starts_with("Could not read the shared file: line 2 gives paper-size something other than a whole number."));
        // Fixed: read again, the problem gone
        fs::write(&path, "prose-size = 15\n").unwrap();
        s.reload();
        assert!(s.problems().is_empty());
        assert_eq!(s.int("prose-size"), 15);
        // An integer where a number is wanted, and a whole float for an integer
        fs::write(&path, "paper-line-spacing = 2\nprose-size = 15.0\n").unwrap();
        s.reload();
        assert_eq!((s.float("paper-line-spacing"), s.int("prose-size")), (2.0, 15));
    }

    #[test]
    fn a_missing_folder_is_a_problem_not_an_empty_file() {
        let home = Home::new("missing");
        let mut s = home.store("terminal");
        let elsewhere = home.0.join("disk");
        s.set("prose-size", Value::Int(19)).unwrap();
        s.move_file(Which::Shared, &elsewhere, Move::Carry).unwrap();
        assert_eq!(s.int("prose-size"), 19);
        fs::remove_dir_all(&elsewhere).unwrap();
        s.reload();
        assert_eq!(s.int("prose-size"), 19, "kept while the disk is away");
        let p = s.problem(Which::Shared).unwrap();
        assert_eq!(p.kind, ProblemKind::FolderMissing);
        assert!(s.sentence(p, "Terminal").starts_with("Could not find ~/disk: is its disk connected?"), "{}", s.sentence(p, "Terminal"));
    }

    #[test]
    fn moving_the_shared_file_moves_it_for_every_app_and_keeps_a_backup() {
        let home = Home::new("move");
        let mut s = home.store("terminal");
        s.set("prose-size", Value::Int(17)).unwrap();
        let mut cerea = home.store("cerea");
        cerea.migrate(&[]).unwrap();
        let place = home.0.join("machine-setup/household-apps");
        assert!(!s.has_file_at(Which::Shared, &place));
        let backups = s.move_file(Which::Shared, &place, Move::Carry).unwrap();
        assert_eq!(backups, [home.default_dir().join("shared.toml.bak")]);
        assert_eq!(s.path(Which::Shared), place.join(SHARED_FILE));
        assert!(!s.is_default(Which::Shared) && s.is_default(Which::App));
        assert_eq!(s.int("prose-size"), 17);
        let pointer = fs::read_to_string(home.default_dir().join(POINTER_FILE)).unwrap();
        assert!(pointer.contains("shared = \"~/machine-setup/household-apps\""), "{pointer}");
        assert!(pointer.starts_with("# Where the household apps keep"));
        // Cerea follows on its next read
        cerea.reload();
        assert_eq!((cerea.path(Which::Shared), cerea.int("prose-size")), (place.join(SHARED_FILE), 17));
        // Back to the default place: the moved file's backup there is kept,
        // the new one backed up where it was
        let backups = s.move_file(Which::Shared, &home.default_dir(), Move::Carry).unwrap();
        assert_eq!(backups, [place.join("shared.toml.bak")]);
        assert!(s.is_default(Which::Shared));
        assert_eq!(s.int("prose-size"), 17);
        assert!(home.default_dir().join("shared.toml.bak").exists());
        assert!(!fs::read_to_string(home.default_dir().join(POINTER_FILE)).unwrap().contains("shared ="));
    }

    #[test]
    fn a_new_place_can_start_empty_or_use_its_file() {
        let home = Home::new("start-empty");
        let mut s = home.store("terminal");
        s.set("prose-size", Value::Int(17)).unwrap();
        let empty = home.0.join("empty");
        s.move_file(Which::Shared, &empty, Move::StartEmpty).unwrap();
        assert_eq!((s.int("prose-size"), s.source("prose-size")), (16, Source::Default));
        assert!(home.default_dir().join("shared.toml.bak").exists());
        // A place with a file of its own: used as it is
        let other = home.0.join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join(SHARED_FILE), "prose-size = 20\n").unwrap();
        assert!(s.has_file_at(Which::Shared, &other));
        s.move_file(Which::Shared, &other, Move::UseExisting).unwrap();
        assert_eq!(s.int("prose-size"), 20);
        assert!(empty.join("shared.toml.bak").exists());
        // Or replaced by the current one, which keeps it as a backup
        let third = home.0.join("third");
        fs::create_dir_all(&third).unwrap();
        fs::write(third.join(SHARED_FILE), "prose-size = 12\n").unwrap();
        s.move_file(Which::Shared, &third, Move::Carry).unwrap();
        assert_eq!(s.int("prose-size"), 20);
        assert_eq!(fs::read_to_string(third.join("shared.toml.bak")).unwrap(), "prose-size = 12\n");
        // A second backup in one place does not overwrite the first
        fs::write(other.join(SHARED_FILE), "x = 1\n").unwrap();
        s.move_file(Which::Shared, &other, Move::Carry).unwrap();
        assert!(other.join("shared.toml.bak").exists() && other.join("shared.toml.bak-2").exists());
    }

    #[test]
    fn this_app_s_file_moves_on_its_own() {
        let home = Home::new("app-move");
        let mut s = home.store("terminal");
        s.make_own("theme").unwrap();
        s.set("theme", Value::Text("dark".into())).unwrap();
        let place = home.0.join("mine");
        s.move_file(Which::App, &place, Move::Carry).unwrap();
        assert_eq!(s.path(Which::App), place.join("terminal.toml"));
        assert_eq!((s.text("theme"), s.source("theme")), ("dark".to_string(), Source::App));
        assert!(s.is_default(Which::Shared));
        let pointer = fs::read_to_string(home.default_dir().join(POINTER_FILE)).unwrap();
        assert!(pointer.contains("terminal = \"~/mine\""));
        // Cerea's file is not moved
        assert_eq!(home.store("cerea").path(Which::App), home.default_dir().join("cerea.toml"));
        assert_eq!(home.store("terminal").text("theme"), "dark");
        assert_eq!(s.users(), ["Terminal"]);
        home.store("cerea").migrate(&[]).unwrap();
        assert_eq!(s.users(), ["Terminal", "Cerea"]);
    }

    #[test]
    fn shared_off_keeps_what_is_shown_and_on_again_undoes_it() {
        let home = Home::new("toggle");
        let mut s = home.store("terminal");
        s.set("prose-size", Value::Int(17)).unwrap();
        s.set("paper-size", Value::Int(22)).unwrap();
        s.make_own("theme").unwrap();
        s.set("theme", Value::Text("dark".into())).unwrap();
        let before = fs::read_to_string(s.path(Which::App)).unwrap();
        let rows = ["prose-size", "paper-size", "theme", "interface-size"];
        s.set_use_shared(false, &rows).unwrap();
        assert!(!s.use_shared());
        assert_eq!(s.chain("prose-size"), None);
        assert_eq!((s.int("prose-size"), s.int("paper-size"), s.text("theme"), s.int("interface-size")), (17, 22, "dark".into(), 14));
        // The shared file changing no longer reaches this app
        home.store("cerea").set("prose-size", Value::Int(13)).unwrap();
        s.reload();
        assert_eq!(s.int("prose-size"), 17);
        // A change while off goes to this app's file
        s.set("interface-size", Value::Int(15)).unwrap();
        assert_eq!(home.store("cerea").int("interface-size"), 14);
        s.set_use_shared(true, &rows).unwrap();
        assert!(s.use_shared());
        // 17 ≠ shared 13 now, so it stays this app's own; paper-size equals
        // the shared 22 and returns to linked
        assert_eq!(s.chain("prose-size"), Some(Chain::Own));
        assert_eq!(s.chain("paper-size"), Some(Chain::Linked));
        assert_eq!(s.chain("interface-size"), Some(Chain::Own));
        // Reset this app to shared
        assert!(s.any_own(&rows));
        s.reset_to_shared(&rows).unwrap();
        assert!(!s.any_own(&rows));
        assert_eq!((s.int("prose-size"), s.text("theme")), (13, "system".into()));
        let _ = before;
    }

    #[test]
    fn a_round_trip_through_shared_off_leaves_the_file_as_it_was() {
        let home = Home::new("round-trip");
        let mut s = home.store("terminal");
        s.set("prose-size", Value::Int(17)).unwrap();
        s.migrate(&[]).unwrap();
        let before = fs::read_to_string(s.path(Which::App)).unwrap();
        s.set_use_shared(false, &["prose-size", "theme"]).unwrap();
        s.set_use_shared(true, &["prose-size", "theme"]).unwrap();
        assert_eq!(fs::read_to_string(s.path(Which::App)).unwrap(), before);
    }

    #[test]
    fn migration_seeds_the_shared_file_then_keeps_only_what_differs() {
        let home = Home::new("migrate");
        let mut s = home.store("terminal");
        let current = [
            ("theme", Value::Text("dark".into())),
            ("prose-size", Value::Int(16)),
            ("paper-size", Value::Int(22)),
            ("code-font", Value::Text("Noto Sans Mono".into())),
            ("paper-line-spacing", Value::Float(1.5)),
        ];
        // The first app: its values fill the shared file, and are shared
        assert!(s.migrate(&current).unwrap());
        let shared = fs::read_to_string(s.path(Which::Shared)).unwrap();
        assert!(shared.starts_with("# The household's shared settings"), "{shared}");
        assert!(shared.contains("\ntheme = \"dark\"") && shared.contains("\npaper-size = 22"), "{shared}");
        assert!(shared.contains("\n# prose-size = 16") && shared.contains("\n# code-font = \"Noto Sans Mono\""), "defaults commented: {shared}");
        for key in KEYS {
            assert!(shared.contains(&format!("{} = ", key.name)), "every key listed: {}", key.name);
        }
        let own = fs::read_to_string(s.path(Which::App)).unwrap();
        assert!(own.starts_with("# terminal's own settings") && !own.contains(" = "), "nothing for this app only: {own}");
        assert_eq!((s.text("theme"), s.source("theme"), s.chain("theme")), ("dark".into(), Source::Shared, Some(Chain::Linked)));
        assert_eq!((s.int("paper-size"), s.int("prose-size")), (22, 16));
        // Once: a second run changes nothing
        assert!(!s.migrate(&[("theme", Value::Text("light".into()))]).unwrap());
        assert_eq!(s.text("theme"), "dark");
        // A later app: overrides only where it differs from the shared file
        let mut cerea = home.store("cerea");
        assert!(cerea.migrate(&[("theme", Value::Text("light".into())), ("paper-size", Value::Int(22)), ("prose-size", Value::Int(16))]).unwrap());
        assert_eq!((cerea.text("theme"), cerea.source("theme")), ("light".into(), Source::App), "light kept: nothing visibly changes");
        assert_eq!(cerea.source("paper-size"), Source::Shared);
        assert_eq!(fs::read_to_string(s.path(Which::Shared)).unwrap(), shared, "the shared file is not changed by a later app");
    }

    #[test]
    fn two_first_runs_at_once_seed_once() {
        let home = Home::new("seed-race");
        let threads: Vec<_> = [("terminal", 17), ("cerea", 19)]
            .into_iter()
            .map(|(app, size)| {
                let (dir, h) = (home.default_dir(), home.0.clone());
                std::thread::spawn(move || {
                    let mut s = Store::open_in(dir, Some(h), app);
                    s.migrate(&[("prose-size", Value::Int(size))]).unwrap();
                    (size, s.int("prose-size"), s.source("prose-size"))
                })
            })
            .collect();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        // Each sees its own size; one of them through the shared file
        assert!(results.iter().all(|(size, seen, _)| size == seen), "{results:?}");
        assert_eq!(results.iter().filter(|(_, _, src)| *src == Source::Shared).count(), 1, "{results:?}");
    }

    #[test]
    fn opening_a_missing_shared_file_makes_the_listing() {
        let home = Home::new("open");
        let mut s = home.store("terminal");
        // Moved to an empty place: no shared file there yet
        s.set("prose-size", Value::Int(18)).unwrap();
        s.move_file(Which::Shared, &home.0.join("new"), Move::StartEmpty).unwrap();
        fs::remove_file(s.path(Which::Shared)).unwrap();
        s.reload();
        assert!(!s.exists(Which::Shared));
        let path = s.create_shared_file().unwrap();
        assert_eq!(path, home.0.join("new").join(SHARED_FILE));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# prose-size = 16") && text.contains("# Light or dark"), "{text}");
        assert!(s.exists(Which::Shared) && s.problems().is_empty());
        // Taking a # away is read as a setting; a second Open keeps the file
        fs::write(&path, text.replace("# prose-size = 16", "prose-size = 15")).unwrap();
        s.reload();
        assert_eq!(s.int("prose-size"), 15);
        s.create_shared_file().unwrap();
        assert!(fs::read_to_string(&path).unwrap().contains("\nprose-size = 15"));
    }

    #[test]
    fn concurrent_writers_lose_nothing() {
        let home = Home::new("concurrent");
        let dir = home.default_dir();
        let names = ["prose-size", "paper-size", "interface-size", "caption-size", "command-size", "tooltip-delay"];
        let threads: Vec<_> = names
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let dir = dir.clone();
                let home = home.0.clone();
                let name = *name;
                std::thread::spawn(move || {
                    let mut s = Store::open_in(dir, Some(home), APPS[i % APPS.len()].id);
                    for n in 0..10 {
                        s.set_in(Which::Shared, name, Value::Int(100 + n)).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        let s = home.store("terminal");
        for name in names {
            assert_eq!(s.int(name), 109, "{name}");
        }
    }

    #[test]
    fn a_held_lock_times_out_as_busy() {
        let home = Home::new("busy");
        let mut s = home.store("terminal");
        fs::create_dir_all(home.default_dir()).unwrap();
        let held = lock(&home.default_dir()).unwrap();
        let start = Instant::now();
        assert!(matches!(s.set("prose-size", Value::Int(17)), Err(WriteError::Busy)));
        assert!(start.elapsed() >= LOCK_DEADLINE);
        drop(held);
        s.set("prose-size", Value::Int(17)).unwrap();
    }

    /// Wait up to `deadline` for the watch to call back, polling, so a
    /// loaded machine's late events still count
    fn heard(rx: &mpsc::Receiver<()>, deadline: Duration) -> bool {
        let start = Instant::now();
        while start.elapsed() < deadline {
            if rx.try_recv().is_ok() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    /// Let one save's events (the temporary file, then the rename, which
    /// may come as one rename event or as a remove and a create) all
    /// arrive: wait until none has come for `QUIET`, within 10 s
    fn settle(rx: &mpsc::Receiver<()>) {
        const QUIET: Duration = Duration::from_secs(1);
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(10) && heard(rx, QUIET) {}
    }

    #[test]
    fn the_watch_hears_another_app_s_write_and_a_move() {
        const DEADLINE: Duration = Duration::from_secs(10);
        let home = Home::new("watch");
        let s = home.store("terminal");
        let (tx, rx) = mpsc::channel();
        let watch = s.watch(move || {
            let _ = tx.send(());
        })
        .unwrap();
        assert_eq!(watch.dirs(), [home.default_dir()], "one watch while all is in one folder");
        let mut cerea = home.store("cerea");
        cerea.set("prose-size", Value::Int(19)).unwrap();
        assert!(heard(&rx, DEADLINE), "heard the shared file change");
        settle(&rx);
        // Cerea's own file is not this app's business: nothing more comes
        cerea.make_own("theme").unwrap();
        assert!(!heard(&rx, Duration::from_secs(1)), "Cerea's file is not watched");
        let mut s = s;
        s.reload();
        assert_eq!(s.int("prose-size"), 19);
        // This app's own file, written by another store, is heard
        home.store("terminal").make_own("theme").unwrap();
        assert!(heard(&rx, DEADLINE), "heard this app's file change");
        settle(&rx);
        // The shared file moved by another app: the pointer changes
        cerea.move_file(Which::Shared, &home.0.join("elsewhere"), Move::Carry).unwrap();
        assert!(heard(&rx, DEADLINE), "heard the pointer change");
        s.reload();
        assert_eq!(s.path(Which::Shared), home.0.join("elsewhere").join(SHARED_FILE));
        // Watched again, the new folder too; a write there is heard
        drop(watch);
        let (tx, rx) = mpsc::channel();
        let watch = s
            .watch(move || {
                let _ = tx.send(());
            })
            .unwrap();
        assert_eq!(watch.dirs().len(), 2);
        cerea.set("paper-size", Value::Int(22)).unwrap();
        assert!(heard(&rx, DEADLINE), "heard the shared file in its new place");
    }

    #[test]
    fn every_key_has_a_default_of_its_kind_and_a_unique_name() {
        let mut names: Vec<_> = KEYS.iter().map(|k| k.name).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), KEYS.len());
        assert!(!KEYS.iter().any(|k| k.name == USE_SHARED));
        assert_eq!(key("interface-size").unwrap().default, Fixed::Int(14));
        assert_eq!(key("prose-size").unwrap().default, Fixed::Int(16));
        assert_eq!(key("paper-size").unwrap().default, Fixed::Int(20));
        assert_eq!(key("caption-size").unwrap().default, Fixed::Int(12));
        assert_eq!(key("command-size").unwrap().default, Fixed::Int(14));
    }
}
