use crate::event::EventBus;
use crate::messaging::MessageBroker;
use crate::queue::Queue;
use crate::state::PlayerStatus;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Represents a single partition with its own playback state
///
/// Each partition has independent:
/// - Queue (playlist)
/// - Player status (playing, paused, stopped)
/// - Playback engine
/// - Output assignments
#[derive(Clone, Debug)]
pub struct PartitionState {
    /// Partition name
    pub name: String,

    /// Playback queue for this partition
    pub queue: Arc<RwLock<Queue>>,

    /// Player status (current song, position, state, etc.)
    pub status: Arc<RwLock<PlayerStatus>>,

    /// Lock-free state access for performance
    pub atomic_state: Arc<std::sync::atomic::AtomicU8>,

    /// Event bus for this partition
    pub event_bus: EventBus,

    /// Message broker for client messaging
    pub message_broker: MessageBroker,

    /// Output IDs assigned to this partition
    pub assigned_outputs: Arc<RwLock<Vec<u32>>>,
}

impl PartitionState {
    /// Create a new partition with the given name
    pub fn new(name: String) -> Self {
        let event_bus = EventBus::new();
        let status = Arc::new(RwLock::new(PlayerStatus::default()));
        let atomic_state = Arc::new(std::sync::atomic::AtomicU8::new(
            crate::state::PlayerState::Stop.to_atomic(),
        ));

        Self {
            name,
            queue: Arc::new(RwLock::new(Queue::new())),
            status,
            atomic_state,
            event_bus,
            message_broker: MessageBroker::new(),
            assigned_outputs: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Assign an output to this partition
    pub async fn assign_output(&self, output_id: u32) {
        let mut outputs = self.assigned_outputs.write().await;
        if !outputs.contains(&output_id) {
            outputs.push(output_id);
        }
    }

    /// Remove an output from this partition
    pub async fn remove_output(&self, output_id: u32) -> bool {
        let mut outputs = self.assigned_outputs.write().await;
        let before = outputs.len();
        outputs.retain(|&id| id != output_id);
        outputs.len() != before
    }

    /// Get all assigned output IDs
    pub async fn get_outputs(&self) -> Vec<u32> {
        self.assigned_outputs.read().await.clone()
    }
}

/// Information about a partition for serialization
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartitionInfo {
    pub name: String,
    pub output_ids: Vec<u32>,
}

/// Manager for multiple partitions
pub struct PartitionManager {
    partitions: RwLock<HashMap<String, Arc<PartitionState>>>,
}

impl PartitionManager {
    /// Create a new partition manager with a default partition
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Create a new partition
    pub async fn create_partition(&self, name: String) -> Result<Arc<PartitionState>, String> {
        let mut partitions = self.partitions.write().await;

        if partitions.contains_key(&name) {
            return Err(format!("Partition already exists: {}", name));
        }

        let partition = Arc::new(PartitionState::new(name.clone()));
        partitions.insert(name, partition.clone());

        Ok(partition)
    }

    /// Delete a partition
    pub async fn delete_partition(&self, name: &str) -> Result<(), String> {
        // Cannot delete default partition
        if name == "default" {
            return Err("Cannot delete default partition".to_string());
        }

        let mut partitions = self.partitions.write().await;

        if !partitions.contains_key(name) {
            return Err(format!("Partition not found: {}", name));
        }

        partitions.remove(name);
        Ok(())
    }

    /// Get a partition by name
    pub async fn get_partition(&self, name: &str) -> Option<Arc<PartitionState>> {
        let partitions = self.partitions.read().await;
        partitions.get(name).cloned()
    }

    /// List all partition names
    pub async fn list_partitions(&self) -> Vec<String> {
        let partitions = self.partitions.read().await;
        let mut names: Vec<String> = partitions.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get partition count
    pub async fn count(&self) -> usize {
        let partitions = self.partitions.read().await;
        partitions.len()
    }

    /// Move an output from one partition to another
    pub async fn move_output(
        &self,
        output_id: u32,
        from_partition: &str,
        to_partition: &str,
    ) -> Result<(), String> {
        // Get both partitions and extract Arc references
        let (from, to) = {
            let partitions = self.partitions.read().await;

            let from = partitions
                .get(from_partition)
                .ok_or_else(|| format!("Source partition not found: {}", from_partition))?
                .clone();

            let to = partitions
                .get(to_partition)
                .ok_or_else(|| format!("Target partition not found: {}", to_partition))?
                .clone();

            (from, to)
        }; // Drop read lock here

        // Remove from source; moving from a partition that does not own the
        // output is an error and likely indicates stale caller state.
        if !from.remove_output(output_id).await {
            return Err(format!(
                "Output {output_id} not assigned to source partition: {from_partition}"
            ));
        }

        // Add to target
        to.assign_output(output_id).await;

        Ok(())
    }

    /// Get all partition info for serialization
    pub async fn get_all_info(&self) -> Vec<PartitionInfo> {
        let partitions = self.partitions.read().await;
        let mut infos = Vec::new();

        for (name, partition) in partitions.iter() {
            infos.push(PartitionInfo {
                name: name.clone(),
                output_ids: partition.get_outputs().await,
            });
        }

        infos
    }

    /// Load partitions from saved info
    pub async fn load_partitions(&self, infos: Vec<PartitionInfo>) {
        for info in infos {
            let name = info.name;
            let output_ids = info.output_ids;
            let partition = if let Some(existing) = self.get_partition(&name).await {
                existing
            } else {
                match self.create_partition(name.clone()).await {
                    Ok(p) => p,
                    Err(e) => {
                        tracing::warn!("failed to load partition '{name}': {e}");
                        continue;
                    }
                }
            };
            for output_id in output_ids {
                partition.assign_output(output_id).await;
            }
        }
    }
}

impl Default for PartitionManager {
    fn default() -> Self {
        let mut partitions = HashMap::new();
        // Always pre-populate the "default" partition — MPD always has it.
        partitions.insert(
            "default".to_string(),
            Arc::new(PartitionState::new("default".to_string())),
        );
        Self {
            partitions: RwLock::new(partitions),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_partition() {
        let manager = PartitionManager::new();

        let result = manager.create_partition("test".to_string()).await;
        assert!(result.is_ok());

        let partition = result.unwrap();
        assert_eq!(partition.name, "test");
    }

    #[tokio::test]
    async fn test_create_duplicate_partition() {
        let manager = PartitionManager::new();

        manager.create_partition("test".to_string()).await.unwrap();
        let result = manager.create_partition("test".to_string()).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already exists"));
    }

    #[tokio::test]
    async fn test_delete_partition() {
        let manager = PartitionManager::new();

        manager.create_partition("test".to_string()).await.unwrap();
        let result = manager.delete_partition("test").await;

        assert!(result.is_ok());
        assert!(manager.get_partition("test").await.is_none());
    }

    #[tokio::test]
    async fn test_cannot_delete_default_partition() {
        let manager = PartitionManager::new();

        // "default" partition is already created by PartitionManager::new(),
        // so we just try to delete it directly.
        let result = manager.delete_partition("default").await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot delete default"));
    }

    #[tokio::test]
    async fn test_list_partitions() {
        let manager = PartitionManager::new();

        manager.create_partition("part1".to_string()).await.unwrap();
        manager.create_partition("part2".to_string()).await.unwrap();

        let names = manager.list_partitions().await;
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"part1".to_string()));
        assert!(names.contains(&"part2".to_string()));
        assert!(names.contains(&"default".to_string()));
        assert_eq!(names, vec!["default", "part1", "part2"]);
    }

    #[tokio::test]
    async fn test_output_assignment() {
        let partition = PartitionState::new("test".to_string());

        partition.assign_output(0).await;
        partition.assign_output(1).await;

        let outputs = partition.get_outputs().await;
        assert_eq!(outputs.len(), 2);
        assert!(outputs.contains(&0));
        assert!(outputs.contains(&1));
    }

    #[tokio::test]
    async fn test_remove_output() {
        let partition = PartitionState::new("test".to_string());

        partition.assign_output(0).await;
        partition.assign_output(1).await;
        assert!(partition.remove_output(0).await);

        let outputs = partition.get_outputs().await;
        assert_eq!(outputs.len(), 1);
        assert!(outputs.contains(&1));
    }

    #[tokio::test]
    async fn test_remove_output_missing_returns_false() {
        let partition = PartitionState::new("test".to_string());
        assert!(!partition.remove_output(42).await);
    }

    #[tokio::test]
    async fn test_move_output() {
        let manager = PartitionManager::new();

        let part1 = manager.create_partition("part1".to_string()).await.unwrap();
        let part2 = manager.create_partition("part2".to_string()).await.unwrap();

        part1.assign_output(0).await;

        manager.move_output(0, "part1", "part2").await.unwrap();

        assert_eq!(part1.get_outputs().await.len(), 0);
        assert_eq!(part2.get_outputs().await.len(), 1);
    }

    #[tokio::test]
    async fn test_move_output_rejects_wrong_source_partition() {
        let manager = PartitionManager::new();
        let part1 = manager.create_partition("part1".to_string()).await.unwrap();
        let _part2 = manager.create_partition("part2".to_string()).await.unwrap();

        part1.assign_output(0).await;
        let err = manager
            .move_output(0, "part2", "part1")
            .await
            .expect_err("move should fail when source does not own output");
        assert!(err.contains("not assigned to source partition"), "{err}");
    }

    #[tokio::test]
    async fn test_default_impl_contains_default_partition() {
        let manager = PartitionManager::default();
        assert_eq!(manager.count().await, 1);
        assert!(manager.get_partition("default").await.is_some());
    }

    #[tokio::test]
    async fn test_load_partitions_updates_existing_default() {
        let manager = PartitionManager::new();
        manager
            .load_partitions(vec![
                PartitionInfo {
                    name: "default".to_string(),
                    output_ids: vec![7],
                },
                PartitionInfo {
                    name: "room".to_string(),
                    output_ids: vec![9],
                },
            ])
            .await;

        let default = manager.get_partition("default").await.unwrap();
        assert!(default.get_outputs().await.contains(&7));

        let room = manager.get_partition("room").await.unwrap();
        assert!(room.get_outputs().await.contains(&9));
    }
}
