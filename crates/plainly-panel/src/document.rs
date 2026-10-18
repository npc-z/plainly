//! What an Explanation looks like on the panel: its sections, and its text.
//!
//! Two jobs with one rule between them. [`sections`] is the only filter of
//! `app.show_*` in the crate, and it is what the body draws from; [`markdown`] is
//! the same list rendered as text for 复制. Both go through
//! [`plainly_core::render::sections`] — the section list, the order, and the two
//! "nothing here" sentences are the renderer's, and a second renderer in the panel
//! would be a second set of rules to keep in step (tickets/15, ADR-0005).
//!
//! They live together because they must agree: 复制 copies what is on screen, and
//! the surest way for two renderings to disagree is for them to be built in two
//! modules with two ideas of which sections exist.
//!
//! The passage is included, though the panel also shows it: a copied Explanation
//! that did not carry the text it explains would leave the reader without the
//! authority the Explanation is checked against (spec §12).

use plainly_core::Artifact;
use plainly_core::render;

use crate::prefs::Prefs;
use crate::strings;

/// The sections of one Explanation, with the person's own switches applied.
///
/// `app.show_*` has to mean something here for the settings screen to be telling
/// the truth about it (tickets/09 wrote the switches; tickets/15 made them work).
/// The empty-key-help decision is core's and is reached through
/// [`plainly_core::render::sections`]; this only removes what the person switched
/// off, so there is still exactly one renderer.
///
/// It lives in this module rather than beside the panel's state because both
/// surfaces use it — the body the widget code draws and the text 复制 puts on the
/// clipboard — and a leaf module is what keeps "the copy shows what the panel
/// shows" from becoming a cycle between the two.
pub fn sections(artifact: &Artifact, prefs: &Prefs) -> Vec<render::Section> {
    render::sections(
        &artifact.passage,
        &artifact.explanation,
        &render::EmptyNotes {
            grammar: Some(strings::GRAMMAR_NOT_NEEDED),
            key_help: Some(strings::KEY_HELP_NOT_NEEDED),
        },
    )
    .into_iter()
    .filter(|section| prefs.shows(section.kind))
    .collect()
}

/// The markdown of one answered Passage, with the person's section switches
/// applied.
///
/// Through [`sections`] rather than calling the renderer directly, so the copied
/// text contains exactly what the panel is showing. That is the whole promise of
/// 复制 — you copy what you are looking at — and it is also what keeps the two
/// surfaces from disagreeing about `app.show_*`: a copy that carried a section the
/// panel hides would be text the person never read.
pub fn markdown(artifact: &Artifact, prefs: &Prefs) -> String {
    render::render_with(crate::strings::section, sections(artifact, prefs))
        // The document form ends in one newline; text on a clipboard does not need
        // one, and a trailing blank line pasted into an editor is noise.
        .trim_end()
        .to_string()
}

/// Several answered Passages as one text, in the order they were explained.
///
/// Joined with a blank line and nothing else: the sections are already separated,
/// and a rule between Passages would be a distinction the panel itself does not
/// draw — the stack is one panel's worth of content, not a document with parts.
pub fn markdown_many(artifacts: &[&Artifact], prefs: &Prefs) -> String {
    artifacts
        .iter()
        .map(|artifact| markdown(artifact, prefs))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support;
    use plainly_core::Gloss;

    /// The shipped switches: every section shown, which is what an unedited
    /// configuration means.
    fn prefs() -> Prefs {
        Prefs::default()
    }

    /// The fixture Passage and its Explanation, with the two empty sections
    /// spelled the way the tests care about.
    fn artifact(grammar: Option<&str>, glosses: Vec<Gloss>) -> Artifact {
        let mut artifact = support::artifact(
            "The manager had gone to ground.",
            "deepseek",
            "deepseek-chat",
        );
        artifact.explanation = support::explanation(
            "The manager had hidden.",
            glosses,
            grammar,
            "经理已经躲了起来。",
        );
        artifact
    }

    /// The copied text is the panel's own words, in the panel's own order, and it
    /// carries the Passage it explains.
    #[test]
    fn the_copy_is_the_panels_own_sections_in_its_own_language() {
        let artifact = artifact(
            None,
            vec![Gloss {
                expression: "go to ground".to_string(),
                gloss: "hide so nobody can find you".to_string(),
                example: "The manager went to ground for a week.".to_string(),
            }],
        );

        let text = markdown(&artifact, &prefs());

        assert!(
            text.starts_with("### 原文\nThe manager had gone to ground."),
            "{text}"
        );
        assert!(text.contains("### 改写\nThe manager had hidden."), "{text}");
        assert!(
            text.contains(
                "### 关键帮助\n- `go to ground`\n  hide so nobody can find you\n\n  > The manager went to ground for a week."
            ),
            "{text}"
        );
        assert!(text.contains("### 语法\n本段不需要"), "{text}");
        assert!(text.ends_with("### 翻译\n经理已经躲了起来。"), "{text}");
        assert!(!text.contains("### Original"), "not the export's headings");
    }

    /// Both empty sections keep a sentence in the copy as well as on the panel:
    /// text a person keeps should not lose the difference between "nothing here"
    /// and "nothing was generated".
    #[test]
    fn the_two_empty_sections_are_said_in_the_copied_text_too() {
        let artifact = artifact(None, vec![]);

        let text = markdown(&artifact, &prefs());

        assert!(
            text.contains("### 关键帮助\n本段没有需要解释的表达"),
            "{text}"
        );
        assert!(text.contains("### 语法\n本段不需要"), "{text}");
    }

    #[test]
    fn several_passages_are_copied_in_the_order_they_arrived() {
        let first = artifact(Some("结构说明"), vec![]);
        let second = artifact(None, vec![]);

        let text = markdown_many(&[&first, &second], &prefs());

        let first_at = text.find("结构说明").expect("the first is there");
        let second_at = text.find("本段不需要").expect("the second is there");
        assert!(first_at < second_at, "{text}");
    }
}
