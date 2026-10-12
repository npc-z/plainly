# 05: 手测清单与收尾

**What to build:** 手测清单写上新形状的验收点，全仓测试与 clippy 过，文档指针补上。

**Blocked by:** 02、03、04。

**Spec:** [spec.md](../spec.md) 全部

**Status:** done (2026-10-12)

- [x] `docs/desktop-manual-checks.md`：三行 / 缩进 / 例句斜体变暗 / 条间距 / 旧记录只出两行 / 面板展开后可用。
- [x] `docs/manual-checks.md`：CLI 与导出的三行形状（含 `history export raw` 的单元格）。
- [x] `.scratch/plainly/spec.md` 顶部加一行指针：§3 的 `[{expression, gloss}]`、§4 的一行式、§9 的 `glosses(record_id, ord, expression, gloss)` 由本规格取代。
- [x] `nix develop -c cargo test -p plainly-core -p plainly-cli` + clippy（CI 的两条）。
- [x] `nix develop -c cargo test -p plainly-desktop -p plainly-panel`。
- [ ] 手测：桌面跑一个 stub provider 的 Explanation，看三行与旧记录两种样子。

**Note:** 上面留着一个未勾的步骤是给人做的真机目视项，已经写进 `docs/desktop-manual-checks.md` / `docs/manual-checks.md`，本次没有 GUI 会话去跑。
