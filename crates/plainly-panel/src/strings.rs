//! The panel's copy, in one place.
//!
//! The interface is Chinese while the Explanation stays English (spec §12), so
//! none of the domain's words live here. There is deliberately no i18n
//! framework: one module is the difference between "translatable later" and
//! "rewrite later". Ticket 15 finishes the set this ticket starts.

use plainly_core::render::SectionKind;
use plainly_core::split::TooManyChunks;

/// What the panel calls itself.
pub const TITLE: &str = "Plainly";

/// Before the clipboard has been read. Reading is local and quick, so this is
/// barely visible — but it is what the panel shows if it is.
pub const READING: &str = "正在读取剪贴板…";

/// While the Explanation is being generated.
pub const EXPLAINING: &str = "正在解释…";

/// The one way out. `keyboard_mode=NONE` means Esc does nothing (spec §12).
pub const CLOSE: &str = "✕";

/// The clipboard is marked secret. The reason is named, and so is the fact that
/// nothing left the machine.
pub const SENSITIVE: &str = "这段内容被密码管理器标记为敏感：没有发送，也没有保存。";

/// The marker was published and its value could not be read, so the run cannot
/// tell. The asymmetry decides it, and the wording says which half is missing.
pub const UNREAD: &str = "这段内容带有敏感标记，但标记的值读不出来：没有发送，也没有保存。";

/// There is nothing on the clipboard Plainly can read as a Passage.
pub const NOTHING: &str = "剪贴板里没有可以解释的 Passage。";

/// What every error state ends with: the Passage was only ever read.
pub const NOTHING_STORED: &str = "剪贴板内容未变，什么都没存。";

/// A clipboard that could not be read at all. The reader's own words follow as a
/// diagnostic — they name the session problem better than a generic sentence —
/// and the state still ends the way every error state does.
pub fn unreadable(reason: &str) -> String {
    format!("读不到剪贴板：{reason}\n{NOTHING_STORED}")
}

/// More Passages than one panel request may carry. Ticket 15 words the way out;
/// the limit itself is [`plainly_core::split::PANEL_CHUNK_LIMIT`].
pub fn too_long(too_many: &TooManyChunks) -> String {
    format!(
        "这段内容有 {} 段，超过面板一次能解释的 {} 段：请用 CLI（plainly explain）或先拆分。",
        too_many.chunks, too_many.limit
    )
}

/// The Grammar section's body when the model found no structural blocker: the
/// panel keeps the section so "nothing here" cannot read as "nothing was
/// generated" (spec §4).
pub const GRAMMAR_NOT_NEEDED: &str = "本段不需要";

/// A section's heading, in the panel's language.
///
/// [`SectionKind::title`] is the *markdown rendering's* heading and stays
/// English; the panel words its own, which is how an interface can be Chinese
/// while the Explanation is not.
pub fn section(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Original => "原文",
        SectionKind::ComprehensibleEnglish => "改写",
        SectionKind::KeyHelp => "关键词",
        SectionKind::Grammar => "语法",
        SectionKind::Translation => "翻译",
    }
}
