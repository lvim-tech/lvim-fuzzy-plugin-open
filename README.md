# lvim-fuzzy-plugin-open

The **open** plugin of the lvim fuzzy tools (`lvim-fuzzy-commander`, `lvim-fuzzy-cli`, the Neovim picker
`lvim-fuzzy-picker`). It decides **which program opens which kind of file**; the tool that asked runs it. What it
decides is its configuration, so one rule serves all three tools — and each tool may override it for itself.

It is a `process` plugin (JSON lines) of the `lvim-fuzzy-plugin` host and ships **inside** `lvim-fuzzy` and
`lvim-fuzzy-commander`: a tool writes `~/.config/lvim-plugins/lvim-fuzzy-plugin-open/plugin.toml` only when its config says so —
`[plugins.lvim-fuzzy-plugin-open] install = "bundled"` (or the path of a program that answers as the plugin); the Neovim picker goes through
`lvim-fuzzy`. A `plugin.toml` of your own with the same name is never touched.

## What it answers

One `open` instruction (`lvim_fuzzy_ui::open` reads it):

| answer | the tool does |
|---|---|
| `{"op":"open","action":"editor"}` | opens the file in its own editor (the picker: a buffer) |
| `{"op":"open","action":"exec","cmd":"imv '/p/a.png'"}` | starts the command, detached (a GUI program); the finder stays open |
| `{"op":"open","missing":["`imv` is not installed (…)"]}` | says so, opens nothing |
| `{"op":"pass"}` | no plugin has an answer for the file |

## Configuration

Defaults: text and directories → `editor`, everything else → `system` (`xdg-open`).

A **type** is a group of files; its `engine` opens it. A group matches by `ext`, `name`, `mime` (`"image/*"`) and / or
`kind` (`dir`, `binary`, `text`); every key given must hold. The most specific group wins (`ext` / `name` / `mime`, then
`kind`, then `priority`); a group with no key matches every file.

```toml
[types.pictures]            # by MIME type
mime   = ["image/*"]
engine = "imv"

[types.pdf]                 # by extension
ext    = ["pdf"]
engine = "zathura"

[engines.imv]               # an engine of your own: a command, {} = the file (quoted); without it the file is appended
kind    = "command"
command = "imv {}"

[engines.zathura]
kind    = "command"
command = "zathura"
```

Built-in engines: `editor`, `system` (xdg-open), `host` (pass: the tool's own way). A program an engine needs that is
not on the PATH is said (`missing`) and nothing is opened.

## What is configured now

`lvim-fuzzy-cli open --list` prints the map that is in effect for lfz (the defaults, the shared file and lfz's own layer),
best group first, with a mark for a program that is not installed:

```
lvim-fuzzy-plugin-open — for lfz
pics         ext png       → imv (command: imv {})   ✓
pdf          ext pdf       → zathura (command: zathura)  ✗ not installed
directories  kind dir      → editor
text         kind text     → editor
other        (everything)  → system  ✓
```

For another tool: `--host lvim-fuzzy-picker --plugins '{"lvim-fuzzy-plugin-open": {…}}'` with that tool's layer.

## Who overrides what

defaults (`plugin.toml`) < `~/.config/lvim-plugins/lvim-fuzzy-plugin-open.toml` (every tool) < the tool's own layer:

- `lvim-fuzzy-cli`: `[plugins.lvim-fuzzy-plugin-open]` in `~/.config/lvim-fuzzy/config.toml`
- `lvim-fuzzy-commander`: `[plugins.lvim-fuzzy-plugin-open]` in `~/.config/lvim-commander/config.toml`
- `lvim-fuzzy-picker`: `plugins = { ["lvim-fuzzy-plugin-open"] = { … } }` in `setup`

so lvc may open a PDF in `zathura` while lfz uses `evince`. (The old `[open]` table of lvc's config and the shared
`open.toml` are gone: this is their replacement; lvc's config reports `[open]` with a pointer here.)

## Build

`cargo build --release`; `plugin.toml`'s `exec` then points at `target/release/lvim-fuzzy-plugin-open`
(`lvim-fuzzy-plugin install` does both for a plugin from git). Not needed for use: the tools carry the same code.

## License

BSD-3-Clause
