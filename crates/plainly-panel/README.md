# plainly-panel

The panel: press a key and the Passage on the clipboard becomes an Explanation,
in a surface that never takes focus and never interrupts typing. The compositor
spawns a process per press; the first keeps the panel and later presses hand
their activation to it, so there is never a second one. Nothing stays resident
once the panel is closed.

## niri

```kdl
binds {
    Mod+Shift+E { spawn "plainly-panel" "--clipboard"; hotkey-overlay-title="Explain clipboard"; }
}
```

`spawn` does not go through a shell and does not expand `~`, so `plainly-panel`
has to be on `PATH` by name. Pressing the key again re-reads the clipboard and
refreshes the one panel — it does not open a second one.

## Other sessions

The panel is a layer surface where the compositor publishes `data-control`, which
is where it can read the clipboard without focus: niri and the wlroots family.
Where it does not — GNOME's Mutter publishes neither data-control nor
layer-shell — the panel degrades to a window that takes focus, and GTK's own
clipboard works there. X11 is not supported in v0.

## What the panel reads out of `config.toml`

Every trigger re-reads the file, so a change takes effect at the next key press
rather than at the next login — the second press reuses the panel, and it is
`Panel::start` that reads before it runs. Four families of key:

| key | what it does |
| --- | --- |
| `app.panel_corner` | `top-left` / `top-right` / `bottom-left` / `bottom-right`. Corners are resolution-independent: changing resolution or monitor re-anchors the panel rather than leaving it off-screen. This **holds** on the layer surface (the niri/wlroots path), where the corner is a set of layer-shell anchors the compositor re-evaluates. On the focused-window fallback the corner is applied with `gtk_window_move`, which a Wayland compositor may ignore for an ordinary toplevel — so there the corner is best-effort rather than a guarantee. |
| `app.panel_display` | `current` — the output the compositor places the panel on — or `output-1`, `output-2`, … for one screen. An output this session does not have falls back to `current`. |
| `app.panel_autohide` + `app.panel_autohide_seconds` | Two independent keys: the switch says *whether* the panel dismisses itself, the number says how long. A duration of `0` is not a second spelling of "never" — the switch is — and a number outside 5–300 falls back to 20. |
| `app.show_*` | Which sections the panel draws. |

## The title bar

`Plainly` · provenance chip · `复制` · `⟳` · `⤢` · `✕`.

- the **chip** names the provider, the model, and whether the model runs on this
  machine (`本地` / `云端`);
- **复制** copies the Explanation as the panel words it, in the interface's
  language. The status line beside the chip says `已复制` or `复制失败` — a layer
  surface has nowhere to put a banner, and a button that renamed itself would be a
  button that stopped saying what it does;
- **⟳** generates again **past the cache**, and — only when the provider in
  effect is a local model **and** a hosted provider has a key — moves to that
  hosted provider. The clipboard is re-read, so this is the Passage the person
  last copied rather than the one already on screen. A local model with nothing
  to move to re-runs the same local model, and the note under the content says
  which of the two it will be; a hosted provider always re-runs itself;
- **⤢** spawns `plainly-desktop`, which opens the history window or hands over to
  the one already open; a failure says `找不到 plainly-desktop` in the status line,
  because the name of the missing program is the whole of what a person can act on;
- **✕** closes the panel. It is there in every state, and it is the one exit path
  that does not depend on the session. `keyboard_mode=NONE` means Esc does
  nothing.

Under the body, and fixed rather than scrolled with it, a locally hosted model
gets one line: its idiom explanations may be confidently wrong, so check them
against the original above or press ⟳.

## What it will not do

A Passage a password manager has marked secret is refused: nothing is sent and
nothing is stored. The panel says so, and there is no "explain anyway". A manager
that does not use that convention is not covered.
