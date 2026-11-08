//! Which programs are games. Two sources:
//! 1. the executables Windows itself has recognised as games (the Game Bar list in
//!    `HKCU\System\GameConfigStore\Children`; "Remember this is a game" adds to it);
//! 2. programs with a window that run from a game store's library: any Steam
//!    `steamapps\common` folder, Epic games (from Epic's install records), and the
//!    standard GOG, EA, Ubisoft and Xbox game folders. Many newer games are only here.
//!
//! Callers reload periodically: Windows and the stores add games as they are installed or run.

use std::collections::HashSet;

use crate::registry::{read_string, subkeys, HKEY_CURRENT_USER};

const CHILDREN: &str = r"System\GameConfigStore\Children";

/// Library folders whose programs are games. Launchers live outside these folders
/// (Epic and Riot keep their launchers beside the games, so they come from records instead).
const LIBRARY_MARKERS: &[&str] = &[
    r"\steamapps\common\",
    r"\gog galaxy\games\",
    r"\ea games\",
    r"\ubisoft game launcher\games\",
    r"\xboxgames\",
];

#[derive(Debug)]
pub struct Games {
    /// Lowercased executable paths Windows lists as games.
    listed: HashSet<String>,
    /// Lowercased install folders (ending in `\`) of games from store records.
    roots: Vec<String>,
}

impl Games {
    pub fn load() -> Self {
        Self {
            listed: windows_list(),
            roots: epic_game_roots(),
        }
    }

    /// `has_window`: the process shows a window, which leaves out crash handlers
    /// and helpers that live in the same folder as the game.
    pub fn is_game(&self, exe_path: &str, has_window: bool) -> bool {
        let p = exe_path.to_lowercase();
        self.listed.contains(&p)
            || (has_window
                && (LIBRARY_MARKERS.iter().any(|m| p.contains(m))
                    || self.roots.iter().any(|r| p.starts_with(r))))
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
    let dir = std::path::Path::new(&base).join(r"Epic\EpicGamesLauncher\Data\Manifests");
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

#[cfg(test)]
mod tests {
    use super::*;

    fn games() -> Games {
        Games {
            listed: [r"e:\steam\common\elden ring\game\eldenring.exe".to_owned()].into(),
            roots: vec![r"e:\epiclibrary\wonderlands\".to_owned()],
        }
    }

    #[test]
    fn detects_listed_library_and_store_games() {
        let g = games();
        assert!(
            g.is_game(r"E:\Steam\common\ELDEN RING\Game\eldenring.exe", false),
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
    fn ignores_helpers_launchers_and_other_apps() {
        let g = games();
        assert!(
            !g.is_game(
                r"E:\SteamLibrary\steamapps\common\CRACK\UnityCrashHandler64.exe",
                false
            ),
            "no window"
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

    #[test]
    fn reads_the_windows_game_list() {
        // Every path in the list is a full .exe path; an empty list is valid on a fresh PC.
        for p in windows_list() {
            assert!(p.ends_with(".exe") && p.contains(':'), "{p}");
        }
    }
}
