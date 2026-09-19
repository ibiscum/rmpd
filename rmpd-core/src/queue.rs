use crate::song::Song;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct QueueItem {
    pub id: u32,
    pub position: u32,
    pub song: Arc<Song>,
    /// Priority (0-255, default 0). Higher values have higher priority.
    pub priority: u8,
    /// Optional playback range (start, end) in seconds
    pub range: Option<(f64, f64)>,
    /// Custom tags attached to this queue item
    pub tags: Option<HashMap<String, String>>,
}

impl Serialize for QueueItem {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("QueueItem", 6)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("position", &self.position)?;
        state.serialize_field("song", self.song.as_ref())?;
        state.serialize_field("priority", &self.priority)?;
        state.serialize_field("range", &self.range)?;
        state.serialize_field("tags", &self.tags)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for QueueItem {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, MapAccess, Visitor};
        use std::fmt;

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Id,
            Position,
            Song,
            Priority,
            Range,
            Tags,
        }

        struct QueueItemVisitor;

        impl<'de> Visitor<'de> for QueueItemVisitor {
            type Value = QueueItem;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct QueueItem")
            }

            fn visit_map<V>(self, mut map: V) -> Result<QueueItem, V::Error>
            where
                V: MapAccess<'de>,
            {
                let mut id = None;
                let mut position = None;
                let mut song = None;
                let mut priority = None;
                let mut range = None;
                let mut tags = None;

                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Id => {
                            if id.is_some() {
                                return Err(de::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value()?);
                        }
                        Field::Position => {
                            if position.is_some() {
                                return Err(de::Error::duplicate_field("position"));
                            }
                            position = Some(map.next_value()?);
                        }
                        Field::Song => {
                            if song.is_some() {
                                return Err(de::Error::duplicate_field("song"));
                            }
                            let s: Song = map.next_value()?;
                            song = Some(Arc::new(s));
                        }
                        Field::Priority => {
                            if priority.is_some() {
                                return Err(de::Error::duplicate_field("priority"));
                            }
                            priority = Some(map.next_value()?);
                        }
                        Field::Range => {
                            if range.is_some() {
                                return Err(de::Error::duplicate_field("range"));
                            }
                            range = Some(map.next_value()?);
                        }
                        Field::Tags => {
                            if tags.is_some() {
                                return Err(de::Error::duplicate_field("tags"));
                            }
                            tags = Some(map.next_value()?);
                        }
                    }
                }

                let id = id.ok_or_else(|| de::Error::missing_field("id"))?;
                let position = position.ok_or_else(|| de::Error::missing_field("position"))?;
                let song = song.ok_or_else(|| de::Error::missing_field("song"))?;
                let priority = priority.unwrap_or(0);
                let range = range;
                let tags = tags;

                Ok(QueueItem {
                    id,
                    position,
                    song,
                    priority,
                    range,
                    tags,
                })
            }
        }

        deserializer.deserialize_struct(
            "QueueItem",
            &["id", "position", "song", "priority", "range", "tags"],
            QueueItemVisitor,
        )
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Queue {
    items: Vec<QueueItem>,
    next_id: u32,
    version: u32,
    /// Name of the last stored playlist successfully loaded into this
    /// queue (mirrors MPD's `Queue::last_loaded_playlist`, `Queue.hxx`).
    /// Empty when none has been loaded since the last `clear`.
    #[serde(default)]
    last_loaded_playlist: String,
}

impl Queue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next queue-item id. Mirrors MPD's `IdTable`, whose
    /// counter starts at 1: id 0 is never a valid song id, and clients
    /// (libmpdclient, mpc) read a 0 `songid` as "no song". The zero guard
    /// also normalizes a queue restored from a state file written before
    /// this rule.
    fn allocate_id(&mut self) -> u32 {
        if self.next_id == 0 {
            self.next_id = 1;
        }
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn add(&mut self, song: Song) -> u32 {
        let id = self.allocate_id();

        let position = self.items.len() as u32;
        self.items.push(QueueItem {
            id,
            position,
            song: Arc::new(song),
            priority: 0, // Default priority
            range: None, // No range restriction by default
            tags: None,  // No custom tags by default
        });

        self.version += 1;
        id
    }

    pub fn delete(&mut self, position: u32) -> Option<QueueItem> {
        if (position as usize) < self.items.len() {
            let item = self.items.remove(position as usize);
            self.reindex();
            self.version += 1;
            Some(item)
        } else {
            None
        }
    }

    pub fn delete_id(&mut self, id: u32) -> Option<QueueItem> {
        if let Some(idx) = self.items.iter().position(|item| item.id == id) {
            let item = self.items.remove(idx);
            self.reindex();
            self.version += 1;
            Some(item)
        } else {
            None
        }
    }

    pub fn clear(&mut self) {
        self.items.clear();
        self.last_loaded_playlist.clear();
        self.version += 1;
    }

    /// Name of the last stored playlist loaded into this queue via `load`,
    /// or `""` if none has been loaded since the last `clear`. Mirrors
    /// MPD's `playlist::GetLastLoadedPlaylist()`.
    pub fn last_loaded_playlist(&self) -> &str {
        &self.last_loaded_playlist
    }

    /// Record `name` as the last stored playlist loaded into this queue.
    /// Mirrors MPD's `playlist::SetLastLoadedPlaylist()`, called
    /// unconditionally once a `load` has successfully read the playlist
    /// file, regardless of how many songs ended up queued.
    pub fn set_last_loaded_playlist(&mut self, name: impl Into<String>) {
        self.last_loaded_playlist = name.into();
    }

    pub fn get(&self, position: u32) -> Option<&QueueItem> {
        self.items.get(position as usize)
    }

    pub fn get_by_id(&self, id: u32) -> Option<&QueueItem> {
        self.items.iter().find(|item| item.id == id)
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn version(&self) -> u32 {
        self.version
    }

    pub fn items(&self) -> &[QueueItem] {
        &self.items
    }

    pub fn shuffle(&mut self) {
        use rand::rng;
        use rand::seq::SliceRandom;

        self.items.shuffle(&mut rng());
        self.reindex();
        self.version += 1;
    }

    pub fn shuffle_range(&mut self, start: u32, end: u32) {
        use rand::rng;
        use rand::seq::SliceRandom;

        let start_idx = start as usize;
        let end_idx = end.min(self.items.len() as u32) as usize;

        if start_idx < end_idx && end_idx <= self.items.len() {
            self.items[start_idx..end_idx].shuffle(&mut rng());
            self.reindex();
            self.version += 1;
        }
    }

    pub fn move_item(&mut self, from: u32, to: u32) -> bool {
        if from >= self.items.len() as u32 || to >= self.items.len() as u32 {
            return false;
        }

        let item = self.items.remove(from as usize);
        self.items.insert(to as usize, item);
        self.reindex();
        self.version += 1;
        true
    }

    pub fn move_by_id(&mut self, id: u32, to: u32) -> bool {
        if let Some(from_idx) = self.items.iter().position(|i| i.id == id) {
            if to as usize >= self.items.len() {
                return false;
            }
            let item = self.items.remove(from_idx);
            self.items.insert(to as usize, item);
            self.reindex();
            self.version += 1;
            true
        } else {
            false
        }
    }

    pub fn swap(&mut self, pos1: u32, pos2: u32) -> bool {
        if pos1 >= self.items.len() as u32 || pos2 >= self.items.len() as u32 {
            return false;
        }
        self.items.swap(pos1 as usize, pos2 as usize);
        self.reindex();
        self.version += 1;
        true
    }

    pub fn swap_by_id(&mut self, id1: u32, id2: u32) -> bool {
        let idx1 = self.items.iter().position(|i| i.id == id1);
        let idx2 = self.items.iter().position(|i| i.id == id2);

        if let (Some(i1), Some(i2)) = (idx1, idx2) {
            self.items.swap(i1, i2);
            self.reindex();
            self.version += 1;
            true
        } else {
            false
        }
    }

    pub fn add_at(&mut self, song: Song, position: Option<u32>) -> u32 {
        let id = self.allocate_id();

        let pos = position.unwrap_or(self.items.len() as u32);
        let item = QueueItem {
            id,
            position: pos,
            song: Arc::new(song),
            priority: 0, // Default priority
            range: None, // No range restriction by default
            tags: None,  // No custom tags by default
        };

        if pos as usize >= self.items.len() {
            self.items.push(item);
        } else {
            self.items.insert(pos as usize, item);
        }

        self.reindex();
        self.version += 1;
        id
    }

    /// Set priority for songs in the given position range.
    /// Returns true if at least one item's priority changed.
    pub fn set_priority_range(&mut self, priority: u8, ranges: &[(u32, u32)]) -> bool {
        let mut any_changed = false;
        for &(start, end) in ranges {
            let start_idx = start as usize;
            let end_idx = end.min(self.items.len() as u32) as usize;

            for idx in start_idx..end_idx {
                if idx < self.items.len() && self.items[idx].priority != priority {
                    self.items[idx].priority = priority;
                    any_changed = true;
                }
            }
        }
        if any_changed {
            self.version += 1;
        }
        any_changed
    }

    /// Set priority for songs with the given IDs
    pub fn set_priority_ids(&mut self, priority: u8, ids: &[u32]) -> bool {
        let mut any_changed = false;
        for &id in ids {
            if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
                item.priority = priority;
                any_changed = true;
            }
        }
        if any_changed {
            self.version += 1;
        }
        any_changed
    }

    /// Set playback range for a song with the given ID.
    ///
    /// The range is specified in seconds as (start, end).
    /// Returns true if the item was found.
    pub fn set_range_by_id(&mut self, id: u32, range: Option<(f64, f64)>) -> bool {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            if item.range != range {
                item.range = range;
                self.version += 1;
            }
            true
        } else {
            false
        }
    }

    /// Add a custom tag to a queue item
    ///
    /// Returns true if the item was found and updated.
    pub fn add_tag_by_id(&mut self, id: u32, tag: String, value: String) -> bool {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            let tags = item.tags.get_or_insert_with(HashMap::new);
            let changed = tags.get(&tag).map(String::as_str) != Some(value.as_str());
            tags.insert(tag, value);
            if changed {
                self.version += 1;
            }
            true
        } else {
            false
        }
    }

    /// Clear tags from a queue item
    ///
    /// If tag is Some, clears only that tag. If None, clears all tags.
    /// Returns true if the item was found.
    pub fn clear_tags_by_id(&mut self, id: u32, tag: Option<&str>) -> bool {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == id) {
            let mut changed = false;
            if let Some(tag_name) = tag {
                // Clear specific tag
                if let Some(tags) = &mut item.tags {
                    changed = tags.remove(tag_name).is_some();
                    // If no tags left, remove the HashMap
                    if tags.is_empty() {
                        item.tags = None;
                    }
                }
            } else {
                // Clear all tags
                changed = item.tags.is_some();
                item.tags = None;
            }
            if changed {
                self.version += 1;
            }
            true
        } else {
            false
        }
    }

    /// Get mutable reference to an item by ID
    pub fn get_by_id_mut(&mut self, id: u32) -> Option<&mut QueueItem> {
        self.items.iter_mut().find(|item| item.id == id)
    }

    fn reindex(&mut self) {
        for (idx, item) in self.items.iter_mut().enumerate() {
            item.position = idx as u32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::create_test_song;

    fn make_queue_with_n(n: u32) -> Queue {
        let mut queue = Queue::new();
        for i in 0..n {
            queue.add(create_test_song(i as u64, &i.to_string()));
        }
        queue
    }

    #[test]
    fn test_shuffle_range() {
        let mut queue = Queue::new();

        // Add test items
        for i in 0..10 {
            let song = create_test_song(i as u64, &i.to_string());
            queue.add(song);
        }

        // Get original IDs of items 2-5
        let original_ids: Vec<u32> = (2..5).map(|i| queue.get(i).unwrap().id).collect();

        // Shuffle range 2:5
        queue.shuffle_range(2, 5);

        // Get IDs after shuffle
        let shuffled_ids: Vec<u32> = (2..5).map(|i| queue.get(i).unwrap().id).collect();

        // Check that the same IDs are still in the range (just reordered)
        assert_eq!(original_ids.len(), shuffled_ids.len());
        for id in original_ids {
            assert!(shuffled_ids.contains(&id));
        }

        // Check that items outside the range weren't affected
        assert_eq!(queue.get(0).unwrap().song.tag("title"), Some("Song 0"));
        assert_eq!(queue.get(1).unwrap().song.tag("title"), Some("Song 1"));
        assert_eq!(queue.get(5).unwrap().song.tag("title"), Some("Song 5"));
    }

    #[test]
    fn test_shuffle_range_bounds() {
        let mut queue = Queue::new();

        // Add 5 items
        for i in 0..5 {
            let song = create_test_song(i as u64, &i.to_string());
            queue.add(song);
        }

        // Test with range beyond length - should handle gracefully
        queue.shuffle_range(2, 100);

        // Should still have 5 items
        assert_eq!(queue.len(), 5);
    }

    #[test]
    fn delete_and_delete_id_reindex_and_update_version() {
        let mut queue = make_queue_with_n(4);
        let v0 = queue.version();

        let removed = queue.delete(1).expect("position 1 should exist");
        assert_eq!(removed.position, 1);
        assert_eq!(queue.len(), 3);
        assert_eq!(queue.version(), v0 + 1);
        assert_eq!(queue.get(0).expect("item 0").position, 0);
        assert_eq!(queue.get(1).expect("item 1").position, 1);
        assert_eq!(queue.get(2).expect("item 2").position, 2);

        let delete_none_version = queue.version();
        assert!(queue.delete(99).is_none());
        assert_eq!(queue.version(), delete_none_version);

        let id = queue.get(1).expect("item 1 exists").id;
        let by_id = queue.delete_id(id).expect("id should exist");
        assert_eq!(by_id.id, id);
        assert_eq!(queue.len(), 2);

        let delete_missing_id_version = queue.version();
        assert!(queue.delete_id(999_999).is_none());
        assert_eq!(queue.version(), delete_missing_id_version);
    }

    #[test]
    fn clear_resets_items_and_last_loaded_playlist_and_bumps_version() {
        let mut queue = make_queue_with_n(2);
        queue.set_last_loaded_playlist("favorites");
        let v0 = queue.version();

        queue.clear();

        assert!(queue.is_empty());
        assert_eq!(queue.last_loaded_playlist(), "");
        assert_eq!(queue.version(), v0 + 1);
    }

    #[test]
    fn add_at_inserts_or_appends_and_reindexes() {
        let mut queue = make_queue_with_n(3);

        let inserted_id = queue.add_at(create_test_song(42, "insert"), Some(1));
        assert_eq!(queue.len(), 4);
        assert_eq!(queue.get(1).expect("inserted item").id, inserted_id);
        assert_eq!(queue.get(1).expect("inserted item").position, 1);
        assert_eq!(queue.get(2).expect("shifted item").position, 2);

        let appended_id = queue.add_at(create_test_song(43, "append"), Some(99));
        assert_eq!(queue.get(4).expect("appended item").id, appended_id);

        let none_pos_id = queue.add_at(create_test_song(44, "none"), None);
        assert_eq!(queue.get(5).expect("none-pos append").id, none_pos_id);
    }

    #[test]
    fn move_and_swap_obey_bounds_and_reindex() {
        let mut queue = make_queue_with_n(4);
        let id0 = queue.get(0).expect("item 0").id;
        let id1 = queue.get(1).expect("item 1").id;

        assert!(queue.move_item(0, 2));
        assert_eq!(queue.get(2).expect("moved item").id, id0);
        assert_eq!(queue.get(0).expect("new item 0").id, id1);
        assert_eq!(queue.get(2).expect("reindexed").position, 2);

        let v_after_move = queue.version();
        assert!(!queue.move_item(99, 0));
        assert!(!queue.move_item(0, 99));
        assert_eq!(queue.version(), v_after_move);

        let id_at_0 = queue.get(0).expect("item").id;
        let id_at_1 = queue.get(1).expect("item").id;
        assert!(queue.swap(0, 1));
        assert_eq!(queue.get(0).expect("swapped").id, id_at_1);
        assert_eq!(queue.get(1).expect("swapped").id, id_at_0);

        let v_after_swap = queue.version();
        assert!(!queue.swap(0, 99));
        assert_eq!(queue.version(), v_after_swap);
    }

    #[test]
    fn move_and_swap_by_id_handle_missing_ids() {
        let mut queue = make_queue_with_n(3);
        let id0 = queue.get(0).expect("item").id;
        let id2 = queue.get(2).expect("item").id;

        assert!(queue.move_by_id(id0, 2));
        assert_eq!(queue.get(2).expect("moved").id, id0);

        let v_after_move = queue.version();
        assert!(!queue.move_by_id(999_999, 0));
        assert!(!queue.move_by_id(id2, 99));
        assert_eq!(queue.version(), v_after_move);

        assert!(queue.swap_by_id(id0, id2));
        let v_after_swap = queue.version();
        assert!(!queue.swap_by_id(id0, 777_777));
        assert_eq!(queue.version(), v_after_swap);
    }

    #[test]
    fn priority_range_ids_and_range_updates_follow_version_rules() {
        let mut queue = make_queue_with_n(4);
        let id0 = queue.get(0).expect("item").id;
        let id1 = queue.get(1).expect("item").id;

        let v0 = queue.version();
        assert!(queue.set_priority_range(7, &[(1, 3)]));
        assert_eq!(queue.get(1).expect("item").priority, 7);
        assert_eq!(queue.get(2).expect("item").priority, 7);
        assert_eq!(queue.version(), v0 + 1);

        let v1 = queue.version();
        assert!(!queue.set_priority_range(7, &[(1, 3)]));
        assert_eq!(queue.version(), v1);

        let v2 = queue.version();
        assert!(queue.set_priority_ids(9, &[id0, id1, 999_999]));
        assert_eq!(queue.get_by_id(id0).expect("item").priority, 9);
        assert_eq!(queue.get_by_id(id1).expect("item").priority, 9);
        assert_eq!(queue.version(), v2 + 1);

        let v3 = queue.version();
        assert!(!queue.set_priority_ids(1, &[555_555]));
        assert_eq!(queue.version(), v3);

        let v4 = queue.version();
        assert!(queue.set_range_by_id(id0, Some((1.5, 3.0))));
        assert_eq!(queue.version(), v4 + 1);

        let v5 = queue.version();
        assert!(queue.set_range_by_id(id0, Some((1.5, 3.0))));
        assert_eq!(queue.version(), v5);

        let v6 = queue.version();
        assert!(!queue.set_range_by_id(111_111, Some((0.0, 1.0))));
        assert_eq!(queue.version(), v6);
    }

    #[test]
    fn tag_add_and_clear_behave_and_version_only_changes_on_mutation() {
        let mut queue = make_queue_with_n(1);
        let id = queue.get(0).expect("item").id;

        let v0 = queue.version();
        assert!(queue.add_tag_by_id(id, "mood".to_owned(), "calm".to_owned()));
        assert_eq!(queue.version(), v0 + 1);
        assert_eq!(
            queue
                .get_by_id(id)
                .and_then(|i| i.tags.as_ref())
                .and_then(|m| m.get("mood"))
                .map(String::as_str),
            Some("calm")
        );

        let v1 = queue.version();
        assert!(queue.add_tag_by_id(id, "mood".to_owned(), "calm".to_owned()));
        assert_eq!(queue.version(), v1);

        let v2 = queue.version();
        assert!(queue.clear_tags_by_id(id, Some("missing")));
        assert_eq!(queue.version(), v2);

        let v3 = queue.version();
        assert!(queue.clear_tags_by_id(id, Some("mood")));
        assert_eq!(queue.version(), v3 + 1);
        assert!(queue.get_by_id(id).expect("item").tags.is_none());

        assert!(queue.add_tag_by_id(id, "genre".to_owned(), "rock".to_owned()));
        let v4 = queue.version();
        assert!(queue.clear_tags_by_id(id, None));
        assert_eq!(queue.version(), v4 + 1);

        let v5 = queue.version();
        assert!(queue.clear_tags_by_id(id, None));
        assert_eq!(queue.version(), v5);

        assert!(!queue.add_tag_by_id(999_999, "x".to_owned(), "y".to_owned()));
        assert!(!queue.clear_tags_by_id(999_999, None));
    }

    #[test]
    fn allocate_id_never_returns_zero_after_restore_like_state() {
        // Simulate old/restored state where next_id is 0.
        let mut queue = Queue {
            items: Vec::new(),
            next_id: 0,
            version: 0,
            last_loaded_playlist: String::new(),
        };

        let id1 = queue.add(create_test_song(1, "a"));
        let id2 = queue.add(create_test_song(2, "b"));

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert!(queue.items().iter().all(|it| it.id != 0));
    }
}
