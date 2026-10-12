//! The window's copy, in one place.
//!
//! The interface is Chinese while the Explanation stays English (spec §12), so
//! nothing here is a domain word: "Passage" and "Gloss" never appear, and what
//! does appear is what a learner reads. There is deliberately no i18n framework —
//! one module is the difference between "translatable later" and "rewrite
//! later".
//!
//! The frontend asks for these once, as data ([`Strings`]), which is what keeps
//! the copy out of the JavaScript. A string typed into a template is a string
//! nobody can find later; a string that arrives from here is one grep away.

use serde::Serialize;

/// Everything the window says, as one value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Strings {
    pub title: &'static str,
    /// The heading over the one screen that is nothing but a failure: the
    /// window opened but its state did not.
    pub error_prefix_title: &'static str,
    pub tab_history: &'static str,
    pub tab_settings: &'static str,
    pub search_placeholder: &'static str,
    /// The stated limit under the search box (spec §9, §13): a limit written
    /// down is a fact about v0, the same limit discovered is a bug report.
    pub search_scope_note: &'static str,
    pub filter_all: &'static str,
    pub group_today: &'static str,
    pub group_yesterday: &'static str,
    pub group_earlier: &'static str,
    pub empty_history: &'static str,
    pub empty_search: &'static str,
    pub select_a_record: &'static str,
    pub detail_original: &'static str,
    pub detail_comprehensible: &'static str,
    pub detail_glosses: &'static str,
    /// The Key Help section when the model found no blocker. Its own sentence,
    /// not the Grammar note's: two empty sections saying the same thing would
    /// make one of them look like the other's mistake.
    pub glosses_empty: &'static str,
    pub detail_grammar: &'static str,
    pub detail_translation: &'static str,
    pub grammar_not_needed: &'static str,
    pub provenance: &'static str,
    pub prompt_version_note: &'static str,
    pub copy_markdown: &'static str,
    pub export_this: &'static str,
    pub add_tag: &'static str,
    pub tag_placeholder: &'static str,
    pub delete_record: &'static str,
    pub delete_record_confirm: &'static str,
    pub clear_history: &'static str,
    pub clear_history_confirm: &'static str,
    pub copied: &'static str,
    pub thinking_note: &'static str,
    pub local_provider_note: &'static str,
    pub first_run_title: &'static str,
    pub first_run_body: &'static str,
    pub first_run_cloud: &'static str,
    pub first_run_local: &'static str,
    pub saved: &'static str,
    /// What a failure's banner starts with. A field rather than a function,
    /// because the frontend builds the sentence and can only read fields.
    pub error_prefix: &'static str,
    /// The badge a locally hosted model carries in the detail (spec §12).
    pub local_badge: &'static str,
    /// Under the copy-on-popup switch: what turning it on costs, and how the
    /// listener that spends it is started.
    pub copy_auto_note: &'static str,
    /// What the frontend says when it is being viewed outside the window, where
    /// no command — not even this one — can be reached. It is here rather than
    /// written into the JavaScript because the interface's copy has one home.
    pub outside_window: &'static str,
    /// The frontend or a caller asked for an export format Plainly does not
    /// have. A field rather than a function because the command that words it
    /// has one to hand.
    pub unknown_export: &'static str,
}

/// The one value the frontend is handed.
pub const STRINGS: Strings = Strings {
    title: "Plainly",
    error_prefix_title: "打不开 plainly 的数据",
    tab_history: "历史",
    tab_settings: "设置",
    search_placeholder: "搜索英文原文、改写、expression…",
    search_scope_note: "中文翻译与例句暂不可搜",
    filter_all: "全部",
    group_today: "今天",
    group_yesterday: "昨天",
    group_earlier: "更早",
    empty_history: "历史还是空的。按合成器快捷键解释第一段。",
    empty_search: "没有匹配的记录。",
    select_a_record: "从左边选一条记录。",
    detail_original: "原文",
    detail_comprehensible: "改写",
    detail_glosses: "关键帮助",
    glosses_empty: "本段没有需要解释的表达",
    detail_grammar: "语法",
    detail_translation: "翻译",
    grammar_not_needed: "本段不需要",
    provenance: "出处",
    prompt_version_note: "同一段落可能有多个提示词版本的记录——这是可以比对的特性，所以这里显示版本号。",
    copy_markdown: "复制 markdown",
    export_this: "导出这一条",
    add_tag: "加标签",
    tag_placeholder: "标签，回车确认",
    delete_record: "删除",
    delete_record_confirm: "删除这一条？删掉就找不回来了。",
    clear_history: "清除全部历史…",
    clear_history_confirm: "清除全部历史？删掉就找不回来了。",
    copied: "已复制",
    thinking_note: "只在这里可改；面板上不暴露，因为 10 秒在「按键看一眼」里不可用。",
    local_provider_note: "本地模型；它的习语解释可能自信地错——请对照上方原文，或换成云端重新生成。",
    first_run_title: "还没有可用的 provider",
    first_run_body: "plainly 需要至少一个能解释文本的模型。两条路，任选一条：",
    first_run_cloud: "① 用云端（推荐，默认）：DeepSeek，关思考约 1.3 秒，key 存 keyring。",
    first_run_local: "② 用本机模型（离线）：在 127.0.0.1 的常见端口上找 llama.cpp / Ollama / LM Studio。",
    saved: "已保存",
    error_prefix: "出错了：",
    local_badge: "本机",
    copy_auto_note: COPY_AUTO_NOTE,
    outside_window: "这个页面运行在窗口之外：请用 plainly-desktop 打开。",
    unknown_export: "不认识的导出格式：",
};

/// The copy, for the command that hands it over.
pub fn strings() -> Strings {
    STRINGS
}

/// The settings screen's labels and notes, one field per piece of copy.
///
/// The section *data* is [`crate::settings::SettingsView`]; this is the words
/// beside it. They are together in this module so that the interface's copy is
/// one file a translator can be handed (spec §12) — including the sentences that
/// explain why something is the way it is, which are the ones a rewrite usually
/// loses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SettingsStrings {
    pub level: &'static str,
    pub level_note: &'static str,
    pub native_language: &'static str,
    pub native_language_note: &'static str,
    pub overridden: &'static str,
    pub provider_configured: &'static str,
    pub provider_configured_note: &'static str,
    pub provider_local: &'static str,
    pub provider_cloud: &'static str,
    pub model: &'static str,
    pub model_note: &'static str,
    pub thinking: &'static str,
    pub thinking_label: &'static str,
    pub thinking_off: &'static str,
    pub thinking_on: &'static str,
    /// The same two states in one word each, for the detail's provenance line.
    pub thinking_off_short: &'static str,
    pub thinking_on_short: &'static str,
    pub thinking_unsupported: &'static str,
    pub key: &'static str,
    pub key_save: &'static str,
    pub key_delete: &'static str,
    pub key_note: &'static str,
    pub provider_in_use: &'static str,
    pub provenance_created: &'static str,
    pub provenance_generated: &'static str,
    pub provenance_seen: &'static str,
    pub provenance_format: &'static str,
    pub section_titles: [&'static str; 5],
    pub panel_corner_top_left: &'static str,
    pub panel_corner_top_right: &'static str,
    pub panel_corner_bottom_left: &'static str,
    pub panel_corner_bottom_right: &'static str,
    pub panel_display_current: &'static str,
    pub prompt_version: &'static str,
    pub prompt_version_note: &'static str,
    pub prompt_appendix: &'static str,
    pub prompt_appendix_note: &'static str,
    pub prompt_factory: &'static str,
    pub prompt_meanings: &'static str,
    pub prompt_meanings_note: &'static str,
    pub copy_auto: &'static str,
    pub panel_corner: &'static str,
    pub panel_corner_note: &'static str,
    pub panel_display: &'static str,
    pub panel_width: &'static str,
    pub panel_height: &'static str,
    pub pixels: &'static str,
    pub panel_width_note: &'static str,
    pub panel_height_note: &'static str,
    pub panel_autohide: &'static str,
    pub seconds: &'static str,
    pub panel_autohide_note: &'static str,
    pub panel_sections: &'static str,
    pub sections: [&'static str; 5],
    pub export_all: &'static str,
    pub export_all_note: &'static str,
    pub data_dir: &'static str,
    pub open_data_dir: &'static str,
    pub files: &'static str,
    pub files_config: &'static str,
    pub files_history: &'static str,
    pub danger: &'static str,
    pub danger_note: &'static str,
    pub clear_done: &'static str,
    pub remove_tag: &'static str,
}

/// The settings screen's copy.
pub const SETTINGS: SettingsStrings = SettingsStrings {
    section_titles: ["语言与等级", "Provider", "提示词", "行为", "数据"],
    panel_corner_top_left: "左上",
    panel_corner_top_right: "右上",
    panel_corner_bottom_left: "左下",
    panel_corner_bottom_right: "右下",
    panel_display_current: "跟随当前显示器",
    level: "等级",
    level_note: "等级必须带含义显示：实测只发标签时，相邻等级经常产出完全相同的解释。",
    native_language: "母语（翻译语言）",
    native_language_note: "这是插进提示词的语言名，翻译就按它写。",
    overridden: "（你的措辞）",
    provider_configured: "当前 provider",
    provider_configured_note: "换 provider 就换了一个问题：同一段会按新的 provider / 模型重新生成一条记录。",
    provider_local: "（本机）",
    provider_cloud: "（云端）",
    model: "模型",
    model_note: "模型是 Lookup Key 的一部分：换模型等于换一个问题，缓存不会串。",
    thinking: "思考模式",
    thinking_label: "思考",
    thinking_off: "关（默认）— 约 1.3 秒",
    thinking_on: "开 — 10–13 秒，更详尽",
    thinking_off_short: "关",
    thinking_on_short: "开",
    thinking_unsupported: "这个 provider 不接受规范的思考开关，Plainly 不会发这个字段。",
    key: "API key",
    key_save: "保存",
    key_delete: "删除",
    key_note: "环境变量 > keyring > 仅本次会话，绝不落明文文件。",
    provider_in_use: "现在用什么",
    provenance_created: "首次记录",
    provenance_generated: "生成",
    provenance_seen: "最后查看",
    provenance_format: "记录格式",
    prompt_version: "版本",
    prompt_version_note: "出厂提示词随应用更新持续改进；版本是内容的哈希，所以相同提示词仍然只生成一次。",
    prompt_appendix: "你的附录（追加在出厂提示词之后）",
    prompt_appendix_note: "只能加、不能删基础规则——基础是提示词的定义，各人的历史才可比。想抵消某条规则，就在它后面写一条相反的。",
    prompt_factory: "出厂提示词（只读）",
    prompt_meanings: "等级含义",
    prompt_meanings_note: "这些措辞是提示词数据：改一个字，版本就变。",
    copy_auto: "复制后自动弹出面板",
    panel_corner: "面板位置",
    panel_corner_note: "角是分辨率无关的——换分辨率或换显示器不会把面板丢到屏幕外。不做自由拖动。",
    panel_display: "面板跟随哪块显示器",
    panel_width: "面板宽度",
    panel_height: "面板高度上限",
    pixels: "px",
    panel_width_note: "面板是固定宽度的一栏：原文按这个宽度折行，所以它决定一行有多长。下一次按键生效。",
    panel_height_note: "上限而不是固定值——内容短时面板仍然短。内容超过它就在面板里滚动；标题栏与底部说明始终可见。",
    panel_autohide: "面板自动消失",
    seconds: "秒",
    panel_autohide_note: "默认关。打开后面板会在设定秒数后自己消失；关着的时候它一直留着，直到你关掉它（面板不接收键盘，Esc 无效）。",
    panel_sections: "面板上显示哪几节",
    sections: ["原文", "改写", "关键帮助", "语法", "翻译"],
    export_all: "导出全部历史",
    export_all_note: "导出写到数据目录，文件名带时刻；单条导出在详情里。",
    data_dir: "数据目录",
    open_data_dir: "打开数据目录",
    files: "文件",
    files_config: "配置",
    files_history: "历史",
    danger: "危险",
    danger_note: "硬删除，没有撤销。",
    clear_done: "已清除 {removed} 条记录",
    remove_tag: "点击移除",
};

/// Every sentence the Provider section says about a key.
pub fn key_in_environment(variable: &str) -> String {
    format!("{variable} 已设置（环境变量优先于 keyring）")
}

/// A key in the OS keyring: it survives the window. This is both what the
/// Provider section reports as the key's state and what storing one says — the
/// same fact, in the same words, in the two places it is read.
pub const KEY_IN_KEYRING: &str = "已保存在 keyring";

/// A key only this process holds: it does not survive the window.
pub const KEY_IN_SESSION: &str = "只在本次会话的内存里，关掉窗口就没了";

/// Nothing has been found yet, and the provider needs one.
pub const KEY_MISSING: &str = "还没有 key";

/// Nothing has been found, and the provider may not need one.
pub const KEY_OPTIONAL: &str = "这个 provider 可以不需要 key；填了就会发送";

/// The stores could not be asked at all: not "there is no key", and the
/// difference matters because the two send the user to different places.
pub fn key_unknown(reason: &str) -> String {
    format!("无法确认是否已有 key：{reason}")
}

/// What removing a key did, in the two cases that are not a failure.
pub const KEY_CLEARED: &str = "已从 keyring 删除";
pub const KEY_CLEARED_NOTHING: &str = "本来就没有保存过 key";

/// A model without a provider to belong to: a mistake in the asking, and one
/// the interface says in the interface's language.
pub const KEY_NO_PROVIDER: &str = "模型要先有 provider 才能设置";

/// The auto-dismiss duration a person typed, when it is outside the range.
///
/// `0` is refused with the reason spec §12 gives: "off" already has a switch,
/// and two spellings of one thing eventually disagree.
pub fn auto_hide_out_of_range(low: u32, high: u32) -> String {
    format!("自动消失时长要在 {low} 到 {high} 秒之间（关掉用开关，不要用 0）")
}

/// A panel size outside what the number input accepts.
///
/// The label travels with the numbers because the two sizes are refused by the
/// same rule and a sentence that named neither would leave a person looking at two
/// inputs and one complaint.
pub fn panel_size_out_of_range(label: &str, low: u32, high: u32) -> String {
    format!("{label}要在 {low} 到 {high} px 之间")
}

/// What one export did, for the sentence in the banner.
pub fn export_summary(only: Option<i64>, count: usize) -> String {
    match only {
        Some(id) => format!("记录 {id} 已导出（{count} 条）"),
        None => format!("已导出 {count} 条记录"),
    }
}

/// Every write in the settings screen that is refused says what was wrong with
/// it: the store's own sentence travels after the colon, because it names the
/// key or the path, and the interface's sentence is what surrounds it.
pub fn update_failed(reason: &str) -> String {
    format!("这次改动没有写入：{reason}")
}

// A failure with no sentence of its own — a command whose argument made no
// sense, a system call that did not work — is still said in Chinese, because the
// banner is part of the interface. The one prefix that does it is
// [`Strings::error_prefix`], which the frontend prepends to the reason: there is
// no function here, because the sentence is assembled on the side that has the
// reason in hand.

/// "Explain on copy" can work here. What it costs is said underneath this line
/// ([`COPY_AUTO_NOTE`]) rather than in it: the capability and the price are two
/// facts, and one sentence holding both would leave the switch looking available
/// when the process that honours it is not running (spec §12).
pub const COPY_AUTO_AVAILABLE: &str = "这个桌面支持 data-control。";

/// What the switch costs and what starts the listener.
///
/// Under the switch in both directions, because both are true: on a session that
/// has no data-control the switch is greyed and this is the same "off" the note
/// above it describes. The key fact is the third sentence — the listener is a
/// process the user starts, and a switch turned on with nobody listening is on
/// in the file and off in the world. Saying that here is the difference between
/// a switch and a switch that appears to work (spec §12, tickets/14).
pub const COPY_AUTO_NOTE: &str =
    "开启后由 plainly-desktop --watch 监听剪贴板：打开这个开关才引入一个常驻进程，默认路径下没有东西需要常驻。";

/// "Explain on copy" cannot work here. The switch is drawn greyed with this
/// sentence rather than hidden, so that a missing capability is a fact the user
/// knows rather than a feature they think does not exist (spec §12).
pub const COPY_AUTO_UNAVAILABLE: &str = "你的桌面不支持：需要 wlroots 类合成器的 data-control 协议。换成 niri / sway 这类合成器后可用。";

/// `--watch` on a session with no data-control.
///
/// The same fact as [`COPY_AUTO_UNAVAILABLE`], said from the process rather than
/// the settings screen: the switch is greyed there, so the only way to ask for a
/// listener anyway is `--watch` by hand.
pub fn watch_needs_data_control() -> &'static str {
    "plainly-desktop --watch：这个桌面没有 data-control（需要 wlroots 类合成器），监听不会启动。"
}

/// `--watch` with the listener actually started.
///
/// Said out loud on the launch that asked for it: a resident process that
/// started quietly is one a person cannot tell from one that failed, and the
/// whole point of `--watch` is that it is a deliberate, visible cost.
pub const WATCH_STARTED: &str =
    "plainly-desktop --watch：复制自动弹的监听已启动（配置里 copy_auto_popup 打开时才真的弹面板）。";

/// The listener stopping because the switch was turned off.
pub const WATCH_STOPPED_SWITCH_OFF: &str =
    "plainly-desktop --watch：配置里 copy_auto_popup 已关，监听停止。";

/// The listener stopping because a later launch asked for the window instead.
pub const WATCH_STOPPED_WINDOW: &str =
    "plainly-desktop --watch：又启动了一次不监听的主窗口，监听停止。";

/// The clipboard watch ending on its own: the compositor stopped answering, or
/// the session went away.
pub const WATCH_ENDED: &str =
    "plainly-desktop --watch：剪贴板监听结束（合成器不再应答或会话已断开），复制自动弹不再生效。";

/// A clipboard that could not be read at all, with the library's reason.
///
/// The listener has no surface to put this on, so it goes to stderr: a resident
/// process that is not working has to say so somewhere, and a silent one is the
/// one thing it must not be.
pub fn watch_read_failed(reason: &str) -> String {
    format!("plainly-desktop --watch：读不了剪贴板：{reason}")
}

/// `plainly-panel` would not start, from the listener.
///
/// Named, because the two binaries are usually built together and one of them
/// missing is the commonest reason for this.
pub fn watch_panel_would_not_start(error: std::io::Error) -> String {
    format!("plainly-desktop --watch：拉不起 plainly-panel：{error}")
}


/// The settings screen's copy, for the command that hands it over.
pub fn settings_strings() -> SettingsStrings {
    SETTINGS
}

/// What storing a key did.
///
/// The session case is the one that is not a success: the key works for this
/// window and no longer. Core words that warning, and it is in English because
/// it is core's — so the window says what happened and core's reason follows
/// after the colon, the same split every other failure takes.
pub fn key_outcome(stored: &plainly_core::Stored) -> String {
    match stored {
        plainly_core::Stored::Keyring => KEY_IN_KEYRING.to_string(),
        plainly_core::Stored::Session { warning } => format!("{KEY_SESSION_ONLY}：{warning}"),
    }
}

/// The configuration file changed on disk since this window read it. Core words
/// this for a command line — "re-run the command" — and the window says the same
/// fact in its own terms: what it will not do is overwrite the other write.
pub fn settings_changed_on_disk() -> &'static str {
    "配置文件在窗口打开之后被别的程序改过：这次改动没有写入，请先看一眼文件再重试"
}

/// A language that is nothing cannot be the language a translation is written
/// in: the prompt's `{{NATIVE}}` placeholder would expand to nothing.
pub fn native_language_required() -> &'static str {
    "母语不能是空的：它是插进提示词的语言名"
}

/// The first half of the sentence above: the key was taken, but not durably.
pub const KEY_SESSION_ONLY: &str = "没有可用的 keyring，key 只在本次会话里有效";

/// `wl-copy` was not there, or refused.
pub fn clipboard_wl_copy(reason: &str) -> String {
    format!("wl-copy 不可用：{reason}")
}

/// The X11 fallback was not there, or refused.
pub fn clipboard_xclip(reason: &str) -> String {
    format!("xclip 不可用：{reason}")
}

/// The data directory could not be opened by the desktop.
///
/// Shown beside the path itself, which the window names either way: "the desktop
/// would not open it" is an answer a person can act on, and the path is how.
pub fn open_failed(reason: &str) -> String {
    format!("打不开：{reason}")
}

/// The window could not open its own state: the history store or the
/// configuration file. Said in the window rather than on stderr, because the
/// window is the surface the person is looking at — and the reason core gives
/// (which names the file) follows after the colon.
pub fn cannot_open(reason: &str) -> String {
    format!("打不开历史库或配置文件：{reason}")
}

/// The window could not be built or started: no display, a webview that will not
/// come up, a configuration Tauri rejects. It is the failure a person is most
/// likely to meet on a machine that is not set up for a GUI, and it is said in
/// the same words as the rest of the crate's exit paths.
pub fn cannot_start(reason: &str) -> String {
    format!("窗口起不来：{reason}")
}

/// Every export name in the directory is taken: reported rather than probed
/// forever, because a directory with ten thousand exports of one history is not
/// a state to keep looking in.
pub fn export_names_exhausted(directory: &str) -> String {
    format!("{directory} 里这个时刻的导出文件名都用完了，先整理一下再导出")
}

/// One output of the compositor, as the picker names it.
///
/// Numbered rather than named, because the connector name (`eDP-1`) comes from
/// the `xdg-output` protocol and `wl_output`'s own events carry only a make and
/// a model: a name this build cannot read would be a lie told confidently.
pub fn output_label(index: usize) -> String {
    format!("显示器 {}", index + 1)
}

