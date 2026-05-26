use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MonitorRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookmarkRecord {
    pub slot: u8,
    pub x: i32,
    pub y: i32,
    pub monitor_device_name: String,
    pub monitor_rect: MonitorRect,
    pub virtual_desktop_id: Option<String>,
    pub anchor_hwnd: Option<isize>,
    pub anchor_process_id: Option<u32>,
    pub anchor_window_title: Option<String>,
    pub created_at_unix_ms: u64,
    pub updated_at_unix_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetOutcome {
    Saved,
    Overwritten,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoveOutcome {
    Removed,
    NotFound,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadWarning {
    pub message: String,
    pub quarantine_path: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BookmarkFile {
    slot_count: u8,
    slots: BTreeMap<u8, BookmarkRecord>,
}

#[derive(Debug, Clone)]
pub struct BookmarkStore {
    slot_count: u8,
    slots: BTreeMap<u8, BookmarkRecord>,
}

impl BookmarkStore {
    pub fn new(slot_count: u8) -> Self {
        Self {
            slot_count: slot_count.max(1),
            slots: BTreeMap::new(),
        }
    }

    #[allow(dead_code)]
    pub fn slot_count(&self) -> u8 {
        self.slot_count
    }

    pub fn get_slot(&self, slot: u8) -> Option<&BookmarkRecord> {
        if !self.is_valid_slot(slot) {
            return None;
        }
        self.slots.get(&slot)
    }

    pub fn set_slot(&mut self, slot: u8, mut record: BookmarkRecord) -> SetOutcome {
        assert!(self.is_valid_slot(slot), "slot out of range: {slot}");
        record.slot = slot;
        let existed = self.slots.insert(slot, record).is_some();
        if existed {
            SetOutcome::Overwritten
        } else {
            SetOutcome::Saved
        }
    }

    pub fn remove_slot(&mut self, slot: u8) -> RemoveOutcome {
        if !self.is_valid_slot(slot) {
            return RemoveOutcome::NotFound;
        }
        if self.slots.remove(&slot).is_some() {
            RemoveOutcome::Removed
        } else {
            RemoveOutcome::NotFound
        }
    }

    pub fn clear_all(&mut self) {
        self.slots.clear();
    }

    #[allow(dead_code)]
    pub fn occupied_slots(&self) -> Vec<u8> {
        self.slots.keys().copied().collect()
    }

    pub fn load(path: &Path, slot_count: u8) -> Result<(Self, Option<LoadWarning>), io::Error> {
        if !path.exists() {
            return Ok((Self::new(slot_count), None));
        }

        let raw = fs::read_to_string(path)?;
        match serde_json::from_str::<BookmarkFile>(&raw) {
            Ok(file) => {
                let mut store = Self::new(slot_count.max(file.slot_count));
                for (slot, mut record) in file.slots {
                    if store.is_valid_slot(slot) {
                        record.slot = slot;
                        store.slots.insert(slot, record);
                    }
                }
                Ok((store, None))
            }
            Err(err) => {
                let quarantine = quarantine_corrupt_file(path)?;
                Ok((
                    Self::new(slot_count),
                    Some(LoadWarning {
                        message: format!("Failed to deserialize bookmarks JSON: {err}"),
                        quarantine_path: Some(quarantine),
                    }),
                ))
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), io::Error> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let payload = BookmarkFile {
            slot_count: self.slot_count,
            slots: self.slots.clone(),
        };
        let json = serde_json::to_vec_pretty(&payload)?;
        let tmp = temp_path_for(path);

        {
            let mut file = File::create(&tmp)?;
            file.write_all(&json)?;
            file.flush()?;
            file.sync_all()?;
        }

        fs::rename(tmp, path)?;
        Ok(())
    }

    fn is_valid_slot(&self, slot: u8) -> bool {
        (1..=self.slot_count).contains(&slot)
    }
}

pub fn resolve_bookmarks_path(config_path: &Path, bookmark_path: &Path) -> PathBuf {
    if bookmark_path.is_absolute() {
        return bookmark_path.to_path_buf();
    }

    let base_dir = if config_path.is_dir() {
        config_path.to_path_buf()
    } else {
        config_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    };

    base_dir.join(bookmark_path)
}

fn temp_path_for(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".tmp");
    PathBuf::from(os)
}

fn quarantine_corrupt_file(path: &Path) -> Result<PathBuf, io::Error> {
    let timestamp = now_unix_ms();
    let mut os = path.as_os_str().to_os_string();
    os.push(format!(".corrupt.{timestamp}"));
    let quarantine = PathBuf::from(os);
    fs::rename(path, &quarantine)?;
    Ok(quarantine)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_record(slot: u8) -> BookmarkRecord {
        BookmarkRecord {
            slot,
            x: 10,
            y: 20,
            monitor_device_name: "DISPLAY1".to_string(),
            monitor_rect: MonitorRect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            virtual_desktop_id: Some("desktop-1".to_string()),
            anchor_hwnd: Some(123),
            anchor_process_id: Some(456),
            anchor_window_title: Some("Title".to_string()),
            created_at_unix_ms: 1,
            updated_at_unix_ms: 2,
        }
    }

    fn test_path(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "multi_mousemover_bookmark_test_{name}_{}",
            now_unix_ms()
        ));
        p
    }

    #[test]
    fn missing_file_loads_empty() {
        let path = test_path("missing");
        let (store, warning) = BookmarkStore::load(&path, 9).unwrap();
        assert_eq!(store.occupied_slots(), Vec::<u8>::new());
        assert!(warning.is_none());
    }

    #[test]
    fn save_load_roundtrip() {
        let path = test_path("roundtrip");
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, fixture_record(1));
        store.save(&path).unwrap();

        let (loaded, warning) = BookmarkStore::load(&path, 9).unwrap();
        assert!(warning.is_none());
        assert_eq!(loaded.get_slot(1), Some(&fixture_record(1)));
    }

    #[test]
    fn overwrite_same_slot_updates_existing_slot_not_duplicate() {
        let mut store = BookmarkStore::new(9);
        assert_eq!(store.set_slot(2, fixture_record(2)), SetOutcome::Saved);
        let mut replacement = fixture_record(2);
        replacement.x = 111;
        assert_eq!(
            store.set_slot(2, replacement.clone()),
            SetOutcome::Overwritten
        );
        assert_eq!(store.occupied_slots(), vec![2]);
        assert_eq!(store.get_slot(2), Some(&replacement));
    }

    #[test]
    fn clear_existing_slot_removes_only_target_slot() {
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, fixture_record(1));
        store.set_slot(2, fixture_record(2));

        assert_eq!(store.remove_slot(1), RemoveOutcome::Removed);
        assert_eq!(store.get_slot(1), None);
        assert!(store.get_slot(2).is_some());
    }

    #[test]
    fn clearing_empty_slot_yields_not_found() {
        let mut store = BookmarkStore::new(9);
        assert_eq!(store.remove_slot(3), RemoveOutcome::NotFound);
    }

    #[test]
    fn save_after_clear_removes_slot_from_persisted_json() {
        let path = test_path("clear_persist");
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, fixture_record(1));
        store.set_slot(2, fixture_record(2));
        store.save(&path).unwrap();

        assert_eq!(store.remove_slot(1), RemoveOutcome::Removed);
        store.save(&path).unwrap();

        let raw = fs::read_to_string(path).unwrap();
        let file: BookmarkFile = serde_json::from_str(&raw).unwrap();
        assert!(!file.slots.contains_key(&1));
        assert!(file.slots.contains_key(&2));
    }

    #[test]
    fn clear_all_empties_store() {
        let mut store = BookmarkStore::new(9);
        store.set_slot(1, fixture_record(1));
        store.set_slot(2, fixture_record(2));
        store.clear_all();
        assert!(store.occupied_slots().is_empty());
    }

    #[test]
    fn corrupt_json_quarantined_and_no_panic() {
        let path = test_path("corrupt");
        fs::write(&path, "not-json").unwrap();

        let (store, warning) = BookmarkStore::load(&path, 9).unwrap();
        assert!(store.occupied_slots().is_empty());
        let warning = warning.expect("expected warning");
        let quarantine = warning.quarantine_path.expect("expected quarantine path");
        assert!(quarantine.exists());
        assert!(!path.exists());
    }

    #[test]
    fn relative_path_resolved_next_to_config_path() {
        let config = PathBuf::from("/tmp/my-app/config.toml");
        let resolved = resolve_bookmarks_path(&config, Path::new("data/bookmarks.json"));
        assert_eq!(resolved, PathBuf::from("/tmp/my-app/data/bookmarks.json"));
    }

    #[test]
    fn absolute_path_unchanged() {
        let absolute = PathBuf::from("/var/tmp/bookmarks.json");
        let resolved = resolve_bookmarks_path(Path::new("config.toml"), &absolute);
        assert_eq!(resolved, absolute);
    }

    #[test]
    fn atomic_save_smoke_test() {
        let path = test_path("atomic");
        let mut store = BookmarkStore::new(9);
        store.set_slot(3, fixture_record(3));
        store.save(&path).unwrap();

        assert!(path.exists());
        let raw = fs::read_to_string(path).unwrap();
        let file: BookmarkFile = serde_json::from_str(&raw).unwrap();
        assert_eq!(file.slots.get(&3).map(|r| r.x), Some(10));
    }

    #[test]
    fn bookmark_record_serializes_optional_anchor_fields() {
        let mut record = fixture_record(3);
        record.virtual_desktop_id = None;
        record.anchor_hwnd = None;
        record.anchor_process_id = None;
        record.anchor_window_title = None;

        let json = serde_json::to_string(&record).unwrap();
        let round_trip: BookmarkRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(round_trip.virtual_desktop_id, None);
        assert_eq!(round_trip.anchor_hwnd, None);
        assert_eq!(round_trip.anchor_process_id, None);
        assert_eq!(round_trip.anchor_window_title, None);
    }
}
