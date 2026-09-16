# rmpd Setup: Running Beside Existing MPD

## Can rmpd run beside MPD?

Yes. rmpd can run standalone beside an existing MPD daemon when you avoid resource collisions.

## What Must Be Different

1. Network port and/or bind address
- Both daemons cannot listen on the same address and port.
- MPD commonly uses `127.0.0.1:6600`, and rmpd defaults there too.
- Run rmpd on a different port, for example `6601`.

2. Unix socket path
- If unix sockets are enabled, each daemon needs its own socket file path.

3. Audio output device
- If both daemons target the same exclusive hardware device (for example raw ALSA `hw:`), one may fail or block the other.
- Use different output devices, or keep one daemon idle when the other needs exclusive access.

4. State and database files
- Keep separate state, database, and playlist paths per daemon to avoid overlap.

## Minimal Side-by-Side rmpd Config

```toml
[network]
bind_address = "127.0.0.1"
port = 6601
# unix_socket = "~/.config/rmpd/rmpd.sock"

[general]
# Keep rmpd data separate
# db_file = "~/.config/rmpd/database.db"
# state_file = "~/.config/rmpd/state"
# playlist_directory = "~/.config/rmpd/playlists"

[audio]
# Optional: choose an explicit device different from MPD's device
# device = "hw:CARD=1,DEV=0"
```

## Example Startup Command

```bash
./target/release/rmpd --config ~/.config/rmpd/rmpd.toml --bind 127.0.0.1 --port 6601
```

## Quick Verification Checklist

1. Start MPD and confirm it is listening on its expected port.
2. Start rmpd on a different port.
3. Connect an MPD client to rmpd explicitly using host and port.
4. Confirm both daemons respond independently.
5. Play a short test stream or track and verify output-device behavior.

## Notes

- rmpd and MPD can share the same music library path if needed.
- Exclusive low-level outputs are the most common source of conflicts.
- If both use non-exclusive mixer paths, coexistence is usually easier.

## References

- [README.md](README.md)
- [rmpd.toml](rmpd.toml)
- [rmpd-core/src/config.rs](rmpd-core/src/config.rs)
