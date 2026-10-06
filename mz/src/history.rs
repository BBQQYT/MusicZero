use crate::plugin::Track;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const LIMIT: usize = 5;

#[derive(Default, Deserialize, Serialize)]
struct SavedHistory {
    tracks: Vec<Track>,
}

pub struct History {
    path: PathBuf,
    tracks: Vec<Track>,
}

impl History {
    pub fn load(module: &str) -> Self {
        Self::from_path(mcz::paths::config_dir("mz").join(format!("{module}.history.json")))
    }

    fn from_path(path: PathBuf) -> Self {
        let mut saved: SavedHistory = if path.exists() {
            mz_module_support::read_json(&path).unwrap_or_else(|error| {
                log::warn!("История треков: {error}");
                SavedHistory::default()
            })
        } else {
            SavedHistory::default()
        };
        let mut seen = std::collections::HashSet::new();
        saved
            .tracks
            .retain(|track| valid(track) && seen.insert(track.id.clone()));
        saved.tracks.truncate(LIMIT);
        for track in &mut saved.tracks {
            track.feedback.clear();
        }
        Self {
            path,
            tracks: saved.tracks,
        }
    }

    pub fn entries(&self, current_id: Option<&str>) -> Vec<Track> {
        self.tracks
            .iter()
            .filter(|track| Some(track.id.as_str()) != current_id)
            .cloned()
            .collect()
    }

    pub fn remember(&mut self, track: &Track) {
        if !valid(track) {
            return;
        }
        let mut track = track.clone();
        track.feedback.clear();
        self.tracks.retain(|entry| entry.id != track.id);
        self.tracks.insert(0, track);
        self.tracks.truncate(LIMIT);
        if let Err(error) = mz_module_support::save(
            &self.path,
            &SavedHistory {
                tracks: self.tracks.clone(),
            },
        ) {
            log::warn!("Не удалось сохранить историю треков: {error}");
        }
    }
}

fn valid(track: &Track) -> bool {
    !track.stream
        && !track.id.is_empty()
        && track.id.len() <= 512
        && !track.id.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn track(id: &str) -> Track {
        serde_json::from_value(json!({"id":id,"title":id,"artist":"Artist","feedback":"old-batch"}))
            .unwrap()
    }

    #[test]
    fn retains_five_distinct_tracks_newest_first_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ymz.history.json");
        let mut history = History::from_path(path.clone());
        for id in ["1", "2", "3", "4", "5", "6", "4"] {
            history.remember(&track(id));
        }
        let loaded = History::from_path(path.clone());
        let entries = loaded.entries(None);
        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.id.as_str())
                .collect::<Vec<_>>(),
            ["4", "6", "5", "3", "2"]
        );
        assert_eq!(loaded.entries(Some("4")).len(), 4);
        assert!(entries.iter().all(|entry| entry.feedback.is_empty()));
        assert!(!std::fs::read_to_string(path).unwrap().contains("old-batch"));
        assert!(History::from_path(dir.path().join("youmz.history.json"))
            .entries(None)
            .is_empty());
    }

    #[test]
    fn live_and_invalid_tracks_are_ignored_and_corrupt_history_recovers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(&path, "broken JSON").unwrap();
        let mut history = History::from_path(path.clone());
        let mut live = track("radio");
        live.stream = true;
        history.remember(&live);
        history.remember(&track(""));
        history.remember(&track("line\nbreak"));
        assert!(history.entries(None).is_empty());
        history.remember(&track("valid"));
        assert_eq!(History::from_path(path).entries(None)[0].id, "valid");
    }
}
