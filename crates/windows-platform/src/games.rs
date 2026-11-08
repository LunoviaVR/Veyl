//! Which programs are games. A game is a program that shows a window and is either:
//! 1. an executable Windows itself has recognised as a game (the Game Bar list in
//!    `HKCU\System\GameConfigStore\Children`; "Remember this is a game" adds to it), or
//! 2. run from a game store's library: Steam `steamapps\common` (minus the apps Steam itself
//!    lists as software, tools or configs, such as Blender or Wallpaper Engine), Epic games
//!    (from Epic's install records), and the standard GOG, EA, Ubisoft and Xbox game folders.
//!
//! The window requirement leaves out crash handlers, helpers and headless servers.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::registry::{read_string, subkeys, HKEY_CURRENT_USER};

const CHILDREN: &str = r"System\GameConfigStore\Children";
const STEAM_LIBRARY: &str = r"\steamapps\common\";

/// Library folders whose programs are games. Launchers live outside these folders
/// (Epic and Riot keep their launchers beside the games, so they come from records instead).
const LIBRARY_MARKERS: &[&str] = &[
    STEAM_LIBRARY,
    r"\gog galaxy\games\",
    r"\ea games\",
    r"\ubisoft game launcher\games\",
    r"\xboxgames\",
];

#[derive(Debug, Default)]
pub struct Games {
    /// Lowercased executable paths Windows lists as games.
    listed: HashSet<String>,
    /// Lowercased install folders (ending in `\`) of games from store records.
    roots: Vec<String>,
    /// Lowercased `steamapps\common` folder names of Steam apps that are not games.
    steam_software: HashSet<String>,
}

impl Games {
    pub fn load() -> Self {
        Self {
            listed: windows_list(),
            roots: epic_game_roots(),
            steam_software: steam_software_dirs(),
        }
    }

    /// The list shared by every caller, reloaded at most once a minute: Windows and the
    /// stores add games as they are installed or first run, and reading them takes a while.
    pub fn current() -> Arc<Games> {
        static CACHE: Mutex<Option<(Instant, Arc<Games>)>> = Mutex::new(None);
        let mut c = CACHE.lock().unwrap_or_else(|p| p.into_inner());
        match &*c {
            Some((at, g)) if at.elapsed() < Duration::from_secs(60) => g.clone(),
            _ => {
                let g = Arc::new(Games::load());
                *c = Some((Instant::now(), g.clone()));
                g
            }
        }
    }

    /// `has_window`: the process shows a window the user can switch to.
    pub fn is_game(&self, exe_path: &str, has_window: bool) -> bool {
        let p = exe_path.to_lowercase();
        let steam_dir = p
            .split_once(STEAM_LIBRARY)
            .map(|(_, rest)| rest.split('\\').next().unwrap_or(rest));
        has_window
            && (self.listed.contains(&p)
                || (LIBRARY_MARKERS.iter().any(|m| p.contains(m))
                    && !steam_dir.is_some_and(|d| self.steam_software.contains(d)))
                || self.roots.iter().any(|r| p.starts_with(r)))
    }
}

fn windows_list() -> HashSet<String> {
    subkeys(HKEY_CURRENT_USER, CHILDREN)
        .iter()
        .filter_map(|k| {
            read_string(
                HKEY_CURRENT_USER,
                &format!(r"{CHILDREN}\{k}"),
                "MatchedExeFullPath",
            )
        })
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Install folder of an Epic manifest (`*.item`, JSON) when it is a game, not an engine or plugin.
fn epic_game_root(manifest: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(manifest).ok()?;
    let is_game = v["AppCategories"]
        .as_array()?
        .iter()
        .any(|c| c.as_str() == Some("games"));
    let dir = v["InstallLocation"]
        .as_str()?
        .trim()
        .trim_end_matches(['\\', '/']);
    (is_game && !dir.is_empty()).then(|| format!(r"{}\", dir.to_lowercase()))
}

fn epic_game_roots() -> Vec<String> {
    let base = std::env::var_os("ProgramData").unwrap_or_else(|| r"C:\ProgramData".into());
    let dir = Path::new(&base).join(r"Epic\EpicGamesLauncher\Data\Manifests");
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("item"))
        })
        .filter_map(|e| std::fs::read_to_string(e.path()).ok())
        .filter_map(|m| epic_game_root(&m))
        .collect()
}

fn steam_software_dirs() -> HashSet<String> {
    read_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
        .and_then(|s| std::fs::read(Path::new(&s).join(r"appcache\appinfo.vdf")).ok())
        .and_then(|b| software_dirs(&b))
        .unwrap_or_default()
}

/// Bounds-checked little-endian reads; every read past the end is `None`.
struct Reader<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn bytes(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.pos..self.pos.checked_add(n)?)?;
        self.pos += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.bytes(4)?.try_into().ok()?))
    }
    fn u8(&mut self) -> Option<u8> {
        Some(self.bytes(1)?[0])
    }
    fn cstr(&mut self) -> Option<&'a str> {
        let rest = self.b.get(self.pos..)?;
        let len = rest.iter().position(|&c| c == 0)?;
        self.pos += len + 1;
        Some(std::str::from_utf8(&rest[..len]).unwrap_or(""))
    }
}

/// Folders of non-game apps in Steam's app cache (`appcache\appinfo.vdf`, formats 28 and 29):
/// each app's `common/type` ("Game", "Demo", "Beta" are games; "Application", "Tool",
/// "Config", "Music"... are not) and `config/installdir`. `None` for an unknown format.
fn software_dirs(b: &[u8]) -> Option<HashSet<String>> {
    let mut r = Reader { b, pos: 0 };
    let magic = r.u32()?;
    let _universe = r.u32()?;
    // Format 29 stores key names once, in a table at the end; format 28 inlines them.
    let names: Option<Vec<&str>> = match magic {
        0x0756_4429 => {
            let at = usize::try_from(u64::from_le_bytes(r.bytes(8)?.try_into().ok()?)).ok()?;
            let mut t = Reader { b, pos: at };
            let n = t.u32()?;
            Some((0..n).map(|_| t.cstr()).collect::<Option<_>>()?)
        }
        0x0756_4428 => None,
        _ => return None,
    };
    let mut out = HashSet::new();
    loop {
        let app = r.u32()?;
        if app == 0 {
            return Some(out);
        }
        let size = r.u32()? as usize;
        let entry = r.bytes(size)?;
        // infoState, lastUpdated, picsToken, SHA-1, changeNumber, binary SHA-1: 60 bytes.
        let Some(kv) = entry.get(60..) else { continue };
        if let Some((kind, dir)) = type_and_dir(kv, names.as_deref()) {
            if !["game", "demo", "beta"].contains(&kind.to_ascii_lowercase().as_str()) {
                out.insert(dir.to_lowercase());
            }
        }
    }
}

/// `appinfo/common/type` and `appinfo/config/installdir` from one app's binary key-values.
fn type_and_dir<'a>(kv: &'a [u8], names: Option<&[&'a str]>) -> Option<(&'a str, &'a str)> {
    let mut r = Reader { b: kv, pos: 0 };
    let mut path: Vec<&str> = Vec::new();
    let (mut kind, mut dir) = (None, None);
    while kind.is_none() || dir.is_none() {
        let t = r.u8()?;
        if t == 8 || t == 11 {
            path.pop()?; // end of the outermost map without both values
            continue;
        }
        let key = match names {
            Some(n) => *n.get(r.u32()? as usize)?,
            None => r.cstr()?,
        };
        match t {
            0 => path.push(key),
            1 => {
                let v = r.cstr()?;
                match (path.last(), key) {
                    (Some(&"common"), "type") => kind = Some(v),
                    (Some(&"config"), "installdir") => dir = Some(v),
                    _ => {}
                }
            }
            2 | 3 | 4 | 6 => {
                r.bytes(4)?;
            }
            7 | 10 => {
                r.bytes(8)?;
            }
            5 => {
                // UTF-16 string ending in a 16-bit zero.
                while r.bytes(2)? != [0, 0] {}
            }
            _ => return None,
        }
    }
    Some((kind?, dir?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn games() -> Games {
        Games {
            listed: [r"e:\steam\common\elden ring\game\eldenring.exe".to_owned()].into(),
            roots: vec![r"e:\epiclibrary\wonderlands\".to_owned()],
            steam_software: ["blender".to_owned()].into(),
        }
    }

    #[test]
    fn detects_listed_library_and_store_games() {
        let g = games();
        assert!(
            g.is_game(r"E:\Steam\common\ELDEN RING\Game\eldenring.exe", true),
            "Windows list, case-insensitive"
        );
        assert!(
            g.is_game(r"E:\SteamLibrary\steamapps\common\CRACK\CRACK.exe", true),
            "Steam library"
        );
        assert!(
            g.is_game(
                r"E:\EpicLibrary\Wonderlands\OakGame\Binaries\Win64\Wonderlands.exe",
                true
            ),
            "Epic record"
        );
        assert!(g.is_game(
            r"C:\Program Files\EA Games\Battlefield 2042\BF2042.exe",
            true
        ));
    }

    #[test]
    fn ignores_helpers_launchers_software_and_other_apps() {
        let g = games();
        assert!(
            !g.is_game(
                r"E:\SteamLibrary\steamapps\common\CRACK\UnityCrashHandler64.exe",
                false
            ),
            "no window"
        );
        assert!(
            !g.is_game(r"E:\Steam\common\ELDEN RING\Game\eldenring.exe", false),
            "listed, but running without a window (a server or helper)"
        );
        assert!(
            !g.is_game(
                r"E:\SteamLibrary\steamapps\common\Blender\blender.exe",
                true
            ),
            "Steam lists Blender as an application"
        );
        assert!(
            !g.is_game(r"C:\Program Files (x86)\Steam\steam.exe", true),
            "the Steam client"
        );
        assert!(!g.is_game(
            r"C:\Program Files\Epic Games\Launcher\Portal\Binaries\Win64\EpicGamesLauncher.exe",
            true
        ));
        assert!(!g.is_game(
            r"C:\Program Files\Blender Foundation\Blender\blender.exe",
            true
        ));
    }

    #[test]
    fn epic_records_count_only_games() {
        let game = r#"{"DisplayName":"Wonderlands","AppCategories":["public","games","applications"],"InstallLocation":"E:\\EpicLibrary\\Wonderlands"}"#;
        let engine = r#"{"DisplayName":"Unreal Engine","AppCategories":["engines/ue5","engines"],"InstallLocation":"D:\\Unreal\\UE_5.8"}"#;
        assert_eq!(
            epic_game_root(game).as_deref(),
            Some(r"e:\epiclibrary\wonderlands\")
        );
        assert_eq!(epic_game_root(engine), None);
        assert_eq!(epic_game_root("not json"), None);
    }

    /// A format-29 app cache with the given (appid, type, installdir) entries.
    fn appinfo(apps: &[(u32, &str, &str)]) -> Vec<u8> {
        let names = [
            "appinfo",
            "common",
            "type",
            "config",
            "installdir",
            "gameid",
        ];
        let mut body = Vec::new();
        for (id, kind, dir) in apps {
            let mut kv = vec![0u8; 60];
            let key = |kv: &mut Vec<u8>, t: u8, k: usize| {
                kv.push(t);
                kv.extend((k as u32).to_le_bytes());
            };
            let s = |kv: &mut Vec<u8>, v: &str| {
                kv.extend(v.as_bytes());
                kv.push(0);
            };
            key(&mut kv, 0, 0); // appinfo {
            key(&mut kv, 0, 1); //   common {
            key(&mut kv, 2, 5); //     gameid = int
            kv.extend(id.to_le_bytes());
            key(&mut kv, 1, 2); //     type = "…"
            s(&mut kv, kind);
            kv.push(8); //   }
            key(&mut kv, 0, 3); //   config {
            key(&mut kv, 1, 4); //     installdir = "…"
            s(&mut kv, dir);
            kv.extend([8, 8, 8]); // } } end
            body.extend(id.to_le_bytes());
            body.extend((kv.len() as u32).to_le_bytes());
            body.extend(kv);
        }
        body.extend(0u32.to_le_bytes());
        let mut b = Vec::new();
        b.extend(0x0756_4429u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(((16 + body.len()) as u64).to_le_bytes());
        b.extend(body);
        b.extend((names.len() as u32).to_le_bytes());
        for n in names {
            b.extend(n.as_bytes());
            b.push(0);
        }
        b
    }

    #[test]
    fn steam_app_cache_marks_software_folders() {
        let b = appinfo(&[
            (365670, "Application", "Blender"),
            (1, "Game", "CRACK"),
            (2, "game", "Old"),
            (3, "Demo", "Some Demo"),
            (228980, "Tool", "Steamworks Shared"),
            (250820, "Config", "SteamVR"),
        ]);
        let dirs = software_dirs(&b).expect("parses");
        let want: HashSet<String> = ["blender", "steamworks shared", "steamvr"]
            .map(String::from)
            .into();
        assert_eq!(dirs, want);
        assert_eq!(software_dirs(&b[..b.len() / 2]), None, "cut short");
        assert_eq!(software_dirs(b"not a cache"), None);
    }

    #[test]
    fn reads_the_windows_game_list_and_steam_cache() {
        // Every path in the list is a full .exe path; an empty list is valid on a fresh PC.
        for p in windows_list() {
            assert!(p.ends_with(".exe") && p.contains(':'), "{p}");
        }
        let _ = steam_software_dirs(); // never panics, with or without Steam
    }
}
