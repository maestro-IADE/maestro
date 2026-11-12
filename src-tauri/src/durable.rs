//! How state files under `~/.maestro` (and the project folders next to it) are written and read, so a crash, a full
//! disk or a newer Maestro never costs the user a record:
//! - every write goes to a uniquely named temp file next to the target, is flushed to disk (`sync_all`) and renamed
//!   over it (retried while Windows' antivirus or indexer holds the file);
//! - before a JSON state file is replaced, the previous one is kept as `<name>.bak` when it was readable, or moved
//!   aside to `<name>.corrupt-<ms>` when it wasn't: never silently dropped;
//! - each file carries a format `version` (an object's key; a list stays bare at version 1 and goes in
//!   `{"version", "items"}` only from version 2); a file without one is version 1. A file
//!   written by a newer Maestro (a higher version than this build knows) is read as an error and never overwritten.
//!
//! Problems the user should know about go through `notice`: a warning dialog in the app (main.rs sets the sink), once
//! per file per run.

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

/// Why a state file couldn't be used.
#[derive(Debug, Clone, PartialEq)]
pub enum StateError {
    /// not JSON, or not the shape this build reads (and no readable `.bak`)
    Corrupt { path: String, detail: String },
    /// written by a newer Maestro: format `found`, this build knows up to `newest`
    TooNew { path: String, found: u64, newest: u64 },
    Io(String),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::Corrupt { path, detail } => write!(f, "{path} 파일이 손상돼 읽지 못했어요 ({detail})"),
            StateError::TooNew { path, found, newest } => {
                write!(f, "{path} 파일은 더 새 Maestro(형식 v{found})가 쓴 거라 이 버전(v{newest})은 바꾸지 않아요. 최신 Maestro로 열어 주세요")
            }
            StateError::Io(e) => f.write_str(e),
        }
    }
}

impl From<StateError> for String {
    fn from(e: StateError) -> String {
        e.to_string()
    }
}

impl StateError {
    /// The error and what happens next, for a reader that carries on without the file.
    pub fn advice(&self) -> String {
        match self {
            StateError::Corrupt { .. } => format!("{self}. 비어 있는 것으로 보고 계속하고, 손상된 파일은 다음 저장 때 옆(.corrupt-…)에 옮겨 둬요."),
            StateError::TooNew { .. } => format!("{self}. 이번 실행에서 바뀐 내용은 이 파일에 저장되지 않아요."),
            StateError::Io(_) => self.to_string(),
        }
    }
}

fn shown(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// `<path>.<suffix>`, next to it (`repos.json` → `repos.json.bak`).
fn beside(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".{suffix}"));
    path.with_file_name(name)
}

/// The previous good copy kept next to a state file.
pub fn backup_path(path: &Path) -> PathBuf {
    beside(path, "bak")
}

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis())
}

/// Sharing violations from Windows' antivirus or indexer holding the file for a moment: worth another try.
fn transient(e: &std::io::Error) -> bool {
    e.kind() == ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

fn retry<T>(mut f: impl FnMut() -> std::io::Result<T>) -> std::io::Result<T> {
    let mut wait = 25;
    for _ in 0..4 {
        match f() {
            Err(e) if transient(&e) => std::thread::sleep(Duration::from_millis(wait)),
            other => return other,
        }
        wait *= 2;
    }
    f()
}

/// Writes `text` to `path` so a crash leaves either the old file or the new one, whole: a temp file of its own in the
/// same folder (two writers never share one), flushed to disk, then renamed over. On Unix the folder is flushed too;
/// Windows can't open a folder for that through std, and NTFS journals the rename itself.
pub fn atomic_write(path: &Path, text: &str) -> Result<(), String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", shown(dir)))?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.{}.tmp", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    let written = File::create(&tmp).and_then(|mut f| {
        f.write_all(text.as_bytes())?;
        f.sync_all()
    });
    if let Err(e) = written.and_then(|_| retry(|| fs::rename(&tmp, path))) {
        let _ = fs::remove_file(&tmp);
        return Err(format!("{}: {e}", shown(path)));
    }
    #[cfg(unix)]
    if let Ok(d) = File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

/// The format version a state file says it has: `version` as a number, or a string's leading number (the canvas
/// writes "1.0.0"). A file without one (a bare list, an object from before versions) is 1.
pub fn version_of(v: &Value) -> u64 {
    match v.get("version") {
        Some(Value::Number(n)) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)).unwrap_or(1),
        Some(Value::String(s)) => s.trim().split(|c: char| !c.is_ascii_digit()).next().and_then(|d| d.parse().ok()).unwrap_or(1),
        _ => 1,
    }
}

/// Replaces the state file at `path` with `text` (format `newest`, the newest this build knows). The file there now:
/// - written by a newer Maestro: Err, nothing is written (not even `.bak`);
/// - readable (`fits`): kept as `<name>.bak`;
/// - not readable: moved aside to `<name>.corrupt-<ms>` (Err when that fails: then nothing is written);
/// - unreadable for another reason (locked, no access): Err, nothing is written.
fn replace(path: &Path, text: &str, newest: u64, fits: &dyn Fn(Value) -> bool) -> Result<(), String> {
    match retry(|| fs::read_to_string(path)) {
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) if e.kind() == ErrorKind::InvalidData => set_aside(path)?,
        Err(e) => return Err(format!("{} 파일을 읽지 못해 덮어쓰지 않았어요: {e}", shown(path))),
        Ok(old) if old == text => return Ok(()),
        Ok(old) => match serde_json::from_str::<Value>(&old) {
            Ok(v) if version_of(&v) > newest => return Err(StateError::TooNew { path: shown(path), found: version_of(&v), newest }.to_string()),
            Ok(v) if fits(v.clone()) => {
                if let Err(e) = atomic_write(&backup_path(path), &old) {
                    eprintln!("[maestro] backup not written: {e}");
                }
            }
            _ => set_aside(path)?,
        },
    }
    atomic_write(path, text)
}

/// Moves an unreadable state file out of the way (kept for the user, never deleted).
fn set_aside(path: &Path) -> Result<(), String> {
    let ms = now_ms();
    // never over an earlier one moved aside in the same ms
    let aside = (0..).map(|n| beside(path, &if n == 0 { format!("corrupt-{ms}") } else { format!("corrupt-{ms}-{n}") })).find(|p| !p.exists()).unwrap();
    retry(|| fs::rename(path, &aside)).map_err(|e| format!("{} 파일이 손상됐는데 옆으로 옮기지 못해 덮어쓰지 않았어요: {e}", shown(path)))?;
    notice(path, format!("{} 파일이 손상돼 {}(으)로 옮겨 두고 새로 저장했어요.", shown(path), shown(&aside)));
    Ok(())
}

/// `replace` for a file whose content the caller owns (the canvas): only "is it JSON" is checked.
pub fn replace_text(path: &Path, text: &str, newest: u64) -> Result<(), String> {
    replace(path, text, newest, &|_| true)
}

/// Writes `value` as format `version` (also the newest this build reads): an object gets a `version` key (older
/// builds ignore it). A list stays a bare list at format 1, so a Maestro from before versions (sharing `~/.maestro`
/// as a dev build or after a downgrade) still reads it; only a list format of 2 or later goes in `{"version",
/// "items"}`. The file it replaces is kept or moved aside (`replace`).
pub fn write_json<T: Serialize + DeserializeOwned>(path: &Path, value: &T, version: u64) -> Result<(), String> {
    let v = match serde_json::to_value(value).map_err(|e| e.to_string())? {
        Value::Object(mut m) => {
            m.insert("version".into(), version.into());
            Value::Object(m)
        }
        other if version <= 1 => other,
        other => serde_json::json!({ "version": version, "items": other }),
    };
    let text = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    replace(path, &text, version, &|v| unwrap::<T>(v).is_ok())
}

/// The value inside a versioned file: a list's `items`, or the object without its `version`. A bare list (before
/// versions) is itself.
fn unwrap<T: DeserializeOwned>(v: Value) -> Result<T, serde_json::Error> {
    let v = match v {
        Value::Object(mut m) => {
            if m.contains_key("items") && m.keys().all(|k| k == "version" || k == "items") {
                m.remove("items").unwrap_or_default()
            } else {
                m.remove("version");
                Value::Object(m)
            }
        }
        other => other,
    };
    serde_json::from_value(v)
}

fn decode<T: DeserializeOwned>(path: &Path, text: &str, newest: u64) -> Result<T, StateError> {
    let corrupt = |e: serde_json::Error| StateError::Corrupt { path: shown(path), detail: e.to_string() };
    let v: Value = serde_json::from_str(text).map_err(corrupt)?;
    let found = version_of(&v);
    if found > newest {
        return Err(StateError::TooNew { path: shown(path), found, newest });
    }
    unwrap(v).map_err(corrupt)
}

/// Reads a state file with `parse`. Missing: None (a `.bak` left from a deleted file is never brought back). Corrupt:
/// the `.bak` when that one reads (with a notice), else the error. Newer than this build: the error, always.
fn read_with<T>(path: &Path, parse: impl Fn(&str) -> Result<T, StateError>) -> Result<Option<T>, StateError> {
    let first = match retry(|| fs::read_to_string(path)) {
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) if e.kind() == ErrorKind::InvalidData => Err(StateError::Corrupt { path: shown(path), detail: "UTF-8 텍스트가 아니에요".into() }),
        Err(e) => return Err(StateError::Io(format!("{} 파일을 읽지 못했어요: {e}", shown(path)))),
        Ok(text) => parse(&text),
    };
    match first {
        Err(e @ StateError::Corrupt { .. }) => match fs::read_to_string(backup_path(path)).ok().map(|s| parse(&s)) {
            Some(Ok(v)) => {
                notice(path, format!("{e}. 직전 백업({})으로 읽었어요. 손상된 파일은 다음 저장 때 옆(.corrupt-…)에 옮겨 둬요.", shown(&backup_path(path))));
                Ok(Some(v))
            }
            _ => Err(e),
        },
        other => other.map(Some),
    }
}

/// A versioned state file as `T` (`write_json`'s format; a bare list or an object without `version` is format 1).
pub fn read_json<T: DeserializeOwned>(path: &Path, newest: u64) -> Result<Option<T>, StateError> {
    read_with(path, |s| decode(path, s, newest))
}

/// A state file whose content the caller owns (the canvas), checked to be JSON and not newer than `newest`.
pub fn read_text(path: &Path, newest: u64) -> Result<Option<String>, StateError> {
    read_with(path, |s| decode::<Value>(path, s, newest).map(|_| s.to_string()))
}

/// A state file for a reader that carries on without it: missing → the default; unusable → the default too, and the
/// user is told (`advice`).
pub fn read_or_default<T: DeserializeOwned + Default>(path: &Path, newest: u64) -> T {
    match read_json(path, newest) {
        Ok(v) => v.unwrap_or_default(),
        Err(e) => {
            notice(path, e.advice());
            T::default()
        }
    }
}

/// A state file about to be changed and written back: a corrupt one starts from the default (the write moves it
/// aside); a newer or unreadable one is an Err, so nothing is written.
pub fn read_for_write<T: DeserializeOwned + Default>(path: &Path, newest: u64) -> Result<T, String> {
    match read_json(path, newest) {
        Ok(v) => Ok(v.unwrap_or_default()),
        Err(StateError::Corrupt { .. }) => Ok(T::default()),
        Err(e) => Err(e.to_string()),
    }
}

// ---------- telling the user ----------

type Sink = Box<dyn Fn(String) + Send + Sync>;
static SINK: OnceLock<Sink> = OnceLock::new();
/// messages from before the sink was set (state loaded early in setup)
static PENDING: Mutex<Vec<String>> = Mutex::new(Vec::new());
static SEEN: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Tells the user about a state file once per run (a save failing on every poll shows one dialog); always logged.
pub fn notice(path: &Path, msg: String) {
    eprintln!("[maestro] {msg}");
    if !SEEN.lock().unwrap_or_else(|e| e.into_inner()).insert(crate::util::path_key(&path.to_string_lossy())) {
        return;
    }
    let mut pending = PENDING.lock().unwrap_or_else(|e| e.into_inner());
    match SINK.get() {
        Some(sink) => {
            drop(pending);
            sink(msg);
        }
        None => pending.push(msg),
    }
}

/// Where notices go from now on (main.rs: a warning dialog); the ones from before are sent there now.
pub fn set_notice_sink(f: impl Fn(String) + Send + Sync + 'static) {
    let _ = SINK.set(Box::new(f));
    let held: Vec<String> = std::mem::take(&mut *PENDING.lock().unwrap_or_else(|e| e.into_inner()));
    if let Some(sink) = SINK.get() {
        for m in held {
            sink(m);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    struct Dir(PathBuf);
    impl Dir {
        fn new() -> Dir {
            let d = crate::util::unique_temp_dir("maestro-durable");
            fs::create_dir_all(&d).unwrap();
            Dir(d)
        }
        fn names(&self) -> Vec<String> {
            let mut v: Vec<String> = fs::read_dir(&self.0).unwrap().flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
            v.sort();
            v
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
    struct S {
        x: u32,
    }

    #[test]
    fn atomic_write_replaces_whole_and_leaves_no_temp() {
        let d = Dir::new();
        let f = d.0.join("a.json");
        atomic_write(&f, "one").unwrap();
        atomic_write(&f, "two").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "two");
        let (a, b) = ("A".repeat(20_000), "B".repeat(30_000));
        std::thread::scope(|s| {
            for t in [&a, &b] {
                let f = &f;
                s.spawn(move || {
                    for _ in 0..50 {
                        let _ = atomic_write(f, t);
                    }
                });
            }
        });
        let end = fs::read_to_string(&f).unwrap();
        assert!(end == a || end == b);
        assert_eq!(d.names(), ["a.json"]);
    }

    #[test]
    fn write_keeps_the_previous_file_as_bak() {
        let d = Dir::new();
        let f = d.0.join("s.json");
        write_json(&f, &S { x: 1 }, 1).unwrap();
        let first = fs::read_to_string(&f).unwrap();
        write_json(&f, &S { x: 2 }, 1).unwrap();
        assert_eq!(read_json::<S>(&f, 1).unwrap(), Some(S { x: 2 }));
        assert_eq!(fs::read_to_string(backup_path(&f)).unwrap(), first);
        assert_eq!(d.names(), ["s.json", "s.json.bak"]);
    }

    #[test]
    fn a_corrupt_file_is_moved_aside_never_backed_up() {
        let d = Dir::new();
        let f = d.0.join("s.json");
        fs::write(&f, "{broken").unwrap();
        write_json(&f, &S { x: 3 }, 1).unwrap();
        assert_eq!(read_json::<S>(&f, 1).unwrap(), Some(S { x: 3 }));
        let names = d.names();
        assert_eq!(names.len(), 2, "{names:?}");
        assert!(names[1].starts_with("s.json.corrupt-"), "{names:?}");
        assert_eq!(fs::read_to_string(d.0.join(&names[1])).unwrap(), "{broken");
        // the wrong shape is unreadable too
        fs::write(&f, r#"{"x":"text"}"#).unwrap();
        write_json(&f, &S { x: 4 }, 1).unwrap();
        assert!(!backup_path(&f).exists());
        assert_eq!(d.names().iter().filter(|n| n.contains(".corrupt-")).count(), 2);
    }

    #[test]
    fn reads_fall_back_to_the_bak_and_never_move_files() {
        let d = Dir::new();
        let f = d.0.join("s.json");
        assert_eq!(read_json::<S>(&f, 1).unwrap(), None);
        fs::write(backup_path(&f), r#"{"x":5}"#).unwrap();
        // a deleted file stays deleted
        assert_eq!(read_json::<S>(&f, 1).unwrap(), None);
        fs::write(&f, "{broken").unwrap();
        assert_eq!(read_json::<S>(&f, 1).unwrap(), Some(S { x: 5 }));
        assert_eq!(fs::read_to_string(&f).unwrap(), "{broken");
        fs::remove_file(backup_path(&f)).unwrap();
        assert!(matches!(read_json::<S>(&f, 1), Err(StateError::Corrupt { .. })));
        fs::write(&f, "").unwrap();
        assert!(matches!(read_json::<S>(&f, 1), Err(StateError::Corrupt { .. })));
        assert_eq!(read_for_write::<S>(&f, 1).unwrap(), S::default());
        assert_eq!(read_or_default::<S>(&f, 1), S::default());
    }

    #[test]
    fn files_without_a_version_are_version_1() {
        let d = Dir::new();
        let f = d.0.join("l.json");
        fs::write(&f, r#"["a"]"#).unwrap();
        assert_eq!(read_json::<Vec<String>>(&f, 1).unwrap(), Some(vec!["a".to_string()]));
        // a format-1 list stays a bare list, so a build from before versions still reads it
        write_json(&f, &vec!["a".to_string(), "b".into()], 1).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v, json!(["a", "b"]));
        assert!(serde_json::from_str::<Vec<String>>(&fs::read_to_string(&f).unwrap()).is_ok());
        assert_eq!(read_json::<Vec<String>>(&f, 1).unwrap().unwrap(), ["a", "b"]);
        // the bare list was readable: it's the .bak
        assert_eq!(fs::read_to_string(backup_path(&f)).unwrap(), r#"["a"]"#);
        // a later list format goes in {version, items}, and reads back
        write_json(&f, &vec!["c".to_string()], 2).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&f).unwrap()).unwrap();
        assert_eq!(v, json!({"version": 2, "items": ["c"]}));
        assert_eq!(read_json::<Vec<String>>(&f, 2).unwrap().unwrap(), ["c"]);
        assert!(matches!(read_json::<Vec<String>>(&f, 1), Err(StateError::TooNew { found: 2, .. })));
        let o = d.0.join("o.json");
        fs::write(&o, r#"{"x":1}"#).unwrap();
        assert_eq!(read_json::<S>(&o, 1).unwrap(), Some(S { x: 1 }));
        write_json(&o, &S { x: 2 }, 1).unwrap();
        let v: Value = serde_json::from_str(&fs::read_to_string(&o).unwrap()).unwrap();
        assert_eq!(v, json!({"version": 1, "x": 2}));
    }

    #[test]
    fn a_newer_file_is_never_written() {
        let d = Dir::new();
        let f = d.0.join("l.json");
        let newer = r#"{"version":99,"items":[]}"#;
        fs::write(&f, newer).unwrap();
        assert_eq!(read_json::<Vec<String>>(&f, 1), Err(StateError::TooNew { path: shown(&f), found: 99, newest: 1 }));
        assert!(read_for_write::<Vec<String>>(&f, 1).is_err());
        let e = write_json(&f, &vec!["a".to_string()], 1).unwrap_err();
        assert!(e.contains("v99") && e.contains("최신 Maestro"), "{e}");
        assert_eq!(fs::read_to_string(&f).unwrap(), newer);
        assert_eq!(d.names(), ["l.json"]);
        // a newer file isn't covered by the .bak either
        fs::write(backup_path(&f), r#"["old"]"#).unwrap();
        assert!(matches!(read_json::<Vec<String>>(&f, 1), Err(StateError::TooNew { .. })));
        // the canvas: its own "x.y.z"
        let c = d.0.join("c.json");
        fs::write(&c, r#"{"version":"2.0.0"}"#).unwrap();
        assert!(matches!(read_text(&c, 1), Err(StateError::TooNew { found: 2, .. })));
        assert!(replace_text(&c, r#"{"version":"1.0.0"}"#, 1).is_err());
        fs::write(&c, r#"{"version":"1.0.0","a":1}"#).unwrap();
        assert_eq!(read_text(&c, 1).unwrap().unwrap(), r#"{"version":"1.0.0","a":1}"#);
        replace_text(&c, r#"{"version":"1.0.0","a":2}"#, 1).unwrap();
        assert_eq!(fs::read_to_string(backup_path(&c)).unwrap(), r#"{"version":"1.0.0","a":1}"#);
    }

    #[test]
    fn version_of_reads_numbers_and_semver() {
        assert_eq!(version_of(&json!({"version": "1.0.0"})), 1);
        assert_eq!(version_of(&json!({"version": "2.0.0"})), 2);
        assert_eq!(version_of(&json!({"version": 3})), 3);
        assert_eq!(version_of(&json!(["version"])), 1);
        assert_eq!(version_of(&json!({"x": 1})), 1);
    }
}
