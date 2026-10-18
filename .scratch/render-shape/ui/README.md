# 预览：详情卡的视觉 parity

`desktop-detail.png` 是真实渲染，不是 mockup：`app.js` 里**真实的** `bodyView()` / `detailView()` / `esc` 用 node 从源文件里抽出来执行，输入一条 wire 形状的 `DetailView`（五节：Original / 可理解英文 / Key Help（一条有例句、一条 `example: null`）/ Grammar 空节 note / Translation），套上**真实的** `crates/plainly-desktop/ui/style.css` 与 `.win / .body / .detail` 外壳，`firefox --headless --window-size=1253,865 --screenshot` 渲染。渲染引擎是 Gecko，实际应用是 WebKitGTK：字体回退可能差一点，其余一致。

重做：

```sh
nix develop -c node .scratch/render-shape/ui/render.js
nix develop -c firefox --headless --window-size=1253,865 \
  --screenshot=.scratch/render-shape/ui/desktop-detail.png \
  file://$PWD/.scratch/render-shape/ui/detail.html
```

对照物是 `.scratch/gloss-examples/ui/desktop-detail.png`（Example 引入时的卡片）：节顺序、标题材质、Original 的纸色块、三行 Gloss、Grammar 空节的斜体、Translation 的颜色与间距一致；差异只在 fixture 文案与之后改过的 strings。取舍见 [../spec.md](../spec.md) §5。
