use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashEntry {
    pub level: String,
    pub message: String,
}

pub(crate) struct FlashState {
    pub(crate) incoming: Vec<FlashEntry>,
    pub(crate) outgoing: Mutex<Vec<FlashEntry>>,
    pub(crate) read: AtomicBool,
}

impl FlashState {
    pub(crate) fn new(incoming: Vec<FlashEntry>) -> Self {
        Self {
            incoming,
            outgoing: Mutex::new(Vec::new()),
            read: AtomicBool::new(false),
        }
    }

    pub(crate) fn push(&self, level: &str, message: &str) {
        let mut outgoing = self.outgoing.lock().expect("flash mutex poisoned");
        outgoing.push(FlashEntry {
            level: level.to_string(),
            message: message.to_string(),
        });
    }

    pub(crate) fn drain_outgoing(&self) -> Vec<FlashEntry> {
        let mut outgoing = self.outgoing.lock().expect("flash mutex poisoned");
        std::mem::take(&mut *outgoing)
    }

    pub(crate) fn was_read(&self) -> bool {
        self.read.load(Ordering::Acquire)
    }

    pub(crate) fn mark_read(&self) {
        self.read.store(true, Ordering::Release);
    }

    #[cfg_attr(not(feature = "templates"), allow(dead_code))]
    pub(crate) fn incoming_as_template_value(&self) -> Vec<BTreeMap<String, String>> {
        self.incoming
            .iter()
            .map(|entry| {
                let mut map = BTreeMap::new();
                map.insert(entry.level.clone(), entry.message.clone());
                map
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_with_empty_incoming() {
        let state = FlashState::new(vec![]);
        assert!(state.incoming.is_empty());
        assert!(!state.was_read());
    }

    #[test]
    fn new_with_incoming_entries() {
        let entries = vec![
            FlashEntry {
                level: "success".into(),
                message: "Done".into(),
            },
            FlashEntry {
                level: "error".into(),
                message: "Oops".into(),
            },
        ];
        let state = FlashState::new(entries.clone());
        assert_eq!(state.incoming, entries);
    }

    #[test]
    fn push_adds_to_outgoing() {
        let state = FlashState::new(vec![]);
        state.push("info", "hello");
        state.push("error", "fail");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 2);
        assert_eq!(
            outgoing[0],
            FlashEntry {
                level: "info".into(),
                message: "hello".into()
            }
        );
        assert_eq!(
            outgoing[1],
            FlashEntry {
                level: "error".into(),
                message: "fail".into()
            }
        );
    }

    #[test]
    fn drain_outgoing_clears_vec() {
        let state = FlashState::new(vec![]);
        state.push("info", "msg");
        let first = state.drain_outgoing();
        assert_eq!(first.len(), 1);
        let second = state.drain_outgoing();
        assert!(second.is_empty());
    }

    #[test]
    fn read_flag_default_false() {
        let state = FlashState::new(vec![]);
        assert!(!state.was_read());
    }

    #[test]
    fn mark_read_sets_flag() {
        let state = FlashState::new(vec![]);
        state.mark_read();
        assert!(state.was_read());
    }

    #[test]
    fn multiple_same_level_preserved_in_order() {
        let state = FlashState::new(vec![]);
        state.push("error", "first");
        state.push("error", "second");
        state.push("info", "third");
        let outgoing = state.drain_outgoing();
        assert_eq!(outgoing.len(), 3);
        assert_eq!(outgoing[0].level, "error");
        assert_eq!(outgoing[0].message, "first");
        assert_eq!(outgoing[1].level, "error");
        assert_eq!(outgoing[1].message, "second");
        assert_eq!(outgoing[2].level, "info");
    }

    #[test]
    fn incoming_as_template_value_formats_correctly() {
        let entries = vec![
            FlashEntry {
                level: "error".into(),
                message: "bad".into(),
            },
            FlashEntry {
                level: "info".into(),
                message: "ok".into(),
            },
        ];
        let state = FlashState::new(entries);
        let result = state.incoming_as_template_value();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].get("error").unwrap(), "bad");
        assert_eq!(result[1].get("info").unwrap(), "ok");
    }

    #[test]
    fn flash_entry_serialization_roundtrip() {
        let entry = FlashEntry {
            level: "success".into(),
            message: "Item saved".into(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        let parsed: FlashEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, parsed);
    }

    #[test]
    fn flash_entry_vec_serialization() {
        let entries = vec![
            FlashEntry {
                level: "error".into(),
                message: "fail".into(),
            },
            FlashEntry {
                level: "success".into(),
                message: "ok".into(),
            },
        ];
        let json = serde_json::to_string(&entries).unwrap();
        let parsed: Vec<FlashEntry> = serde_json::from_str(&json).unwrap();
        assert_eq!(entries, parsed);
    }
}
