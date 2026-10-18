//! The panel's copy, in one place.
//!
//! The interface is Chinese while the Explanation stays English, so none of the
//! domain's words live here. There is deliberately no i18n framework: one module
//! is the difference between "translatable later" and "rewrite later", and a
//! translator can be handed this file alone.
//!
//! The states a panel can be in are all here rather than in `panel.rs`: what it
//! says while it works, what it says when it refuses, and what a refusal offers
//! as a way out. Every message that is not a plain constant takes the facts it
//! needs as arguments, so the module has no other dependency than core's own
//! names for things.

use plainly_core::{RunNotice, TurnNotice};
use plainly_core::render::SectionKind;
use plainly_core::split::TooManyChunks;
use plainly_core::{CredentialError, RunnerError};

/// What the panel calls itself.
pub const TITLE: &str = "Plainly";

/// Before the clipboard has been read. Reading is local and quick, so this is
/// barely visible.
pub const READING: &str = "正在读取剪贴板…";

pub const EXPLAINING: &str = "正在解释…";

/// The one way out. `keyboard_mode=NONE` means Esc does nothing, so the panel
/// must give itself a close button that works in every state.
pub const CLOSE: &str = "✕";

/// The close button's accessible name: `✕` is a glyph, and a screen reader
/// reading it aloud is not a way to close anything.
pub const CLOSE_LABEL: &str = "关闭面板";

/// Copy this Explanation's own text. The panel is read once, in
/// place, so the text that is worth keeping is the text that is in front of the
/// person rather than the whole five-section document.
pub const COPY: &str = "复制";

/// The copy button's accessible name and its tooltip: what "复制" copies, said
/// before the click rather than after it.
pub const COPY_LABEL: &str = "复制上面的解释";

/// What the header says once 复制 has copied something. There is no place in a
/// 470px layer surface for a toast, so the **status line** carries it, and a
/// "copied" that copied nothing is worse than an error.
pub const COPIED: &str = "已复制";

/// The same line when the clipboard would not take the text. The reader's own
/// words are not useful here — the failure is local, and the reason is a protocol
/// or a store — so this is one short phrase, and the line empties on the next
/// trigger.
pub const COPY_FAILED: &str = "复制失败";

/// ⤢ could not start the main window at all: the program is neither on `PATH` nor
/// beside this panel.
///
/// The status line shows the name of the missing program rather than a generic
/// sentence, because that name is the whole of what a person can act on. It is the
/// panel's copy, not the process's, so it lives here with the rest.
pub fn window_not_found(program: impl std::fmt::Display) -> String {
    format!("找不到 {program}：它不在 PATH 上，也不和这个面板放在一起")
}

/// `plainly-desktop` was found and would not start: no permission, no display,
/// a binary for the wrong machine. The system's own words follow the colon, the
/// way every other failure in Plainly carries the reason it was given — and this
/// is interface copy like [`window_not_found`], because the panel's status line
/// shows it verbatim.
pub fn window_would_not_start(
    program: impl std::fmt::Display,
    reason: impl std::fmt::Display,
) -> String {
    format!("起不来 {program}：{reason}")
}

/// Generate this Passage again, **past the cache**: the point of pressing it is
/// that the stored answer is not the one wanted, either because a local model got
/// it wrong or because the person wants to see what else the model can say.
pub const REGENERATE: &str = "⟳";

pub const REGENERATE_LABEL: &str = "重新生成（不用缓存）";

/// Open the main window, on the history screen. It goes through
/// `plainly-desktop`'s single-instance handover, so a window that is already
/// open comes forward instead of a second one appearing.
pub const OPEN_WINDOW: &str = "⤢";

pub const OPEN_WINDOW_LABEL: &str = "打开主窗口（历史）";

/// The clipboard is marked secret. The reason is named, and so the fact that
/// nothing left the machine.
///
/// The way out is the trigger itself — copy something else and press the key
/// again — and it is **said**, because this is the one state where the panel has
/// nothing else to offer: there is no "explain anyway" to press. A refusal that
/// only refused would be a wall, so this one's way out is a re-trigger.
pub const SENSITIVE: &str =
    "这段内容被密码管理器标记为敏感：没有发送，也没有保存。\n换个内容再按一次键即可。";

/// The marker was published and its value could not be read, so the run cannot
/// tell. The asymmetry decides it, and the wording says which half is missing —
/// as does the way out, which is the same re-trigger [`SENSITIVE`] names.
pub const UNREAD: &str =
    "这段内容带有敏感标记，但标记的值读不出来：没有发送，也没有保存。\n换个内容再按一次键即可。";

/// There is nothing on the clipboard Plainly can read as a Passage.
pub const NOTHING: &str = "剪贴板里没有可以解释的 Passage。";

/// The fallback reader had no display to ask for the clipboard at all: the
/// session it was reading through has gone. Said as a reason rather than a panic,
/// because the panel still has a ✕ to offer and a read is all that failed.
pub const NO_DISPLAY: &str = "显示连接已经断开";

/// What every error state ends with: the Passage was only ever read.
pub const NOTHING_STORED: &str = "剪贴板内容未变，什么都没存。";

/// A clipboard that could not be read at all. The reader's own words follow as a
/// diagnostic — they name the session problem better than a generic sentence —
/// and the state still ends the way every error state does.
pub fn unreadable(reason: &str) -> String {
    format!("读不到剪贴板：{reason}\n{NOTHING_STORED}")
}

/// More Passages than one panel request may carry. The reason and the way out are
/// in one sentence: the CLI takes any length, and the limit itself is
/// [`plainly_core::split::PANEL_CHUNK_LIMIT`]'s, never a number this surface
/// keeps.
pub fn too_long(too_many: &TooManyChunks) -> String {
    format!(
        "这段太长：{} 段，超过面板一次能解释的 {} 段。\n用 CLI 一次解释完（plainly explain），或先拆成几段再按一次键。",
        too_many.chunks, too_many.limit
    )
}

/// The panel's sentence for a run that could not open at all.
///
/// A credential failure is worded here rather than in core (ADR 0003): core
/// states the fact — which provider, which variable, or the store's reason — and
/// the panel says it in Chinese. Only the credential states get interface wording.
/// The other `RunnerError` variants carry core diagnostics (a path, a SQLite
/// message, a setup problem) and are shown as-is: they are not sentences a person
/// acts on in Chinese, and inventing one here would hide the diagnostic that names
/// the fix.
pub fn runner_error(error: &RunnerError) -> String {
    match error {
        RunnerError::Credential(CredentialError::Missing { provider, .. }) => missing_key(provider),
        RunnerError::Credential(CredentialError::Unreadable { provider, source }) => {
            credential_unreadable(provider, &source.to_string())
        }
        _ => error.to_string(),
    }
}

/// A required provider's key is not set anywhere. Said as "set one" rather than
/// copying a command: the person is looking at the panel, and a CLI invocation
/// here would be the wrong surface's wording (ADR 0003).
pub fn missing_key(provider: &str) -> String {
    format!("{provider} 需要 API key，但还没有设置：先设置一个，再重新触发。")
}

/// The credential store would not answer for a provider that needs a key. Not
/// "no key": the person may well have set one, and sending them to set another
/// is the wrong fix.
pub fn credential_unreadable(provider: &str, reason: &str) -> String {
    format!("读不到 {provider} 的凭据存储：{reason}；这次没法运行。")
}

/// The history could not be read, so no stored answer could be found and the
/// chunk fails rather than paying for a generation the store could not rule out.
pub fn history_unreadable(reason: impl std::fmt::Display) -> String {
    format!("读不到历史记录：{reason}")
}

/// The panel's sentence for one run-level core notice.
///
/// Core hands over the fact, not the wording (ADR 0003). The run-level half is
/// about the credential store; the turn-level half is [`turn_notice`], a
/// different fact that sends a person somewhere else.
pub fn run_notice(notice: &RunNotice) -> String {
    match notice {
        RunNotice::KeyringUnavailable { provider, reason } => credential_notice(provider, reason),
    }
}

/// The panel's sentence for one turn-level core notice.
pub fn turn_notice(notice: &TurnNotice) -> String {
    match notice {
        TurnNotice::NotStored { reason } => not_stored(reason),
    }
}

/// A run-level fact shown once in the status line: a provider's credential store
/// could not answer, so this run continues without a key.
///
/// "读不到" rather than "没有": the person may well have set a key, and telling
/// them to set one they already set sends them to the wrong place.
pub fn credential_notice(provider: &str, reason: &str) -> String {
    format!("读不到 {provider} 的凭据存储：{reason}；这次运行不带 key 继续。")
}

/// The warning under a Passage whose Explanation was produced but not saved.
///
/// The answer stays on screen because it has already been paid for. No claim is
/// made about the next trigger: a failed fresh turn will ask again, but a failed
/// regeneration leaves the older Record in place, so the next key press reuses
/// that old answer instead.
pub fn not_stored(reason: &str) -> String {
    format!("这个答案没有存进历史：{reason}。")
}

/// A `Bypass` turn named a provider that could not be used, and the chunk failed
/// because of it. The provider is named because it is the one ⟳ moved to, and
/// the reason says whether its setup or its credential was the problem.
pub fn provider_unavailable(provider: &str, reason: impl std::fmt::Display) -> String {
    format!("{provider} 这次用不了：{reason}")
}

/// Under the content when a model on this machine is answering and there is a
/// cloud provider ⟳ can move to. One fixed line rather than a per-Passage note:
/// the fact is about the provider, not about the paragraph, and a warning repeated
/// under every chunk would read as part of the Explanation.
pub const LOCAL_NOTE: &str =
    "本地模型；它的习语解释可能自信地错——请对照上方原文，或点 ⟳ 换成云端重新生成。";

/// The same warning when there is **no** cloud provider to move to.
///
/// The sentence is the button's instructions: without a hosted provider and a key
/// for it, ⟳ regenerates with the same local model, and telling the person to
/// "换成云端" would send them looking for a setting they do not have.
pub const LOCAL_NOTE_NO_CLOUD: &str =
    "本地模型；它的习语解释可能自信地错——请对照上方原文；⟳ 会用同一个本地模型再生成一次。";

/// The Grammar section's body when the model found no structural blocker: the
/// panel keeps the section so "nothing here" cannot read as "nothing was
/// generated".
pub const GRAMMAR_NOT_NEEDED: &str = "本段不需要";

/// The Key Help section's body when the model found no Gloss. Its own sentence,
/// not the Grammar note's: two empty sections repeating one sentence would make
/// one of them look like the other's mistake, and the two facts are different
/// ones anyway.
pub const KEY_HELP_NOT_NEEDED: &str = "本段没有需要解释的表达";

/// A section's heading, in the panel's language.
///
/// [`SectionKind::title`] is the *markdown rendering's* heading and stays
/// English; the panel words its own, which is how an interface can be Chinese
/// while the Explanation is not.
pub fn section(kind: SectionKind) -> &'static str {
    match kind {
        SectionKind::Original => "原文",
        SectionKind::ComprehensibleEnglish => "改写",
        SectionKind::KeyHelp => "关键帮助",
        SectionKind::Grammar => "语法",
        SectionKind::Translation => "翻译",
    }
}

/// Which provider answered, and whether it is on this machine.
///
/// Two facts in one line because they are read together: the chip exists so a
/// person can judge how much to trust the Explanation, and "本地" is the half of
/// that judgement which changes what the text is worth.
pub fn provenance(provider: &str, model: &str, local: bool) -> String {
    let where_it_runs = match local {
        true => LOCAL_BADGE,
        false => CLOUD_BADGE,
    };
    format!("{provider} · {model} · {where_it_runs}")
}

pub const LOCAL_BADGE: &str = "本地";

/// The badge a model reached over the network carries. Named rather than left
/// blank: "本地" being absent is not the same fact as "云端" being present when
/// the person is deciding whether a vendor stands behind the answer.
pub const CLOUD_BADGE: &str = "云端";

/// Before any Explanation has arrived, the chip says what is happening rather
/// than showing a provider that has not answered yet.
pub const PROVENANCE_PENDING: &str = "正在生成 ↗";

#[cfg(test)]
mod tests {
    use super::*;
    use plainly_core::split::PANEL_CHUNK_LIMIT;

    #[test]
    fn a_local_model_and_a_cloud_one_are_told_apart_in_the_chip() {
        assert_eq!(
            provenance("ollama", "llama3.2", true),
            "ollama · llama3.2 · 本地"
        );
        assert_eq!(
            provenance("deepseek", "deepseek-chat", false),
            "deepseek · deepseek-chat · 云端"
        );
    }

    /// The two notices say different things because they send a person to
    /// different places: one is "the store cannot answer", the other is "the
    /// answer is not in the history". Neither may be mistaken for "no key".
    #[test]
    fn the_notice_sentences_name_the_store_and_the_unsaved_answer() {
        let credential = credential_notice("ollama", "the keyring is locked");
        assert!(credential.contains("ollama"), "{credential}");
        assert!(credential.contains("凭据"), "{credential}");
        assert!(credential.contains("读不到"), "{credential}");

        let unsaved = not_stored("disk full");
        assert!(unsaved.contains("没有存进历史"), "{unsaved}");
        assert!(unsaved.contains("disk full"), "{unsaved}");
    }

    /// The two empty sections are different facts, and a person who sees both in
    /// one Explanation has to be able to tell which is which.
    #[test]
    fn the_two_empty_sections_do_not_say_the_same_thing() {
        assert_ne!(GRAMMAR_NOT_NEEDED, KEY_HELP_NOT_NEEDED);
        assert_eq!(GRAMMAR_NOT_NEEDED, "本段不需要");
    }

    /// The refusal names its reason and its way out. Both halves are in the
    /// sentence because a refusal a person cannot act on is a wall.
    #[test]
    fn the_too_long_refusal_names_the_reason_and_the_way_out() {
        let message = too_long(&TooManyChunks {
            chunks: PANEL_CHUNK_LIMIT + 3,
            limit: PANEL_CHUNK_LIMIT,
        });

        assert!(
            message.contains(&(PANEL_CHUNK_LIMIT + 3).to_string()),
            "how far over it is: {message}"
        );
        assert!(
            message.contains(&PANEL_CHUNK_LIMIT.to_string()),
            "the limit itself: {message}"
        );
        assert!(message.contains("plainly explain"), "{message}");
        assert!(message.contains("拆"), "the manual way out: {message}");
    }

    /// The sensitive refusal's way out is not an action button: it is the trigger
    /// the person already knows, and the sentence says what it rests on.
    #[test]
    fn the_sensitive_refusal_says_nothing_left_the_machine() {
        assert!(SENSITIVE.contains("没有发送"), "{SENSITIVE}");
        assert!(SENSITIVE.contains("没有保存"), "{SENSITIVE}");
    }

    /// The sensitive refusal has nothing to press, so its way out is the trigger
    /// the person already knows, said out loud.
    #[test]
    fn both_sensitive_refusals_name_their_way_out() {
        for message in [SENSITIVE, UNREAD] {
            assert!(message.contains("再按一次键"), "{message}");
        }
    }
}
