//! Publish-subscribe messaging system
//!
//! Models MPD client messaging semantics:
//! - subscriptions are tracked per client
//! - `sendmessage` fans out to all subscribed clients
//! - `readmessages` drains only the requesting client's inbox

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;

/// MPD caps pending client-to-client messages per client.
const MAX_MESSAGES_PER_CLIENT: usize = 64;

/// A message in a channel
#[derive(Debug, Clone)]
pub struct Message {
    pub channel: String,
    pub text: String,
}

/// Delivery outcome for `send_message`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SendMessageResult {
    /// At least one subscribed client accepted the message.
    pub delivered: bool,
    /// At least one recipient transitioned from empty inbox to non-empty.
    pub notified_idle_message: bool,
}

#[derive(Debug, Default)]
struct ClientMailbox {
    subscriptions: HashSet<String>,
    messages: VecDeque<Message>,
}

/// Message broker managing channels and message delivery
#[derive(Debug, Clone)]
pub struct MessageBroker {
    inner: Arc<RwLock<MessageBrokerInner>>,
}

#[derive(Debug)]
struct MessageBrokerInner {
    /// Monotonic id generator for registering clients.
    next_client_id: u64,
    /// Per-client subscriptions and message inboxes.
    clients: HashMap<u64, ClientMailbox>,
    /// Number of active subscribers per channel
    subscriber_counts: HashMap<String, usize>,
}

impl MessageBroker {
    /// Create a new message broker
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(MessageBrokerInner {
                next_client_id: 1,
                clients: HashMap::new(),
                subscriber_counts: HashMap::new(),
            })),
        }
    }

    /// Register a new connected protocol client and return its id.
    pub async fn register_client(&self) -> u64 {
        let mut inner = self.inner.write().await;
        let id = inner.next_client_id;
        inner.next_client_id = inner.next_client_id.saturating_add(1);
        inner.clients.insert(id, ClientMailbox::default());
        id
    }

    /// Unregister a client and remove all of its subscriptions.
    /// Returns the number of removed channel subscriptions.
    pub async fn unregister_client(&self, client_id: u64) -> usize {
        let mut inner = self.inner.write().await;
        let Some(client) = inner.clients.remove(&client_id) else {
            return 0;
        };

        let mut removed = 0;
        for channel in client.subscriptions {
            if let Some(count) = inner.subscriber_counts.get_mut(&channel) {
                if *count > 0 {
                    *count -= 1;
                }
                if *count == 0 {
                    inner.subscriber_counts.remove(&channel);
                }
            }
            removed += 1;
        }

        removed
    }

    /// Register a channel subscription for a client.
    /// Returns true if this is a new subscription.
    pub async fn register_subscriber(&self, client_id: u64, channel: &str) -> bool {
        let mut inner = self.inner.write().await;
        let Some(client) = inner.clients.get_mut(&client_id) else {
            return false;
        };

        if !client.subscriptions.insert(channel.to_string()) {
            return false;
        }

        *inner
            .subscriber_counts
            .entry(channel.to_string())
            .or_insert(0) += 1;
        true
    }

    /// Unregister a channel subscription for a client.
    /// Returns true if the client was subscribed.
    pub async fn unregister_subscriber(&self, client_id: u64, channel: &str) -> bool {
        let mut inner = self.inner.write().await;
        let Some(client) = inner.clients.get_mut(&client_id) else {
            return false;
        };

        if !client.subscriptions.remove(channel) {
            return false;
        }

        if let Some(count) = inner.subscriber_counts.get_mut(channel) {
            if *count > 0 {
                *count -= 1;
            }
            if *count == 0 {
                inner.subscriber_counts.remove(channel);
            }
        }

        true
    }

    /// Send a message to all currently subscribed clients.
    pub async fn send_message(&self, channel: String, text: String) -> SendMessageResult {
        let mut inner = self.inner.write().await;

        let mut delivered = false;
        let mut notified_idle_message = false;

        for client in inner.clients.values_mut() {
            if !client.subscriptions.contains(&channel) {
                continue;
            }

            if client.messages.len() >= MAX_MESSAGES_PER_CLIENT {
                continue;
            }

            if client.messages.is_empty() {
                notified_idle_message = true;
            }

            client.messages.push_back(Message {
                channel: channel.clone(),
                text: text.clone(),
            });
            delivered = true;
        }

        SendMessageResult {
            delivered,
            notified_idle_message,
        }
    }

    /// Drain all pending messages for one client.
    pub async fn read_messages(&self, client_id: u64) -> Vec<Message> {
        let mut inner = self.inner.write().await;
        let Some(client) = inner.clients.get_mut(&client_id) else {
            return Vec::new();
        };
        client.messages.drain(..).collect()
    }

    /// Get list of channel names with active subscribers.
    pub async fn list_channels(&self) -> Vec<String> {
        let inner = self.inner.read().await;
        let mut result: Vec<String> = inner.subscriber_counts.keys().cloned().collect();
        result.sort();
        result
    }
}

impl Default for MessageBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_and_read_message_for_one_client() {
        let broker = MessageBroker::new();
        let client = broker.register_client().await;

        assert!(broker.register_subscriber(client, "test").await);
        let outcome = broker
            .send_message("test".to_string(), "hello".to_string())
            .await;
        assert!(outcome.delivered);
        assert!(outcome.notified_idle_message);

        let messages = broker.read_messages(client).await;
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].channel, "test");
        assert_eq!(messages[0].text, "hello");
    }

    #[tokio::test]
    async fn sendmessage_fans_out_to_all_subscribers() {
        let broker = MessageBroker::new();
        let client_a = broker.register_client().await;
        let client_b = broker.register_client().await;

        assert!(broker.register_subscriber(client_a, "shared").await);
        assert!(broker.register_subscriber(client_b, "shared").await);

        let outcome = broker
            .send_message("shared".to_string(), "hello".to_string())
            .await;
        assert!(outcome.delivered);
        assert!(outcome.notified_idle_message);

        let messages_a = broker.read_messages(client_a).await;
        let messages_b = broker.read_messages(client_b).await;

        assert_eq!(messages_a.len(), 1);
        assert_eq!(messages_b.len(), 1);
        assert_eq!(messages_a[0].text, "hello");
        assert_eq!(messages_b[0].text, "hello");
    }

    #[tokio::test]
    async fn messages_are_consumed_per_client() {
        let broker = MessageBroker::new();
        let client = broker.register_client().await;

        assert!(broker.register_subscriber(client, "test").await);
        let _ = broker
            .send_message("test".to_string(), "hello".to_string())
            .await;

        let messages = broker.read_messages(client).await;
        assert_eq!(messages.len(), 1);

        let messages = broker.read_messages(client).await;
        assert_eq!(messages.len(), 0);
    }

    #[tokio::test]
    async fn channels_list_only_active_subscriptions() {
        let broker = MessageBroker::new();
        let client = broker.register_client().await;

        assert!(broker.register_subscriber(client, "channel1").await);
        assert!(broker.register_subscriber(client, "channel2").await);

        let channels = broker.list_channels().await;
        assert_eq!(channels.len(), 2);
        assert!(channels.contains(&"channel1".to_string()));
        assert!(channels.contains(&"channel2".to_string()));

        assert!(broker.unregister_subscriber(client, "channel1").await);
        assert!(broker.unregister_subscriber(client, "channel2").await);
        let channels = broker.list_channels().await;
        assert!(channels.is_empty());
    }

    #[tokio::test]
    async fn max_messages_limit_matches_mpd() {
        let broker = MessageBroker::new();
        let client = broker.register_client().await;

        assert!(broker.register_subscriber(client, "test").await);

        for i in 0..MAX_MESSAGES_PER_CLIENT {
            let outcome = broker
                .send_message("test".to_string(), format!("msg{}", i))
                .await;
            assert!(outcome.delivered);
        }

        // MPD drops new messages when a client's queue is full.
        let overflow = broker
            .send_message("test".to_string(), "overflow".to_string())
            .await;
        assert!(!overflow.delivered);
        assert!(!overflow.notified_idle_message);

        let messages = broker.read_messages(client).await;
        assert_eq!(messages.len(), MAX_MESSAGES_PER_CLIENT);
        assert_eq!(messages[0].text, "msg0");
        assert_eq!(messages[MAX_MESSAGES_PER_CLIENT - 1].text, "msg63");
    }

    #[tokio::test]
    async fn sendmessage_idle_edge_trigger_matches_mpd() {
        let broker = MessageBroker::new();
        let client = broker.register_client().await;
        assert!(broker.register_subscriber(client, "test").await);

        let first = broker
            .send_message("test".to_string(), "one".to_string())
            .await;
        assert!(first.delivered);
        assert!(first.notified_idle_message);

        let second = broker
            .send_message("test".to_string(), "two".to_string())
            .await;
        assert!(second.delivered);
        assert!(!second.notified_idle_message);

        let _ = broker.read_messages(client).await;

        let third = broker
            .send_message("test".to_string(), "three".to_string())
            .await;
        assert!(third.delivered);
        assert!(third.notified_idle_message);
    }

    #[tokio::test]
    async fn unregister_client_removes_subscriptions() {
        let broker = MessageBroker::new();
        let client = broker.register_client().await;
        assert!(broker.register_subscriber(client, "test").await);
        assert_eq!(broker.unregister_client(client).await, 1);

        let channels = broker.list_channels().await;
        assert!(channels.is_empty());
    }
}
