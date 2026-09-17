# MPD Parity Matrix

This document tracks protocol-behavior parity between rmpd and upstream MPD.

References:
- Upstream source: https://github.com/MusicPlayerDaemon/MPD
- Upstream messaging handlers: src/command/MessageCommands.cxx
- Upstream protocol docs: doc/protocol.rst

Status legend:
- Parity: Behavior matches upstream in implementation and tests.
- Partial: Some behavior matches, but edge cases or limits differ.
- Gap: Known behavior mismatch or missing implementation.
- Not Assessed: No focused parity audit yet.

## Current Matrix

| Area | Upstream Reference | Status | Current rmpd State | Remaining Work |
|---|---|---|---|---|
| Messaging: subscribe/unsubscribe validation and limits | src/command/MessageCommands.cxx, src/client/Subscribe.cxx | Parity | Channel validation, duplicate/full errors, and unsubscribe not-subscribed behavior align. | Keep regression tests green. |
| Messaging: sendmessage fanout | src/command/MessageCommands.cxx::handle_send_message | Parity | `sendmessage` now delivers to each subscribed client inbox (not a shared queue). | None for core fanout semantics. |
| Messaging: readmessages consumption | src/command/MessageCommands.cxx::handle_read_messages | Parity | `readmessages` drains only the caller client inbox. | None for current behavior. |
| Messaging: channels output | src/command/MessageCommands.cxx::handle_channels | Parity | `channels` now lists active subscription channel names only. | None for current behavior. |
| Messaging: queue bounds and overflow behavior | src/client/Client.hxx, src/client/Subscribe.cxx::PushMessage | Parity | Per-client queue cap is 64; overflow drops new message and reports no delivery if all recipients full. | Consider stress tests in CI for high fanout load. |
| Messaging idle notification edge-triggering | src/client/Subscribe.cxx::PushMessage + client idle path | Parity | Message idle event is emitted only when at least one recipient transitions empty -> non-empty. | None for current behavior. |
| Command table coverage checklist generation | src/command/AllCommands.cxx | Parity | Auto-generated checklist now compares upstream command table with local parser command metadata. | Keep checklist regenerated when parser or upstream command table changes. |
| Idle subsystem event aggregation behavior | src/client/Idle.cxx | Partial | rmpd aggregates queued events in idle responses; broad behavior appears aligned but not fully audited subsystem-by-subsystem. | Audit all subsystem edge cases and race behavior; add conformance tests for mixed subsystem bursts. |
| Command list semantics (`command_list_*`) | command processing flow in MPD | Partial | Basic behavior implemented with size caps and async-command guardrails. | Audit exact ACK/index behavior and close semantics for all malformed list scenarios. |
| Permission model parity | src/command/AllCommands.cxx + permission checks | Partial | Core permission bits and many command metadata checks exist. | Run exhaustive parity audit against command table and side effects by permission level. |
| Partition-scoped behavior | src/client, src/Partition, command handlers | Not Assessed | Partition commands exist and tests cover basic flows. | Verify strict parity for cross-partition visibility, messaging scope, and output movement edge cases. |
| Sticker command behavior | src/command/StickerCommands.cxx | Not Assessed | Implemented with tests, but parity-level comparison incomplete. | Audit domain resolution order, exact ACK messages, and sort/window edge handling. |
| Database/search command semantics | DB command handlers + protocol docs | Not Assessed | Broad functionality exists. | Compare filter grammar, quoting rules, sorting/window semantics, and errors with upstream. |

## Completed in this change set

- Reworked broker semantics to MPD-style per-client inboxes.
- Changed message delivery from shared channel drain to subscriber fanout.
- Changed `channels` to active subscriptions only.
- Added idle message edge-trigger behavior.
- Added focused conformance tests for deterministic `readmessages`, fanout, and `channels` behavior.
- Added generated command checklist at `docs/mpd-command-parity-checklist.md`.
- Added generator script at `scripts/generate_mpd_command_parity_checklist.sh`.

## Suggested Next Parity Milestones

1. Add snapshot tests for exact ACK codes/messages on parse and runtime failures.
2. Add targeted parity suites for idle, command-list, and permission-denied behavior.
3. Audit partition scoping across commands (especially messaging, outputs, and queue views).
4. Expand CI to include conformance test groups as separate required jobs.
