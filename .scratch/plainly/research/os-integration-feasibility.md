# Research: Tauri 2 + Rust cross-platform OS-integration feasibility

Provenance: research subagent, 2026-10-05, for the Wayfinder charting of `idea.md`.
Question: can "user copies text -> explanation panel appears" be built cross-platform on
Tauri 2 + Rust, and what does each OS actually permit?
Verdict up front: **the copy-triggered, cursor-positioned popup is not achievable uniformly.**
The hotkey-first design is the only cross-platform core.

**Section 5 is a follow-up addendum** that answers the niri / layer-shell / IPC questions, and
corrects one premise: **niri is Smithay-based, not wlroots-based.** It also narrows the
"copy-detection is impossible" claim to GNOME/Mutter — on niri, KDE and Windows it works, and on
niri an unfocused background process can read the clipboard.

---

## 1. GLOBAL HOTKEYS

### Rust crate + Tauri plugin (the default path)

- `global-hotkey` README states supported platforms: Windows, macOS, **Linux (X11 Only)**.
  https://github.com/tauri-apps/global-hotkey
- Its Wayland issue is still OPEN, labelled "help wanted", last activity 2025-06-27:
  https://github.com/tauri-apps/global-hotkey/issues/28
  - A contributor explains why the portal was never wired in: you can bind once, cannot unbind, must
    listen for activation/deactivation/shortcut-changed; "for application developers... it would be
    more reasonable to implement global shortcuts for wayland using separate crate like ashpd":
    https://github.com/tauri-apps/global-hotkey/issues/28#issuecomment-2518588893
  - Tauri maintainer (FabianLars, 2025-06-27): no one is assigned; "this has to rely on community
    contributions".
    https://github.com/tauri-apps/global-hotkey/issues/28#issuecomment-3012666121
- `tauri-plugin-global-shortcut` README claims Linux ✓ with no Wayland caveat
  (https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/global-shortcut/README.md), but
  the plugin's source just wraps `global_hotkey` directly — no ashpd/D-Bus/portal code path at all
  (https://raw.githubusercontent.com/tauri-apps/plugins-workspace/v2/plugins/global-shortcut/src/lib.rs).
  **Conclusion: the plugin is X11-only in practice.** Real-world report (Mar 2026, closed) of the
  Tauri shortcut plugin simply never firing on COSMIC, Sway and Hyprland, with the app forced to use
  an evdev daemon or compositor keybinds instead: https://github.com/cjpais/Handy/issues/949
- "Silently only via XWayland?" — worse than that: there is no evidence of a working XWayland path;
  a native Wayland client has no X11 connection, and one downstream project had to ship a fix
  exporting `DISPLAY` so hotkey registration worked at all (third-party commit, anecdotal):
  https://github.com/danielrosehill/handy-wayland-autostart-fix/commit/ff1c20e6ada0d2f734c96a748ad0eed5224a133a

### Wayland protocol situation

- Wayland deliberately has no global-key-grab protocol; the sanctioned mechanism is the XDG desktop
  portal. Portal introduced in xdg-desktop-portal 1.16.0 (2022-12-12):
  https://github.com/flatpak/xdg-desktop-portal/releases/tag/1.16.0
- Interface spec (currently **version 2**): `CreateSession`, `BindShortcuts`, `ListShortcuts`,
  `ConfigureShortcuts` (v2); signals `Activated`, `Deactivated`, `ShortcutsChanged`; `BindShortcuts`
  "will typically result in the portal presenting a dialog"; **"An application can only attempt to
  bind shortcuts of a session once"**; `Activated`/`Deactivated` option dicts carry
  `activation_token`: https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html
- BindShortcuts takes an app-supplied shortcut ID + `description` + optional `preferred_trigger`
  (XDG shortcuts spec format); returned shortcuts give `description` + `trigger_description` (so you
  must render the user's actual binding yourself).
- activation token gap: historically `Activated` lacked `activation_token`, tracked in
  https://github.com/flatpak/xdg-desktop-portal/issues/1678 (closed 2025-09-12, "completed"); the
  token is now documented in the v2 spec, so a portal-driven hotkey *can* ask the compositor to
  raise a window.

### Backend support matrix (the decisive part)

- **KDE Plasma**: `xdg-desktop-portal-kde` MR !80 ("It uses systemsettings and KGlobalAccel to
  implement it"): https://invent.kde.org/plasma/xdg-desktop-portal-kde/-/merge_requests/80 — present
  at least since Plasma 6.0. Two gaps fixed recently:
  - shortcuts were re-created on every run (session token in the component name) → fixed in Plasma
    **6.3.0** (Jan 2025): https://bugs.kde.org/show_bug.cgi?id=492992
  - an app could not shrink/clear its own shortcut set → fixed in **6.5.3** (Nov 2025):
    https://bugs.kde.org/show_bug.cgi?id=483838
  - still open (KCM does not represent portal-registered apps correctly):
    https://bugs.kde.org/show_bug.cgi?id=474294
- **GNOME**: GlobalShortcuts landed for **GNOME 48** — "With GNOME 48, it is now possible for apps
  to register system-wide global shortcuts... fully supported by GNOME 48":
  https://release.gnome.org/48/developers/ (merge:
  https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/merge_requests/208)
  - BUT xdg-desktop-portal-gnome issue #183 is **still open** (created 2025-09-10, updated
    2026-02-21): `BindShortcuts` returns `Method BindShortcuts is not implemented on interface
    org.gnome.Settings.GlobalShortcutsProvider` on GNOME 48 and 49rc1:
    https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/work_items/183 and
    https://github.com/bilelmoussaoui/ashpd/issues/302 . Practical reading: GNOME portal shortcuts
    work on a full desktop install (GPU Screen Recorder works) but are environment-fragile.
- **wlroots / Sway: NOT implemented.** `xdg-desktop-portal-wlr` issue is still **open** (created
  2022-09-30, last updated 2025-12-28, 15 comments):
  https://github.com/emersion/xdg-desktop-portal-wlr/issues/240
- **Hyprland**: its own portal ships a GlobalShortcuts backend
  (`src/portals/GlobalShortcuts.cpp`), plus its own protocol
  `hyprland-global-shortcuts-v1` (https://wayland.app/protocols/hyprland-global-shortcuts-v1) and a
  docs page: https://wiki.hypr.land/configuring/core/binds/globals/ — **NOT content-verified**
  (code.hyprland.org serves an anti-bot challenge); treat as source-listed only.
- **COSMIC**: no portal evidence found; real-world report says Tauri global shortcuts do not fire
  there (Handy #949).

### Reachable from Rust today? YES, via `ashpd`

- `ashpd` 0.13.13 (published 2026-09-28) has module `ashpd::desktop::global_shortcuts`, feature
  `global_shortcuts`: https://docs.rs/ashpd/latest/ashpd/desktop/global_shortcuts/index.html
- API: `GlobalShortcuts::new()/with_connection()`, `version()`,
  `create_session(CreateSessionOptions) -> Session`,
  `bind_shortcuts(&session, &[NewShortcut], Option<&WindowIdentifier>, BindShortcutsOptions)`,
  `list_shortcuts`, `configure_shortcuts`, `receive_activated()`, `receive_deactivated()`,
  `receive_shortcuts_changed()` (async `Stream`s):
  https://docs.rs/ashpd/latest/ashpd/desktop/global_shortcuts/struct.GlobalShortcuts.html
- Depends on `zbus ^5.13`, optional `tokio ^1.43`; no GTK/Wayland requirement for the D-Bus side.
  Usable from a Tauri app if run on your own async runtime/thread (Tauri's GTK main loop is not a
  tokio runtime).
- Known gaps: one bind attempt per session; no unbind method; shortcuts are session-scoped and the
  app must reuse stable shortcut IDs across runs (exactly the KDE bug 492992 fail mode); the user's
  actual combo may differ from `preferred_trigger`; `Deactivated` enables push-to-talk.

### Fallbacks people actually ship on Wayland

- Compositor keybinding → spawn your CLI. Hyprland: https://wiki.hypr.land/configuring/core/binds/globals/
  ; COSMIC custom shortcuts file (documented inside https://github.com/cjpais/Handy/issues/949)
- Raw device access via evdev/libinput, bypassing the compositor; needs membership in the `input`
  group. Working script + rationale ("evdev reads raw kernel input events at /dev/input/event*,
  before the Wayland compositor processes them"): https://github.com/cjpais/Handy/issues/949

---

## 2. CLIPBOARD MONITORING / READING

### Wayland fundamentals

- Core protocol: selection is delivered to the **focused** client via `wl_data_device`. Direct
  evidence: wl-clipboard's own BUGS section — "Unless the Wayland compositor implements the wlroots
  data-control protocol, wl-clipboard has to resort to using a hack to access the clipboard: it will
  briefly pop up a tiny transparent surface (window)... In some cases the Wayland compositor doesn't
  give focus to the popup surface, which prevents wl-clipboard from accessing the clipboard and
  manifests as a hang": https://man.archlinux.org/man/wl-paste.1.en
- Same conclusion mechanically: "wl-copy cannot bind any data-control protocol, so it falls back to
  creating an xdg_toplevel surface to acquire keyboard focus (required by the core wl_data_device
  protocol). KWin never activates this surface, so wl-copy blocks forever":
  https://bugs.launchpad.net/ubuntu/+source/wl-clipboard/+bug/2163106
- `wl-paste --watch`: continuously watches the clipboard and runs a command per new selection,
  **"This mode requires a compositor that supports the wlroots data-control protocol."** It sets
  `CLIPBOARD_STATE=data|nil|clear|sensitive` for the child; the man page admits `clear` is never
  actually set and `sensitive` only when `x-kde-passwordManagerHint` is offered.
- Data-control protocols: `ext-data-control-v1` is a **staging** protocol granting a *privileged*
  client selection control (https://wayland.app/protocols/ext-data-control-v1); the older
  `wlr-data-control-unstable-v1` (https://wayland.app/protocols/wlr-data-control-unstable-v1).
  ext-data-control shipped in wayland-protocols v1.39; wl-clipboard 2.3.0 added support (issue
  closed 2025-04-12): https://github.com/bugaevc/wl-clipboard/issues/242
- **Compositor support (the crux)**:
  - wlroots family (Sway, Hyprland, river): data-control available (ext and/or wlr) — see
    wl-clipboard-rs's requirement statement: "The protocol used for clipboard interaction is
    `ext-data-control` or `wlr-data-control`": https://github.com/YaLTeR/wl-clipboard-rs
  - **niri: NOT wlroots-based** — it is a Smithay-based Rust compositor that implements most of the
    important wlr protocols (layer-shell, gamma-control, screencopy) plus ext-data-control. Protocol
    support must be checked per protocol, never assumed from a "wlroots" label. Verified on the
    dogfood machine: `zwlr_layer_shell_v1` v5, `zwlr_data_control_manager_v1` v2,
    `ext_data_control_manager_v1` v1, wl-clipboard 2.3.0.
  - KWin: ported to `ext-data-control` (KWin MR !6606) and dropped the wlr variant; Plasma 6.6
    exposes only `ext_data_control_manager_v1` (verified by `wayland-info`):
    https://bugs.launchpad.net/ubuntu/+source/wl-clipboard/+bug/2163106
  - **GNOME/Mutter: implements NEITHER protocol.** Mutter issue closed 2025-02-28 with 0 merge
    requests: https://gitlab.gnome.org/GNOME/mutter/-/work_items/3941
- Portal: there is **no general clipboard portal**. `org.freedesktop.portal.Clipboard` "does not
  create its own sessions. Instead, it extends sessions created by other portals" (RemoteDesktop,
  InputCapture); needs `RequestClipboard()` before `Start()`; clipboard access for input-capture
  sessions is "only available while the session is active":
  https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Clipboard.html —
  unusable as a generic background clipboard watcher.

### X11

- Change notification is event-driven, not polled: XFIXES `XFixesSelectSelectionInput` (request) and
  `xXFixesSelectionNotifyEvent` (with `owner`, `selection`, `timestamp`, `selectionTimestamp`):
  https://gitlab.freedesktop.org/xorg/proto/xorgproto/-/raw/master/include/X11/extensions/xfixesproto.h
  — you still need the CLIPBOARD (and separately PRIMARY) selection atoms.

### macOS

- Detection by polling `NSPasteboard.changeCount`: "The change count starts at zero when a client
  creates the receiver and becomes the first owner. The change count subsequently increments each
  time the pasteboard ownership changes."
  https://developer.apple.com/documentation/appkit/nspasteboard/changecount
- **Privacy constraint (hard)**: `NSPasteboard.AccessBehavior` exists since **macOS 15.4**: "The
  default behavior for the General pasteboard is to ask upon programmatic access... Once
  programmatic pasteboard access triggers the first pasteboard access alert, the state automatically
  changes to `.ask`. At this point the app starts being shown in System Settings, where the user can
  toggle the behavior between `.ask`, `.alwaysAllow`, and `.alwaysDeny`." User-originated **and
  paste-related** access is always allowed without notification:
  https://developer.apple.com/documentation/appkit/nspasteboard/accessbehavior-swift.enum
  - Consequence: a background app that silently reads the clipboard after every copy hits the macOS
    alert on first use and can be permanently denied. A user gesture (hotkey) is the safe design —
    but note the docs only exempt "user originated **and paste related**" access, so a
    hotkey-triggered programmatic read may still alert.

### Windows

- `AddClipboardFormatListener(hwnd)`: "it is posted a WM_CLIPBOARDUPDATE message whenever the
  contents of the clipboard have changed" (Vista+, user32):
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-addclipboardformatlistener
- `GetClipboardSequenceNumber()`: increments "whenever the contents of the clipboard change or the
  clipboard is emptied"; returns 0 without `WINSTA_ACCESSCLIPBOARD`; with delayed rendering the
  number is not incremented until rendering happens:
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getclipboardsequencenumber
- Neither API tells you whether a copy came from a "sensitive" app — no Microsoft doc found (gap).

### Rust crates

- **`arboard`** (repo now `rustdesk-org/arboard`; 3.6.1): Linux default is **X11**; "fear not because
  Wayland works with the X11 protocol just as well" (i.e. via XWayland); optional
  `wayland-data-control` feature backed by `wl-clipboard-rs`, prioritised over X11 with automatic
  fallback; caveat in their own words: "in my tests the wayland backend did not keep the clipboard
  contents after the process exited". Read/write only — **no change-watch API**.
  https://github.com/rustdesk-org/arboard
- **`clipboard-rs`** 0.3.5: README platform table says **"Linux(X11)"**, with "Watch Changes ✅" for
  that column; exposes `ClipboardWatcherContext` + `ClipboardHandler::on_clipboard_change`, X11 read
  timeout 500 ms overridable via `ClipboardContextX11Options`. Cargo.toml confirms
  `[features] wayland = ["dep:wl-clipboard-rs"]` with `default = ["image"]` → Wayland is **opt-in,
  non-default**, and inherits the data-control requirement.
  https://github.com/ChurchTao/clipboard-rs
- **`tauri-plugin-clipboard-manager`** 2.3.3: README claims Linux ✓, API is read/write only —
  `writeText, readText, writeHtml, clear` — **no clipboard-change event**. Independent downstream bug
  report frames the runtime reality as "readClipboardText is X11-only" on Wayland (anecdotal):
  https://github.com/earendil-works/pi/issues/7248
- **`wl-clipboard-rs`** (YaLTeR) — the only crate with first-class Wayland change-watching: "intended
  to be used by terminal applications, clipboard managers and other utilities which don't spawn
  Wayland surfaces"; requires `ext-data-control` or `wlr-data-control`; exposes
  `wl_clipboard_rs::watch::Watcher` with `ClipboardEvent::{Changed{mime_types, offer}, Cleared}` and
  a `CancelHandle`: https://github.com/YaLTeR/wl-clipboard-rs
- **Bottom line**: none of them can detect a copy on **GNOME Wayland**; on KDE/wlroots,
  `wl-clipboard-rs` (directly or via `arboard --features wayland-data-control` /
  `clipboard-rs --features wayland`) is the working path.

---

## 3. NON-ACTIVATING POPUP WINDOWS + CURSOR POSITIONING

### macOS

- The OS primitive exists: `NSWindow.StyleMask.nonactivatingPanel` — "The window is a panel or a
  subclass of NSPanel that does not activate the owning app":
  https://developer.apple.com/documentation/appkit/nswindow/stylemask-swift.struct/nonactivatingpanel
- **Tauri does not expose it**: open feature request "[feat] NSPanel type of Window":
  https://github.com/tauri-apps/tauri/issues/13034 (the older
  https://github.com/tauri-apps/tauri/issues/2258 was closed as completed by exposing
  `set_activation_policy`, which hides the Dock icon — trade-off users complain about in
  https://github.com/tauri-apps/tauri/issues/11488, closed as *not planned*).
- Community workaround `tauri-nspanel` swizzles an `NSWindow` into an `NSPanel`:
  https://github.com/ahkohd/tauri-nspanel — but the requester in tauri#13034 reports its limits:
  "I need to click the panel to focus it, and it doesn't prevent focus stealing. This doesn't behave
  like a normal panel."
- `focus: false` / `focused(false)` is not honoured at window creation on macOS (open bug):
  https://github.com/tauri-apps/tauri/issues/9065
- Global cursor position: `NSEvent.mouseLocation` "Reports the current mouse position in screen
  coordinates": https://developer.apple.com/documentation/appkit/nsevent/mouselocation — positioning
  a panel at the cursor is feasible on macOS.
- Known Tauri caveat: an `alwaysOnTop` window misbehaves over other apps' full-screen spaces in
  packaged builds: https://github.com/tauri-apps/tauri/issues/9556

### Windows

- `WS_EX_NOACTIVATE` (0x08000000): "A top-level window created with this style does not become the
  foreground window when the user clicks it... The window does not appear on the taskbar by
  default"; `WS_EX_TOPMOST` (0x00000008): "should be placed above all non-topmost windows and should
  stay above them, even when the window is deactivated":
  https://learn.microsoft.com/en-us/windows/win32/winmsg/extended-window-styles
- `SetWindowPos` with `HWND_TOPMOST` + `SWP_NOACTIVATE` (0x0010: "Does not activate the window") is
  the runtime combination: https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos
- Global cursor position: `GetCursorPos` — "Retrieves the position of the mouse cursor, in screen
  coordinates": https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-getcursorpos
- Tauri side: `WindowConfig` has `always_on_top`, `skip_taskbar`, `focus`, `focusable`, `x`/`y`,
  `transparent`, `decorations`, `no_redirection_bitmap`:
  https://docs.rs/tauri-utils/latest/tauri_utils/config/struct.WindowConfig.html . `focusable` +
  `set_focusable` added in tauri-runtime 2.8.0 (Aug 18 2025):
  https://v2.tauri.app/release/tauri-runtime/v2.8.0/
  - Caveat: Tauri has no config field for `WS_EX_NOACTIVATE`; apply it yourself with
    `SetWindowLongPtr(GWL_EXSTYLE)` on the HWND (Tauri exposes `hwnd()`), then
    `SetWindowPos(..., SWP_NOACTIVATE)`. **Windows is the one platform where full "panel at cursor,
    never steals focus" is achievable.**

### Linux

- X11: positioning works. Tauri's own bug report: on "X11 / Windows / macOS" popup placement is
  "Accurate", while on Wayland an unparented popup "always appears at screen center" (with
  `Gdk-WARNING: Couldn't map as window ... as popup because it doesn't have a parent").
  https://github.com/tauri-apps/tauri/issues/13608
- Wayland: **no global positioning exists.**
  - xdg-shell has no toplevel position request; the only positioning object is `xdg_positioner`, and
    its rules are explicitly relative to a *parent* surface: "a child surface must intersect with or
    be at least partially adjacent to its parent surface"; the anchor rect is "relative to the
    window geometry ... of the parent surface": https://wayland.app/protocols/xdg-shell
  - Tauri confirms the practical effect: "Calling `tauri::Window::set_position` and configuring a
    window with `tauri::Window::always_on_top` does not throw an error or successfully resolve (it
    silently fails/no-ops)... This also seems to include `tauri::WebviewWindowBuilder::position`" —
    open, filed 2026-02-08 against tauri 2.10.2/tao 0.34.5 on Wayland:
    https://github.com/tauri-apps/tauri/issues/14913 . A follow-up comment adds that `skip_taskbar`
    is also non-operational there.
  - Global pointer query: the core protocol only surfaces pointer position through per-surface
    `wl_pointer` events: https://wayland.app/protocols/wayland#wl_pointer
  - **Layer-shell is the Wayland-idiomatic answer** (anchor/margins/`set_keyboard_interactivity`;
    layer surfaces "do not receive keyboard events" by default):
    https://wayland.app/protocols/wlr-layer-shell-unstable-v1 — but GNOME/Mutter does not implement
    it (mutter request closed 2019-12-14 with no implementation:
    https://gitlab.gnome.org/GNOME/mutter/-/work_items/973), and apps still break on GNOME 48 for
    exactly this reason (rofi 2.0 crash: https://github.com/davatorium/rofi/discussions/2230).
  - Focus without stealing: Wayland's mechanism is `xdg-activation` — explicitly best-effort: "The
    token the activating client gets may be ineffective either already at the time it receives it...
    The activating client will have no way to discover the validity of the token":
    https://wayland.app/protocols/xdg-activation-v1
- GTK-level knob: `gtk_window_set_accept_focus()` is only "a hint asking the desktop environment not
  to receive the input focus" — on a compositor that ignores the hint you get focus anyway:
  https://docs.gtk.org/gtk3/method.Window.set_accept_focus.html

---

## 4. SYNTHESIS

**Recommended architecture: make the hotkey the primary trigger and the clipboard watch the
opportunistic upgrade.**

### macOS (high fidelity, one privacy catch)

1. Trigger: `tauri-plugin-global-shortcut` (Carbon hotkey, works natively) OR poll
   `NSPasteboard.changeCount`.
2. Text: read `NSPasteboard.general.string(forType: .string)` after the trigger.
3. Panel: borderless, `alwaysOnTop`, `visibleOnAllWorkspaces`, non-activating — requires `NSPanel` +
   `NSWindowStyleMaskNonactivatingPanel`, so either adopt `tauri-nspanel` and accept its
   focus-stealing limitation or set `set_activation_policy(Accessory)` and lose the Dock icon.
4. Position: `NSEvent.mouseLocation` → convert with `NSScreen.visibleFrame`/backing scale.
5. Catch: on macOS ≥15.4 programmatic clipboard reads trigger the system "allow paste" alert and can
   be denied. Design for "user pressed hotkey"; do not advertise a zero-touch "copy → panel" mode.

### Windows (full fidelity)

1. Trigger: global shortcut (RegisterHotKey) and/or `AddClipboardFormatListener`.
2. Text: read the clipboard on the event; filter for text formats.
3. Panel: `WS_EX_NOACTIVATE | WS_EX_TOPMOST` via `SetWindowLongPtr(GWL_EXSTYLE)` on the Tauri HWND,
   shown with `SetWindowPos(..., HWND_TOPMOST, SWP_NOACTIVATE|SWP_SHOWWINDOW)`, plus Tauri
   `always_on_top`, `skip_taskbar`, `decorations:false`, `transparent`.
4. Position: `GetCursorPos` → `set_position` (works). Add a tray icon for discoverability.

### Linux (two tiers — be honest about GNOME)

- **Tier A — portal desktops (KDE Plasma, GNOME 48+, Hyprland)**:
  1. Trigger: `ashpd::desktop::global_shortcuts`: CreateSession → BindShortcuts once with stable IDs
     and a `preferred_trigger` → listen on `receive_activated`; use the `activation_token` if the
     compositor must raise the panel.
  2. Text: after activation, read the clipboard. On KDE/wlroots/KWin-6.6 this can be done unfocused
     via `ext-data-control` (`wl-clipboard-rs`); **on GNOME there is no data-control**, so you must
     take focus and read via GTK — and you can never *detect* the copy, only read on hotkey.
  3. Panel: always-on-top works only where the compositor honours it; positioning is impossible
     (tauri#14913). Fall back to a centered/top-centered panel on the active monitor.
- **Tier B — no portal (Sway/wlroots today, COSMIC, GNOME <48)**: ship a documented "bind this
  command in your compositor" flow (`bindsym` → `plainly --explain-clipboard`, Hyprland `bind`,
  COSMIC custom shortcuts), i.e. the compositor invokes the CLI, which then reads the clipboard.
- Optional power-user escape hatch (documented, opt-in): evdev listener in the `input` group (works
  on every compositor, no portal).

### Punch list

**HARD BLOCKERS**

1. Linux/Wayland — global hotkey on a compositor with no GlobalShortcuts backend. X11 grabs do not
   see keys typed into native Wayland clients.
2. Linux/Wayland — detecting "the user just copied something" from an unfocused app **on GNOME**:
   Mutter implements neither data-control protocol, and there is no general clipboard portal.
3. Linux/Wayland — positioning a panel at the cursor, or querying global pointer position: no
   toplevel positioning in xdg-shell; Tauri `set_position`/`always_on_top` silently no-op.
4. Linux/Wayland — a true always-on-top non-activating panel with no compositor cooperation; Tauri
   `always_on_top` no-ops on Wayland and layer-shell is unavailable on GNOME.
5. macOS — silent, prompt-free background clipboard reading on macOS ≥15.4.
6. macOS — a genuine non-activating panel *in Tauri core*: no NSPanel support; the community plugin
   still steals focus.

**WORKAROUNDS**

1. Hotkey-first everywhere: the hotkey is the user gesture, so the clipboard read happens on demand.
   On GNOME this is the *only* viable trigger.
2. Compositor keybind → CLI as the universal Linux fallback.
3. evdev/libinput daemon (member of `input` group) for compositor-independent hotkeys on Wayland.
4. GNOME extras: a GNOME Shell extension for the missing window control — but Tauri does not expose
   the Linux window id today, so this needs a Tauri/tao change.
5. Panel placement fallbacks on Wayland: center or top-center on the active monitor, or a persistent
   tray item. **Drop "at the cursor" on Wayland entirely.**
6. macOS: `tauri-nspanel` + `set_activation_policy(Accessory)` where the Dock icon is expendable;
   accept a first-run "allow paste" prompt and expose a Settings deep-link for it.
7. Windows: full behaviour achievable, but you write the Win32 bits yourself on top of Tauri's HWND.
8. Robustness for the portal path: stable shortcut IDs per action, handle `ShortcutsChanged`, keep
   the portal session alive, treat `BindShortcuts` failure as a first-class fallback trigger.

**RECOMMENDED PRODUCT DECISION**: promise "press a hotkey → panel explains the clipboard" as the
cross-platform core; market "copy → panel appears automatically" only on Windows and X11 (+
KDE/wlroots Wayland with data-control), and **feature-detect at runtime rather than in
documentation**.

---

## UNVERIFIED / GAPS

- code.hyprland.org (anti-bot challenge) blocked the Hyprland portal README/`.portal` file — its
  portal details are source-listed but not content-verified.
- www.x.org blocked the XFixes spec page (xorgproto's `xfixesproto.h` used instead).
- GTK4 `focusable` / Wayland mapping not verified.
- macOS `NSScreen.visibleFrame` coordinate details not fetched.
- ANSWERED in section 5 (addendum) below.

---

## 5. ADDENDUM — niri specifically (Tauri layer-shell, window rules, clipboard, IPC)

### 5.1 Tauri + wlr-layer-shell — possible, but only via an unsupported hack

- **The naive attempt fails.** Layer-shell on the tao/webkit `GtkWindow` gives
  `custom_shell_surface_init: assertion '!gtk_widget_get_mapped (GTK_WIDGET (gtk_window))' failed`
  → "GtkWindow is not a layer surface. Make sure you called gtk_layer_init_for_window()". Cause: tao
  maps the GtkWindow before you can reach it. Closed 2024-06-27 with no core support; the reporter
  asked for a "bring your own window" API: https://github.com/tauri-apps/tao/issues/925
- **Working pattern** (posted 2024-10-19, 18 👍): in the Tauri `.setup()` hook — hide the webview
  window, create a **new** `gtk::ApplicationWindow` from the same `gtk::Application`, move the
  webview's `default_vbox()` into it, then call `init_layer_shell()` **before** `show_all()`.
  Working code with `gtk = "0.18.1"` + `gtk-layer-shell = "0.8.1"`:
  https://github.com/tauri-apps/tauri/issues/2083#issuecomment-2424150009
  - GTK3, not GTK4, because Tauri v2 on Linux is webkit2gtk-4.1/GTK3; the GTK4/webkit6 migration is
    still open (https://github.com/tauri-apps/wry/issues/1474). gtk-layer-shell (GTK3) is in
    maintenance mode: https://github.com/wmww/gtk-layer-shell
  - **Prototype-first**: depends on Tauri internals (`default_vbox()`), re-verify on each upgrade.
    Still the only route that keeps one Rust/Tauri app with a real layer-shell surface.
- **Alternatives (all imply a second native panel process)**: GTK4 + gtk4-layer-shell
  (https://github.com/wmww/gtk4-layer-shell); **`iced_layershell`** 0.19.1 (2026-07-12), pure Rust,
  no GTK/WebKit, cleanest if the panel is simple text
  (https://crates.io/api/v1/crates/iced_layershell); or accept an xdg-toplevel + niri window rules
  (5.2).
- You **cannot** convert the tao toplevel into a layer surface after the fact (tao#925).

### 5.2 Toplevel popup on niri — the rules exist

All from https://raw.githubusercontent.com/wiki/niri-wm/niri/Configuration:-Window-Rules.md

- **`open-floating true`** — required, else the window is tiled into the scrolling column layout.
- **`open-focused false`** — prevents automatic focus on open.
- **`default-floating-position x=… y=… relative-to=…`** — initial position; `relative-to` ∈
  top-left/top-right/bottom-left/bottom-right/top/bottom/left/right. The docs give a literal
  dropdown-panel recipe (`open-floating true`,
  `default-floating-position x=0 y=0 relative-to="top"`,
  `default-window-height { proportion 0.5; }`, `default-column-width { proportion 0.8; }`).
  - Limitation: **initial position only** — "Afterward, the window will remember its last floating
    position". No runtime move exists (Tauri `set_position` no-ops: tauri#14913).
- **`on-xdg-activate "ignore"`** — stops focus stealing; the default focuses when the token carries a
  valid serial.
- Appearance/sizing: `default-column-width { fixed N; }`, `default-window-height { fixed N; }`,
  `geometry-corner-radius`, `clip-to-geometry true`, `opacity`, `open-on-output "…"`;
  `match app-id="^plainly-panel$"` is a regex.
- `is-floating` "will apply only after the window is already open", so it cannot drive opening props.
- For the **layer-shell** design, positioning comes from client-side anchors/margins, and by default
  "layer shell surfaces do not receive keyboard events" — exactly the non-activating behaviour
  wanted.

### 5.3 Clipboard on niri — works unfocused, no focus needed

- niri implements `ext-data-control`: a niri selection-event bug states "the ext-data-control-v1
  protocol is being used"; fixed and closed 2025-08-11: https://github.com/niri-wm/niri/issues/1831
- **Requires wl-clipboard ≥ 2.3.0** for ext-data-control (2.2.x only speaks the deprecated wlr
  variant — the thing that broke on KWin 6.6): https://github.com/bugaevc/wl-clipboard/issues/242
  **VERIFIED on the dogfood machine: wl-clipboard 2.3.0, and niri advertises
  `ext_data_control_manager_v1` v1 + `zwlr_data_control_manager_v1` v2.**
- Real-world proof on niri: a live config runs
  `spawn-at-startup "wl-paste" "--type" "text" "--watch" "cliphist" "store"`
  (https://github.com/niri-wm/niri/issues/4520).
- **The focus question — good news**: without data-control, Wayland is focus-based (wl-clipboard pops
  a transparent surface to grab focus, and can hang). **With data-control, focus is not required** —
  "the client will be able to manage the current selection and take the role of a clipboard manager",
  and `selection` events arrive on binding and on every new selection
  (https://wayland.app/protocols/ext-data-control-v1). **So an UNFOCUSED background daemon can
  read/watch the clipboard on niri.** On GNOME this remains impossible (mutter#3941).
- **Crate warning**: do NOT rely on arboard's Wayland path — opt-in backend, X11 fallback, author
  notes it "did not keep the clipboard contents after the process exited", and a downstream report
  says it returns `Ok(())` while writing nothing on non-wlroots compositors like niri (fixed by
  shelling out to `wl-copy`): https://github.com/codewhale-hq/Codewhale/issues/1920 . Use
  **`wl-clipboard-rs` 0.9+** (its `watch::Watcher` yields `Changed{mime_types, offer}` / `Cleared`
  and reports `MissingProtocol` for feature-detection) or shell out to `wl-paste`.

### 5.4 Keyboard trigger + IPC

- niri bind (https://raw.githubusercontent.com/wiki/niri-wm/niri/Configuration:-Key-Bindings.md):
  - `binds { Mod+Shift+E { spawn "plainly" "--explain-clipboard"; } }`
  - `spawn` does **not** use a shell and does not expand env vars or `~`; one quoted arg per
    argument, or `spawn-sh "…"` (since 25.08).
  - `hotkey-overlay-title="Explain clipboard"` for discoverability; `allow-when-locked=true`;
    `repeat=false` / `cooldown-ms=…` to tame key-repeat.
  - Every bindable action is also invocable via `niri msg action`.
  - **Open caveat**: a `spawn` bind fires on key-down, and niri has an open bug where the spawned
    client sometimes never receives the modifier key-up (thinks `Mod` is held) — plan for it if the
    panel is keyboard-interactive: https://github.com/niri-wm/niri/issues/4520
- CLI → daemon IPC, in order:
  1. **Unix domain socket** under `$XDG_RUNTIME_DIR` — idiomatic, user-permissioned, not
     network-reachable. Precedent: niri's own IPC is a UNIX socket at `$NIRI_SOCKET` speaking
     newline-delimited JSON (https://raw.githubusercontent.com/wiki/niri-wm/niri/IPC.md).
  2. **D-Bus session bus** (zbus) — desktop-native; needed for `.desktop` DBusActivatable/portal.
  3. Localhost TCP — avoid; no access control by default.
- **Shortcut worth taking**: `tauri-plugin-single-instance` delivers the second invocation's
  `argv`/`cwd` into the live app — `plainly --explain-clipboard` reaches the running Tauri process
  with **no custom IPC server**:
  https://github.com/tauri-apps/plugins-workspace/blob/v2/plugins/single-instance/README.md
- **Panel-output selection**: niri IPC exposes `FocusedWindow` and outputs
  (`niri msg --json outputs`), so the daemon can pick the right output for
  `get_layer_surface(output)`.

### 5.5 Revised bottom line for the niri dogfood machine

- **Good UX, feasible today**: compositor bind (or `niri msg`) → daemon → clipboard via
  **ext-data-control** (no focus needed) → anchored **layer-shell** panel via the gtk-layer-shell
  reparenting hack (prototype-first) or a small GTK4 / `iced_layershell` panel process.
- **Less effort**: keep a normal Tauri window + `open-floating true`, `open-focused false`,
  `default-floating-position`, `on-xdg-activate "ignore"`.
- **Premise correction**: the blanket "Wayland can't detect a copy" applies to **GNOME/Mutter only**.
  On niri/wlroots/KWin copy-detection *is* available. The true hard blockers on this machine are
  (a) no in-process global hotkey capture on Wayland → compositor bind, (b) no client-side/global
  window positioning → layer-shell anchors or a window rule, (c) Tauri's
  `always_on_top`/`set_position`/`skip_taskbar` silently no-op on Wayland.
  **Verified locally: `zwlr_layer_shell_v1` v5 is advertised, so layer-shell is available on niri.**
