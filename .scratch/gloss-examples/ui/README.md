# 预览：这一节的两种样子

两轮设计改动的证据，都是真渲染，不是手画的 mockup：

- `desktop-detail.png` — 桌面详情卡，1253×865。
  用 `app.js` 里**真实的** `detailView()`（node 抽出来执行）产出的 HTML，加**真实的** `crates/plainly-desktop/ui/style.css`，
  放进 `.win / .body / .detail` 的外壳里，用 `firefox --headless --window-size=1253,865 --screenshot` 渲染。
  渲染引擎是 Gecko，实际应用是 WebKitGTK：字体回退可能差一点，其它一致。
- `panel-key-help.png` — 面板的「关键帮助」一节，470px（面板的宽）。
  用面板**真实的**控件构造（`panel.rs` 的 `heading` / `glosses_widget`）塞进 `gtk::OffscreenWindow`，
  `show_all()` 之后把主循环跑空再 `pixbuf()` 存 PNG。为此临时加过一个 `#[ignore]` 测试，看完即删；
  要重做就照这几步再加一次（GTK 不能无头初始化，跑的时候需要 display；
  在 `nix develop` 里还要把 `pkg-config --variable=libdir gdk-pixbuf-2.0` 拼进 `LD_LIBRARY_PATH`，
  否则 cargo 的 build script 找不到 `libgdk_pixbuf`）。

设计取舍（为什么是这三种材质）写在 [../spec.md](../spec.md) §6–§8。
