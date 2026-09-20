# Contributing

## Issues are welcome

Bug reports are genuinely useful, and the most useful kind is **the board said one thing
and the session was doing another**. That is the failure this product exists to avoid, and
it is the one thing its author cannot find alone: it depends on how you work, how many
windows you keep, and which editor build you run.

A report that is easy to act on says:

- what the board showed, and what the session was actually doing
- your Windows and VS Code versions, and which Claude Code extension build
- whether exact mode was on — the menu's **diagnosis** answers this, and its four lines are
  worth pasting in whole
- if a click did not raise the right window, the output of `mcv-board --check-raise`

Neither the diagnosis nor `--check-raise` prints conversation content. `--check-raise`
does print window titles and folder paths, so read it before pasting if either is private.

## Code contributions are not accepted

This is a personal project and its design is not open to change, so pull requests are
turned off rather than left open and refused. That is not a judgement on anybody's patch —
it is that reviewing and maintaining them is work this project is not set up to carry.

You are of course free to fork it and take it wherever you like; the licence is MIT.

## Security

Do **not** open an issue for a security problem. See [SECURITY.md](SECURITY.md) for how to
report one privately.
