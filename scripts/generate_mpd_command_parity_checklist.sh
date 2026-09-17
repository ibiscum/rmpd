#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_FILE="${ROOT_DIR}/docs/mpd-command-parity-checklist.md"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

UPSTREAM_FILE="${TMP_DIR}/AllCommands.cxx"
LOCAL_FILE="${ROOT_DIR}/rmpd-protocol/src/parser.rs"
LOCAL_SERVER_FILE="${ROOT_DIR}/rmpd-protocol/src/server.rs"
UPSTREAM_TSV="${TMP_DIR}/upstream.tsv"
LOCAL_PERM_TSV="${TMP_DIR}/local-perm.tsv"
LOCAL_ARG_RANGE_PARITY_TSV="${TMP_DIR}/local-arg-range-parity.tsv"
LOCAL_META_TSV="${TMP_DIR}/local-meta.tsv"
MERGED_TSV="${TMP_DIR}/merged.tsv"

curl -fsSL "https://raw.githubusercontent.com/MusicPlayerDaemon/MPD/master/src/command/AllCommands.cxx" -o "${UPSTREAM_FILE}"

perl -ne '
if (/\{\s*"([^"]+)"\s*,\s*(PERMISSION_[A-Z_]+)\s*,\s*([^,]+)\s*,\s*([^,]+)\s*,/) {
  my ($name, $perm, $min, $max) = ($1, $2, $3, $4);
  $min =~ s/^\s+|\s+$//g;
  $max =~ s/^\s+|\s+$//g;
  print "$name\t$perm\t$min\t$max\n";
}
' "${UPSTREAM_FILE}" | sort -t $'\t' -k1,1 -u > "${UPSTREAM_TSV}"

perl -ne '
if (/#\[command\(name = "([^"]+)"(?:, permission = ([0-9]+))?\)/) {
  my ($name, $perm_n) = ($1, $2);
  next if $name eq "unknown";
  next if $name eq "command_list";
  my %perm_map = (
    "0" => "PERMISSION_NONE",
    "1" => "PERMISSION_READ",
    "2" => "PERMISSION_ADD",
    "4" => "PERMISSION_CONTROL",
    "8" => "PERMISSION_ADMIN",
    "16" => "PERMISSION_PLAYER",
  );
  my $perm = defined($perm_n) ? ($perm_map{$perm_n} // "PERMISSION_UNKNOWN") : "PERMISSION_NONE";
  print "$name\t$perm\n";
}
' "${LOCAL_FILE}" | sort -t $'\t' -k1,1 -u > "${LOCAL_PERM_TSV}"

perl -0777 -ne '
my $src = $_;
if ($src =~ /fn\s+command_arity\s*\([^)]*\)\s*->\s*Option<\(i32,\s*i32\)>\s*\{(.*?)\n\}\n\nfn\s+command_parser\s*\(/s) {
  my $body = $1;
  while ($body =~ /((?:\s*"[^"]+"\s*(?:\|\s*"[^"]+"\s*)*))=>\s*(?:\{\s*)?\(([-0-9]+)\s*,\s*([-0-9]+)\)\s*(?:\}\s*)?\s*(?:,|(?=\s*"|\s*_\s*=>))/sg) {
    my ($lhs, $min, $max) = ($1, $2, $3);
    my @names = ($lhs =~ /"([^"]+)"/g);
    for my $name (@names) {
      print "$name\t$min\t$max\n";
    }
  }
}
' "${LOCAL_FILE}" | sort -t $'\t' -k1,1 -u > "${LOCAL_ARG_RANGE_PARITY_TSV}"

# Local parser intentionally omits unchecked-argument-range commands from command_arity.
if ! grep -q $'^close\t' "${LOCAL_ARG_RANGE_PARITY_TSV}"; then
  printf 'close\t-1\t-1\n' >> "${LOCAL_ARG_RANGE_PARITY_TSV}"
fi
if ! grep -q $'^kill\t' "${LOCAL_ARG_RANGE_PARITY_TSV}"; then
  printf 'kill\t-1\t-1\n' >> "${LOCAL_ARG_RANGE_PARITY_TSV}"
fi
sort -t $'\t' -k1,1 -u -o "${LOCAL_ARG_RANGE_PARITY_TSV}" "${LOCAL_ARG_RANGE_PARITY_TSV}"

join -t $'\t' -a1 -a2 -e '' -o '0,1.2,2.2,2.3' "${LOCAL_PERM_TSV}" "${LOCAL_ARG_RANGE_PARITY_TSV}" > "${LOCAL_META_TSV}"

join -t $'\t' -a1 -a2 -e '' -o '0,1.2,1.3,1.4,2.2,2.3,2.4' "${UPSTREAM_TSV}" "${LOCAL_META_TSV}" > "${MERGED_TSV}"

upstream_count="$(wc -l < "${UPSTREAM_TSV}" | tr -d ' ')"
local_count="$(wc -l < "${LOCAL_META_TSV}" | tr -d ' ')"
perm_match_count="$(awk -F'\t' '$2 != "" && $5 != "" && $2 == $5 {c++} END {print c+0}' "${MERGED_TSV}")"
perm_mismatch_count="$(awk -F'\t' '$2 != "" && $5 != "" && $2 != $5 {c++} END {print c+0}' "${MERGED_TSV}")"
arg_range_parity_match_count="$(awk -F'\t' '$2 != "" && $5 != "" && $3 != "" && $4 != "" && $6 != "" && $7 != "" && $3 == $6 && $4 == $7 {c++} END {print c+0}' "${MERGED_TSV}")"
arg_range_parity_mismatch_count="$(awk -F'\t' '$2 != "" && $5 != "" && $3 != "" && $4 != "" && $6 != "" && $7 != "" && ($3 != $6 || $4 != $7) {c++} END {print c+0}' "${MERGED_TSV}")"
arg_range_parity_missing_local_count="$(awk -F'\t' '$2 != "" && $5 != "" && $3 != "" && $4 != "" && ($6 == "" || $7 == "") {c++} END {print c+0}' "${MERGED_TSV}")"
missing_count="$(awk -F'\t' '$2 != "" && $5 == "" {c++} END {print c+0}' "${MERGED_TSV}")"
extra_count="$(awk -F'\t' '$2 == "" && $5 != "" {c++} END {print c+0}' "${MERGED_TSV}")"
noidle_extra="$(awk -F'\t' '$1 == "noidle" && $2 == "" && $5 != "" {print 1; found=1} END {if (!found) print 0}' "${MERGED_TSV}")"
generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Second pass: special-case semantics that are not represented in AllCommands.cxx
has_noidle_parser="no"
has_idle_parser="no"
has_command_list_begin_parser="no"
has_command_list_end_parser="no"
has_noidle_outside_idle_empty="no"
has_noidle_inside_list_ignored="no"
has_idle_in_list_rejected="no"
has_command_list_end_outside_unknown="no"
has_command_list_tokens_in_list_unknown="no"

grep -q '"noidle" => Ok(Command::NoIdle)' "${LOCAL_FILE}" && has_noidle_parser="yes"
grep -q '"idle" => {' "${LOCAL_FILE}" && has_idle_parser="yes"
grep -q '"command_list_begin" => Ok(Command::CommandListBegin)' "${LOCAL_FILE}" && has_command_list_begin_parser="yes"
grep -q '"command_list_end" => Ok(Command::CommandListEnd)' "${LOCAL_FILE}" && has_command_list_end_parser="yes"

grep -q 'Ok(Command::NoIdle) => {' "${LOCAL_SERVER_FILE}" && grep -q 'Response::Text(String::new())' "${LOCAL_SERVER_FILE}" && has_noidle_outside_idle_empty="yes"
grep -q 'Ok(Command::NoIdle) => {' "${LOCAL_SERVER_FILE}" && grep -q 'Silently ignore noidle inside command list' "${LOCAL_SERVER_FILE}" && has_noidle_inside_list_ignored="yes"
grep -q 'cannot be used inside a command list' "${LOCAL_SERVER_FILE}" && has_idle_in_list_rejected="yes"
grep -q 'command_list_end"' "${LOCAL_SERVER_FILE}" && grep -q 'Ok(Command::CommandListEnd) => {' "${LOCAL_SERVER_FILE}" && has_command_list_end_outside_unknown="yes"
grep -q 'Command::CommandListBegin | Command::CommandListOkBegin | Command::CommandListEnd' "${LOCAL_SERVER_FILE}" && has_command_list_tokens_in_list_unknown="yes"

{
  echo "# MPD Command Parity Checklist"
  echo
  echo "Auto-generated against MPD command table and local parser metadata."
  echo
  echo "- Generated: ${generated_at}"
  echo "- Upstream source: src/command/AllCommands.cxx"
  echo "- Local source: rmpd-protocol/src/parser.rs"
  echo
  echo "## Summary"
  echo
  echo "- Upstream commands: ${upstream_count}"
  echo "- Local commands: ${local_count}"
  echo "- Matching command+permission: ${perm_match_count}"
  echo "- Permission mismatches: ${perm_mismatch_count}"
  echo "- Matching argument-range parity (for comparable commands): ${arg_range_parity_match_count}"
  echo "- Argument-range parity mismatches: ${arg_range_parity_mismatch_count}"
  echo "- Argument-range parity missing in local metadata: ${arg_range_parity_missing_local_count}"
  echo "- Missing in local: ${missing_count}"
  echo "- Extra in local: ${extra_count}"
  echo
  echo "## Known Exceptions"
  echo
  if [[ "${noidle_extra}" == "1" ]]; then
    echo "- noidle appears as extra-local because MPD handles it as a special async control token, not as a static entry in AllCommands.cxx."
  else
    echo "- (none)"
  fi
  echo
  echo "## Argument-Range Parity Mismatches"
  echo
  echo "| Command | Upstream Argument Range | Local Argument Range |"
  echo "|---|---|---|"
  awk -F'\t' '$2 != "" && $5 != "" && $3 != "" && $4 != "" && $6 != "" && $7 != "" && ($3 != $6 || $4 != $7) {printf("| %s | %s..%s | %s..%s |\n", $1, $3, $4, $6, $7); found=1} END {if (!found) print "| (none) | - | - |"}' "${MERGED_TSV}"
  echo
  echo "## Special-Case Semantics Pass"
  echo
  echo "These checks cover behavior that is not fully represented by MPD's static command table."
  echo
  echo "| Check | Expected (MPD) | Local Evidence | Status |"
  echo "|---|---|---|---|"
  if [[ "${has_noidle_parser}" == "yes" ]]; then
    echo "| noidle parse support | yes | parser has noidle token | pass |"
  else
    echo "| noidle parse support | yes | parser token missing | fail |"
  fi
  if [[ "${has_idle_parser}" == "yes" ]]; then
    echo "| idle parse support | yes | parser has idle token | pass |"
  else
    echo "| idle parse support | yes | parser token missing | fail |"
  fi
  if [[ "${has_command_list_begin_parser}" == "yes" && "${has_command_list_end_parser}" == "yes" ]]; then
    echo "| command_list token parse support | yes | parser has begin/end tokens | pass |"
  else
    echo "| command_list token parse support | yes | parser missing begin/end token(s) | fail |"
  fi
  if [[ "${has_noidle_outside_idle_empty}" == "yes" ]]; then
    echo "| noidle outside idle response | empty response | server returns empty text | pass |"
  else
    echo "| noidle outside idle response | empty response | no matching server branch | fail |"
  fi
  if [[ "${has_noidle_inside_list_ignored}" == "yes" ]]; then
    echo "| noidle in command list | ignored | execute_command_list ignores noidle | pass |"
  else
    echo "| noidle in command list | ignored | no matching server branch | fail |"
  fi
  if [[ "${has_idle_in_list_rejected}" == "yes" ]]; then
    echo "| idle in command list | ACK error | explicit cannot-be-used branch | pass |"
  else
    echo "| idle in command list | ACK error | no matching server branch | fail |"
  fi
  if [[ "${has_command_list_end_outside_unknown}" == "yes" ]]; then
    echo "| command_list_end outside list | unknown command ACK | explicit unknown-command response | pass |"
  else
    echo "| command_list_end outside list | unknown command ACK | no matching server branch | fail |"
  fi
  if [[ "${has_command_list_tokens_in_list_unknown}" == "yes" ]]; then
    echo "| nested command_list tokens in active list | unknown command ACK | explicit unknown-token branch | pass |"
  else
    echo "| nested command_list tokens in active list | unknown command ACK | no matching server branch | fail |"
  fi
  echo
  echo "## Permission Mismatches"
  echo
  echo "| Command | Upstream Permission | Local Permission |"
  echo "|---|---|---|"
  awk -F'\t' '$2 != "" && $5 != "" && $2 != $5 {printf("| %s | %s | %s |\n", $1, $2, $5); found=1} END {if (!found) print "| (none) | - | - |"}' "${MERGED_TSV}"
  echo
  echo "## Missing In Local"
  echo
  echo "| Command | Upstream Permission | Upstream Argument Range (min..max) |"
  echo "|---|---|---|"
  awk -F'\t' '$2 != "" && $5 == "" {printf("| %s | %s | %s..%s |\n", $1, $2, $3, $4); found=1} END {if (!found) print "| (none) | - | - |"}' "${MERGED_TSV}"
  echo
  echo "## Extra In Local"
  echo
  echo "| Command | Local Permission |"
  echo "|---|---|"
  awk -F'\t' '$2 == "" && $5 != "" {printf("| %s | %s |\n", $1, $5); found=1} END {if (!found) print "| (none) | - |"}' "${MERGED_TSV}"
  echo
  echo "## Full Checklist"
  echo
  echo "| Command | Upstream | Local | Upstream Permission | Local Permission | Upstream Argument Range | Local Argument Range | Status |"
  echo "|---|---|---|---|---|---|---|---|"
  awk -F'\t' '
    {
      if ($2 != "" && $5 != "") {
        perm_ok = ($2 == $5);
        arg_range_parity_ok = ($3 != "" && $4 != "" && $6 != "" && $7 != "" && $3 == $6 && $4 == $7);
        arg_range_parity_missing = ($6 == "" || $7 == "");
        if (!perm_ok) {
          status = "permission-mismatch";
        } else if (arg_range_parity_missing) {
          status = "argument-range-parity-missing-local";
        } else if (!arg_range_parity_ok) {
          status = "argument-range-parity-mismatch";
        } else {
          status = "match";
        }
        local_arg_range_parity = ($6 == "" || $7 == "") ? "-" : ($6 ".." $7);
        printf("| %s | yes | yes | %s | %s | %s..%s | %s | %s |\n", $1, $2, $5, $3, $4, local_arg_range_parity, status);
      } else if ($2 != "" && $5 == "") {
        printf("| %s | yes | no | %s | - | %s..%s | - | missing-local |\n", $1, $2, $3, $4);
      } else if ($2 == "" && $5 != "") {
        local_arg_range_parity = ($6 == "" || $7 == "") ? "-" : ($6 ".." $7);
        printf("| %s | no | yes | - | %s | - | %s | extra-local |\n", $1, $5, local_arg_range_parity);
      }
    }
  ' "${MERGED_TSV}"
} > "${OUT_FILE}"

echo "Generated ${OUT_FILE}"