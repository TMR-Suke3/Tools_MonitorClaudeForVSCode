# ADR-0035 Observe recent Codex IDE turns

Show Codex IDE turns in the existing workspace groups and row format.
An internal prefix keeps Codex row identifiers distinct from Claude Code.

The observed rollouts identify turns but lack confirmed tab-close and permission-approval
signals. Recorded GUI questions use the board's waiting state until a matching
answer, turn end, or timeout. A recorded start maps to working for up to 30 minutes and a recorded
finish or interruption to idle for five minutes. These limits bound stale
rows and can hide work exceeding the limit. History is subject to the same
rules after a board restart.

On click, match a visible VS Code or Insiders window with a unique workspace
name in its standard title, foreground it, and invoke that installation's CLI
with the extension's local conversation URI. Custom titles, duplicate
workspace names and session directories that differ from workspace roots can
prevent resolution. Windows routing is subject to the editor's foreground
selection and needs the two-window acceptance check.
