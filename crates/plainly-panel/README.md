# plainly-panel

The panel: press a key and the Passage on the clipboard becomes an Explanation,
in a surface that never takes focus and never interrupts typing. The compositor
spawns a process per press; the first keeps the panel and later presses hand
their activation to it, so there is never a second one. The process exits when
the panel closes, so nothing of it stays resident.

## niri

```kdl
binds {
    Mod+Shift+E { spawn "plainly-panel" "--clipboard"; hotkey-overlay-title="Explain clipboard"; }
}
```

The spawn inherits the compositor's environment rather than the dev shell's, so
use the `nix build` wrapper on `PATH` rather than `target/debug/plainly-panel`.

## Other sessions

The panel is a layer surface where the compositor publishes `data-control`, which
is how it reads the clipboard without focus: niri and the wlroots family. GNOME
publishes neither data-control nor layer-shell, so there the panel degrades to an
ordinary window that takes focus, and GTK's own clipboard works. X11 is not
supported.

## What the panel reads out of `config.toml`

Every trigger re-reads the file, so a change takes effect at the next key press
rather than at the next login. The keys it reads:

| key | what it does |
| --- | --- |
| `app.panel_corner` | `top-left` / `top-right` / `bottom-left` / `bottom-right`. A corner is resolution-independent: changing resolution or monitor re-anchors the panel rather than leaving it off-screen. That holds on the layer surface, where the corner is a set of layer-shell anchors the compositor re-evaluates, and it is best-effort on the focused-window fallback, where a Wayland compositor may ignore a move request. |
| `app.panel_display` | `current`, the output the compositor places the panel on, or `output-1`, `output-2`, … for one screen. An output this session does not have falls back to `current`. |
| `app.panel_width` / `app.panel_height` | The width the text wraps at, and a ceiling on the whole panel. The ceiling is held inside the screen's workarea, so a panel this tall can never put its ✕ off the display. |
| `app.panel_autohide` + `app.panel_autohide_seconds` | Two independent keys: the switch says whether the panel dismisses itself, the number says how long. A duration of `0` is not a second spelling of "never", the switch is, and a number outside 5–300 falls back to 20. |
| `app.show_*` | Which sections the panel draws. |

## The title bar

`Plainly` · provenance chip · `复制` · `⟳` · `⤢` · `✕`.

- the **chip** names the provider, the model, and whether the model runs on this
  machine (`本地` / `云端`);
- **复制** copies the Explanation as the panel words it, in the interface's
  language. The status line beside the chip says `已复制` or `复制失败`, because a
  button that renamed itself would be a button that stopped saying what it does;
- **⟳** generates again past the cache. On a local model it also moves to a hosted
  provider when one has a key; with nothing to move to it re-runs the same local
  model, and the note under the content says which of the two it will be. The
  clipboard is re-read, so this is the Passage the person last copied rather than
  the one already on screen;
- **⤢** spawns `plainly-desktop`, which opens the history window or hands over to
  the one already open; a failure says `找不到 plainly-desktop` in the status line,
  because the name of the missing program is the whole of what a person can act on;
- **✕** closes the panel. It is there in every state, and it is the one exit path
  that does not depend on the session: `keyboard_mode=NONE` means Esc does
  nothing.

Under the body, fixed rather than scrolled with it, a locally hosted model gets
one line: its idiom explanations may be confidently wrong, so check them against
the original above or press ⟳.

## What it will not do

A Passage a password manager has marked secret is refused, and the panel says so:
nothing is sent and nothing is stored. The root `README.md` has the exact rule and
its limits.
