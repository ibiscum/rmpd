//! Connection and server control commands
//!
//! This module handles commands related to server configuration, control,
//! and connection management.

use super::{AppState, ResponseBuilder};
use crate::commands::utils::{ACK_ERROR_PASSWORD, ACK_ERROR_PERMISSION, ACK_ERROR_SYS};
use crate::connection::ConnectionState;

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut diff = a.len() ^ b.len();
    let max_len = a.len().max(b.len());

    for i in 0..max_len {
        let av = *a.get(i).unwrap_or(&0);
        let bv = *b.get(i).unwrap_or(&0);
        diff |= (av ^ bv) as usize;
    }

    diff == 0
}

/// Return server configuration
///
/// Returns server configuration information from AppState. MPD restricts
/// this command to clients connected over the local Unix socket
/// (`Client::IsLocal`); remote clients get an ACK.
pub async fn handle_config_command(state: &AppState, conn_state: &ConnectionState) -> String {
    if !conn_state.is_local {
        return ResponseBuilder::error(
            ACK_ERROR_PERMISSION,
            0,
            "config",
            "Command only permitted to local clients",
        );
    }

    let mut resp = ResponseBuilder::new();

    if let Some(music_dir) = &state.music_dir {
        resp.field("music_directory", music_dir);
    }

    if let Some(playlist_dir) = &state.playlist_dir {
        resp.field("playlist_directory", playlist_dir);
    }

    // rmpd has no PCRE support compiled in (matches MPD builds without
    // HAVE_PCRE): omit the field entirely rather than reporting "pcre: 0".
    resp.ok()
}

/// Kill the server (graceful shutdown)
///
/// Sends a shutdown signal to the main server loop, triggering graceful shutdown.
pub async fn handle_kill_command(state: &AppState) -> String {
    let Some(shutdown_tx) = &state.shutdown_tx else {
        return ResponseBuilder::error(ACK_ERROR_SYS, 0, "kill", "shutdown channel not configured");
    };

    if shutdown_tx.send(()).is_err() {
        return ResponseBuilder::error(ACK_ERROR_SYS, 0, "kill", "shutdown channel closed");
    }

    ResponseBuilder::new().ok()
}

/// Handle the `password` command.
///
/// If no password is configured any value is accepted.
/// On success all permissions are granted; on failure an ACK error is returned.
pub async fn handle_password_command(
    state: &AppState,
    conn_state: &mut ConnectionState,
    password: &str,
) -> String {
    match &state.password {
        None => {
            // No password configured — any password is accepted, grant all permissions
            conn_state.grant_all_permissions();
            ResponseBuilder::new().ok()
        }
        Some(configured) => {
            if constant_time_eq(password, configured.as_str()) {
                conn_state.grant_all_permissions();
                ResponseBuilder::new().ok()
            } else {
                ResponseBuilder::error(ACK_ERROR_PASSWORD, 0, "password", "incorrect password")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{PERMISSION_ALL, PERMISSION_NONE};

    #[test]
    fn constant_time_eq_matches_expected_results() {
        assert!(constant_time_eq("secret", "secret"));
        assert!(!constant_time_eq("secret", "secreT"));
        assert!(!constant_time_eq("secret", "secretx"));
        assert!(!constant_time_eq("secret", ""));
    }

    #[tokio::test]
    async fn kill_fails_when_shutdown_sender_missing() {
        let state = AppState::new();
        let resp = handle_kill_command(&state).await;
        assert!(resp.contains("ACK [52@0] {kill}"), "got: {resp}");
        assert!(
            resp.contains("shutdown channel not configured"),
            "got: {resp}"
        );
    }

    #[tokio::test]
    async fn kill_fails_when_shutdown_sender_closed() {
        let mut state = AppState::new();
        let (tx, rx) = tokio::sync::broadcast::channel::<()>(1);
        drop(rx);
        state.set_shutdown_sender(tx);

        let resp = handle_kill_command(&state).await;
        assert!(resp.contains("ACK [52@0] {kill}"), "got: {resp}");
        assert!(resp.contains("shutdown channel closed"), "got: {resp}");
    }

    #[tokio::test]
    async fn kill_succeeds_with_live_shutdown_receiver() {
        let mut state = AppState::new();
        let (tx, mut rx) = tokio::sync::broadcast::channel::<()>(1);
        state.set_shutdown_sender(tx);

        let resp = handle_kill_command(&state).await;
        assert_eq!(resp, "OK\n");
        let got = rx
            .recv()
            .await
            .expect("receiver should observe shutdown signal");
        assert_eq!(got, ());
    }

    #[tokio::test]
    async fn password_success_and_failure_preserve_permissions() {
        let mut state = AppState::new();
        state.set_password(Some("secret".to_owned()));

        let mut conn = ConnectionState::new();
        conn.permissions = PERMISSION_NONE;

        let bad = handle_password_command(&state, &mut conn, "wrong").await;
        assert!(bad.contains("ACK [3@0] {password}"), "got: {bad}");
        assert_eq!(conn.permissions, PERMISSION_NONE);

        let ok = handle_password_command(&state, &mut conn, "secret").await;
        assert_eq!(ok, "OK\n");
        assert_eq!(conn.permissions, PERMISSION_ALL);
    }
}
