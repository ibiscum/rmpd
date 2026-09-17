# MPD Command Parity Checklist

Auto-generated against MPD command table and local parser metadata.

- Generated: 2026-09-17T15:07:07Z
- Upstream source: src/command/AllCommands.cxx
- Local source: rmpd-protocol/src/parser.rs

## Summary

- Upstream commands: 115
- Local commands: 116
- Matching command+permission: 115
- Permission mismatches: 0
- Matching argument-range parity (for comparable commands): 115
- Argument-range parity mismatches: 0
- Argument-range parity missing in local metadata: 0
- Missing in local: 0
- Extra in local: 1

## Known Exceptions

- noidle appears as extra-local because MPD handles it as a special async control token, not as a static entry in AllCommands.cxx.

## Argument-Range Parity Mismatches

| Command | Upstream Argument Range | Local Argument Range |
|---|---|---|
| (none) | - | - |

## Special-Case Semantics Pass

These checks cover behavior that is not fully represented by MPD's static command table.

| Check | Expected (MPD) | Local Evidence | Status |
|---|---|---|---|
| noidle parse support | yes | parser has noidle token | pass |
| idle parse support | yes | parser has idle token | pass |
| command_list token parse support | yes | parser has begin/end tokens | pass |
| noidle outside idle response | empty response | server returns empty text | pass |
| noidle in command list | ignored | execute_command_list ignores noidle | pass |
| idle in command list | ACK error | explicit cannot-be-used branch | pass |
| command_list_end outside list | unknown command ACK | explicit unknown-command response | pass |
| nested command_list tokens in active list | unknown command ACK | explicit unknown-token branch | pass |

## Permission Mismatches

| Command | Upstream Permission | Local Permission |
|---|---|---|
| (none) | - | - |

## Missing In Local

| Command | Upstream Permission | Upstream Argument Range (min..max) |
|---|---|---|
| (none) | - | - |

## Extra In Local

| Command | Local Permission |
|---|---|
| noidle | PERMISSION_NONE |

## Full Checklist

| Command | Upstream | Local | Upstream Permission | Local Permission | Upstream Argument Range | Local Argument Range | Status |
|---|---|---|---|---|---|---|---|
| add | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 1..2 | 1..2 | match |
| addid | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 1..2 | 1..2 | match |
| addtagid | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 3..3 | 3..3 | match |
| albumart | yes | yes | PERMISSION_READ | PERMISSION_READ | 2..2 | 2..2 | match |
| binarylimit | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 1..1 | 1..1 | match |
| channels | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| clear | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..0 | 0..0 | match |
| clearerror | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..0 | 0..0 | match |
| cleartagid | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 1..2 | 1..2 | match |
| close | yes | yes | PERMISSION_NONE | PERMISSION_NONE | -1..-1 | -1..-1 | match |
| commands | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 0..0 | 0..0 | match |
| config | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 0..0 | 0..0 | match |
| consume | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| count | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..-1 | 1..-1 | match |
| crossfade | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| currentsong | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| decoders | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| delete | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| deleteid | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| delpartition | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 1..1 | 1..1 | match |
| disableoutput | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 1..1 | 1..1 | match |
| enableoutput | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 1..1 | 1..1 | match |
| find | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..-1 | 1..-1 | match |
| findadd | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 1..-1 | 1..-1 | match |
| getfingerprint | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..1 | 1..1 | match |
| getvol | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| idle | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..-1 | 0..-1 | match |
| kill | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | -1..-1 | -1..-1 | match |
| list | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..-1 | 1..-1 | match |
| listall | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..1 | 0..1 | match |
| listallinfo | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..1 | 0..1 | match |
| listfiles | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..1 | 0..1 | match |
| listmounts | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| listneighbors | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| listpartitions | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| listplaylist | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..2 | 1..2 | match |
| listplaylistinfo | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..2 | 1..2 | match |
| listplaylists | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| load | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 1..3 | 1..3 | match |
| lsinfo | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..1 | 0..1 | match |
| mixrampdb | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| mixrampdelay | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| mount | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 2..2 | 2..2 | match |
| move | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..2 | 2..2 | match |
| moveid | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..2 | 2..2 | match |
| moveoutput | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 1..1 | 1..1 | match |
| newpartition | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 1..1 | 1..1 | match |
| next | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..0 | 0..0 | match |
| noidle | no | yes | - | PERMISSION_NONE | - | - | extra-local |
| notcommands | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 0..0 | 0..0 | match |
| outputs | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| outputset | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 3..3 | 3..3 | match |
| partition | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..1 | 1..1 | match |
| password | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 1..1 | 1..1 | match |
| pause | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..1 | 0..1 | match |
| ping | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 0..0 | 0..0 | match |
| play | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..1 | 0..1 | match |
| playid | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..1 | 0..1 | match |
| playlist | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| playlistadd | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 2..3 | 2..3 | match |
| playlistclear | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 1..1 | 1..1 | match |
| playlistdelete | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 2..2 | 2..2 | match |
| playlistfind | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..-1 | 1..-1 | match |
| playlistid | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..1 | 0..1 | match |
| playlistinfo | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..1 | 0..1 | match |
| playlistlength | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..1 | 1..1 | match |
| playlistmove | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 3..3 | 3..3 | match |
| playlistsearch | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..-1 | 1..-1 | match |
| plchanges | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..2 | 1..2 | match |
| plchangesposid | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..2 | 1..2 | match |
| previous | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..0 | 0..0 | match |
| prio | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..-1 | 2..-1 | match |
| prioid | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..-1 | 2..-1 | match |
| protocol | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 0..-1 | 0..-1 | match |
| random | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| rangeid | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 2..2 | 2..2 | match |
| readcomments | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..1 | 1..1 | match |
| readmessages | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| readpicture | yes | yes | PERMISSION_READ | PERMISSION_READ | 2..2 | 2..2 | match |
| rename | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 2..2 | 2..2 | match |
| repeat | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| replay_gain_mode | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| replay_gain_status | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| rescan | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 0..1 | 0..1 | match |
| rm | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 1..1 | 1..1 | match |
| save | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 1..2 | 1..2 | match |
| search | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..-1 | 1..-1 | match |
| searchadd | yes | yes | PERMISSION_ADD | PERMISSION_ADD | 1..-1 | 1..-1 | match |
| searchaddpl | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 2..-1 | 2..-1 | match |
| searchcount | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..-1 | 1..-1 | match |
| searchplaylist | yes | yes | PERMISSION_READ | PERMISSION_READ | 2..4 | 2..4 | match |
| seek | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..2 | 2..2 | match |
| seekcur | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| seekid | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..2 | 2..2 | match |
| sendmessage | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 2..2 | 2..2 | match |
| setvol | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| shuffle | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..1 | 0..1 | match |
| single | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
| stats | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| status | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| sticker | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 3..-1 | 3..-1 | match |
| stickernames | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 0..0 | 0..0 | match |
| stickernamestypes | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 0..1 | 0..1 | match |
| stickertypes | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 0..0 | 0..0 | match |
| stop | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 0..0 | 0..0 | match |
| stringnormalization | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 0..-1 | 0..-1 | match |
| subscribe | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..1 | 1..1 | match |
| swap | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..2 | 2..2 | match |
| swapid | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 2..2 | 2..2 | match |
| tagtypes | yes | yes | PERMISSION_NONE | PERMISSION_NONE | 0..-1 | 0..-1 | match |
| toggleoutput | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 1..1 | 1..1 | match |
| unmount | yes | yes | PERMISSION_ADMIN | PERMISSION_ADMIN | 1..1 | 1..1 | match |
| unsubscribe | yes | yes | PERMISSION_READ | PERMISSION_READ | 1..1 | 1..1 | match |
| update | yes | yes | PERMISSION_CONTROL | PERMISSION_CONTROL | 0..1 | 0..1 | match |
| urlhandlers | yes | yes | PERMISSION_READ | PERMISSION_READ | 0..0 | 0..0 | match |
| volume | yes | yes | PERMISSION_PLAYER | PERMISSION_PLAYER | 1..1 | 1..1 | match |
