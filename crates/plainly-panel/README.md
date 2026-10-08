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

## What it will not do

A Passage a password manager has marked secret is refused: nothing is sent and
nothing is stored. The panel says so, and there is no "explain anyway". A manager
that does not use that convention is not covered.
