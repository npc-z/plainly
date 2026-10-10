//! What the panel is showing, as a value — the conversation it is having.
//!
//! The panel is a GTK process, so anything that decides what appears on the
//! screen has to be reachable without a compositor if it is to be tested at all.
//! This module is that part: one trigger's outcome, the stack of Passages, the
//! provenance chip, and whether a local model's warning applies. `panel.rs` then
//! owns widgets and nothing else — it draws what this says and asks it what an
//! action means.
//!
//! The shape of a trigger is **the four states the spec names** (loading, result,
//! error, refusal), told apart as data rather than as the absence of a widget:
//!
//! - [`Showing::Loading`] — the clipboard has not been read yet;
//! - [`Showing::Stack`] — Passages are up, each with its own outcome;
//! - and the failures and refusals, which are one sentence and no artifacts.
//!
//! Telling "a refusal" from "a failed run" from "an empty clipboard" is what lets
//! the title bar know whether there is anything to copy, and it is the whole
//! reason the panel's states are not strings.

use plainly_core::Artifact;
use plainly_core::split::TooManyChunks;

use crate::document;
use crate::prefs::Prefs;
use crate::provenance::Provenance;
use crate::strings;
use crate::worker::Recall;

/// One Passage's place in the stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub passage: String,
    /// `None` until the run reports on this Passage.
    pub outcome: Option<Outcome>,
}

/// What came of one Passage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Explained {
        artifact: Box<Artifact>,
        provenance: Provenance,
    },
    Failed(String),
}

impl Outcome {
    pub fn artifact(&self) -> Option<&Artifact> {
        match self {
            Outcome::Explained { artifact, .. } => Some(artifact),
            Outcome::Failed(_) => None,
        }
    }
}

/// A refusal, or a wait, or a failure: the states that are one sentence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Showing {
    /// Before the clipboard has been read.
    Loading,
    /// The Passages, in the order the clipboard held them.
    Stack(Vec<Slot>),
    /// The clipboard's owner marked the content secret (spec §11).
    Sensitive,
    /// The marker exists and its value could not be read.
    Unread,
    /// There is nothing on the clipboard to explain.
    Nothing,
    /// More Passages than one panel request may carry.
    TooLong(TooManyChunks),
    /// The clipboard could not be read at all.
    Unreadable(String),
    /// The run could not start, or every Passage failed with the same reason.
    Failed(String),
}

/// Which state the panel is in, as one of the four the spec names.
///
/// The spec counts four — loading, result, error, refusal (spec §12) — and the
/// panel's exit paths have to work in all of them. The classification is a fact
/// about [`Showing`] that the tests pin rather than something the widget code
/// branches on: the title bar is above every state unconditionally, which is why
/// there is no `match State` in `panel.rs` for this to feed. It is built only for
/// the tests rather than left as an unused door in the binary.
#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Loading,
    Result,
    Error,
    Refusal,
}

impl Showing {
    #[cfg(test)]
    pub fn state(&self) -> State {
        match self {
            Showing::Loading => State::Loading,
            Showing::Stack(_) => State::Result,
            Showing::Sensitive | Showing::Unread | Showing::Nothing | Showing::TooLong(_) => {
                State::Refusal
            }
            Showing::Unreadable(_) | Showing::Failed(_) => State::Error,
        }
    }

    /// The sentence this state says, when it says one.
    ///
    /// `Stack` has no sentence of its own: each of its Passages does.
    pub fn message(&self) -> Option<String> {
        match self {
            Showing::Loading => Some(strings::READING.to_string()),
            Showing::Stack(_) => None,
            Showing::Sensitive => Some(strings::SENSITIVE.to_string()),
            Showing::Unread => Some(strings::UNREAD.to_string()),
            Showing::Nothing => Some(strings::NOTHING.to_string()),
            Showing::TooLong(too_many) => Some(strings::too_long(too_many)),
            Showing::Unreadable(reason) => Some(strings::unreadable(reason)),
            Showing::Failed(reason) => Some(failure_sentence(reason)),
        }
    }

    /// Whether this state has an Explanation in it that could be copied.
    ///
    /// **Built only for the tests**, like [`State`] above: the copy button asks
    /// [`Trigger::copy_text`] for the text itself, and a `None` is the answer to
    /// both questions. The two ways of asking are what the tests pin.
    #[cfg(test)]
    pub fn has_explanation(&self) -> bool {
        !self.explained().is_empty()
    }

    /// Every answered Explanation in the state, in the order it is shown.
    pub fn explained(&self) -> Vec<&Artifact> {
        self.slots()
            .filter_map(|slot| slot.outcome.as_ref().and_then(Outcome::artifact))
            .collect()
    }

    /// The provenance of the first answered Passage: what the chip says.
    ///
    /// The first rather than a list: one trigger uses one provider profile, so a
    /// stack answered by two providers is not a state a trigger can produce — and
    /// a chip that grew with the number of Passages would stop being readable at
    /// a glance, which is its whole job (spec §12).
    pub fn provenance(&self) -> Option<&Provenance> {
        self.slots().find_map(|slot| match &slot.outcome {
            Some(Outcome::Explained { provenance, .. }) => Some(provenance),
            _ => None,
        })
    }

    /// The chip's text.
    pub fn chip(&self) -> String {
        match self.provenance() {
            Some(provenance) => provenance.text(),
            None => strings::PROVENANCE_PENDING.to_string(),
        }
    }

    /// Every slot, borrowed.
    pub fn slots(&self) -> impl Iterator<Item = &Slot> {
        match self {
            Showing::Stack(slots) => slots.iter(),
            _ => [].iter(),
        }
    }
}

/// What a failed Passage says under its original.
///
/// One function because two surfaces draw it — the sentence states through
/// [`Showing::message`], and a failed Passage inside a stack through the panel's
/// own widget — and a reason without "nothing was stored" is the sentence the spec
/// requires every error state to end with (spec §12).
pub fn failure_sentence(reason: &str) -> String {
    format!("{reason}\n{}", strings::NOTHING_STORED)
}

/// One trigger's run, as the panel accumulates it.
///
/// A value rather than a set of widget handles: the run arrives one Passage at a
/// time, the panel has to be able to say what it would show after any of them,
/// and the tests of that are the tests of a `Trigger`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    pub showing: Showing,
    /// Whether this run goes past the cache.
    pub recall: Recall,
    /// The configuration this trigger was started on.
    ///
    /// Carried here rather than read again when 复制 asks for its text: the body
    /// was drawn with *these* switches, and a copy rendered from a newer file would
    /// disagree with the screen it is supposed to be a copy of. That is the whole
    /// promise of 复制, and it is why this travels with the state rather than
    /// living beside it.
    pub prefs: Prefs,
}

impl Trigger {
    /// A run that has not read the clipboard yet.
    pub fn loading(recall: Recall, prefs: Prefs) -> Self {
        Self {
            showing: Showing::Loading,
            recall,
            prefs,
        }
    }

    /// The Passages are up and no Explanation has arrived.
    pub fn stack(passages: Vec<String>, recall: Recall, prefs: Prefs) -> Self {
        Self {
            recall,
            prefs,
            showing: Showing::Stack(
                passages
                    .into_iter()
                    .map(|passage| Slot {
                        passage,
                        outcome: None,
                    })
                    .collect(),
            ),
        }
    }

    /// One Passage's turn came out.
    pub fn commit(&mut self, index: usize, outcome: Outcome) {
        if let Showing::Stack(slots) = &mut self.showing
            && let Some(slot) = slots.get_mut(index)
        {
            slot.outcome = Some(outcome);
        }
    }

    /// The run failed before it could report on a single Passage.
    ///
    /// Every Passage gets the same reason rather than one of them getting it:
    /// each keeps its original on screen, and a person looking at the fourth of
    /// five Passages has to be able to see why there is no Explanation for it
    /// (spec §12).
    pub fn fail(&mut self, reason: &str) {
        if let Showing::Stack(slots) = &mut self.showing {
            for slot in slots {
                slot.outcome = Some(Outcome::Failed(reason.to_string()));
            }
        } else {
            self.showing = Showing::Failed(reason.to_string());
        }
    }

    /// The panel's text for the copy button, or `None` when there is nothing to
    /// copy.
    ///
    /// Everything that came out, in the order it is shown: the panel is one
    /// surface, and a copy that silently took half of it would be a copy the person
    /// has to check rather than one they can trust (tickets/15).
    ///
    /// It renders from [`Trigger::prefs`] and takes no argument, which is the
    /// point: `app.show_*` decides what the panel draws, 复制 copies **what is on
    /// screen**, and a rendering that asked the caller for the switches again would
    /// be a second opinion about which sections exist.
    pub fn copy_text(&self) -> Option<String> {
        let explained = self.showing.explained();
        if explained.is_empty() {
            return None;
        }
        let prefs = &self.prefs;

        // `None` when the rendering is blank: with every `app.show_*` off there is
        // nothing the person can see, so there is nothing to copy — and a 复制 that
        // says 已复制 while putting whitespace on the clipboard is the kind of
        // success this button exists not to report.
        let text = document::markdown_many(&explained, prefs);
        let text = text.trim();
        (!text.is_empty()).then(|| text.to_string())
    }
}

/// Take one thing the run reported into the trigger.
///
/// The worker's own event type stops here: nothing above this line has to know
/// how a run talks, and nothing below it has to know how the panel stacks
/// Passages. It is the one place the two are put together, so a new event cannot
/// be added to the run without the panel being made to say what it does with it.
///
pub fn apply(trigger: &mut Trigger, event: crate::worker::Event) {
    use crate::worker::{Event, Outcome as Ran};

    match event {
        Event::Chunk { index, outcome } => {
            let outcome = match outcome {
                Ran::Explained {
                    artifact,
                    provenance,
                } => Outcome::Explained {
                    artifact,
                    provenance,
                },
                Ran::Failed(reason) => Outcome::Failed(reason),
            };
            trigger.commit(index, outcome);
        }
        Event::Failed(reason) => trigger.fail(&reason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provenance::Provenance;
    use plainly_core::split::PANEL_CHUNK_LIMIT;

    /// The shipped configuration with some `app.show_*` switches changed, which is
    /// how every test about those switches spells its fixture.
    fn prefs_with(change: impl FnOnce(&mut plainly_core::App)) -> Prefs {
        let mut app = plainly_core::App::default();
        change(&mut app);

        Prefs::of(
            &plainly_core::Config {
                app,
                ..plainly_core::Config::default()
            },
            &plainly_core::Secrets::with_stores(plainly_core::EnvSecrets::empty(), None),
        )
    }

    /// The shared Artifact plus the provenance of this run.
    fn artifact(passage: &str, provider: &str, local: bool) -> (Artifact, Provenance) {
        let artifact = crate::support::artifact(passage, provider, "m");
        let provenance = Provenance::of(&artifact, local);
        (artifact, provenance)
    }

    #[test]
    fn the_four_states_the_spec_names_are_told_apart() {
        assert_eq!(Showing::Loading.state(), State::Loading);
        assert_eq!(Showing::Stack(vec![]).state(), State::Result);
        assert_eq!(Showing::Sensitive.state(), State::Refusal);
        assert_eq!(
            Showing::TooLong(TooManyChunks {
                chunks: 6,
                limit: 5
            })
            .state(),
            State::Refusal
        );
        assert_eq!(Showing::Unreadable("x".into()).state(), State::Error);
        assert_eq!(Showing::Failed("x".into()).state(), State::Error);
    }

    /// Every state that is a sentence says one, and the one that is a stack does
    /// not: its Passages each say their own.
    #[test]
    fn every_sentence_state_has_a_sentence() {
        let states = [
            Showing::Loading,
            Showing::Sensitive,
            Showing::Unread,
            Showing::Nothing,
            Showing::TooLong(TooManyChunks {
                chunks: 6,
                limit: 5,
            }),
            Showing::Unreadable("cannot read".into()),
            Showing::Failed("http 500".into()),
        ];

        for state in states {
            assert!(
                state.message().is_some_and(|message| !message.is_empty()),
                "{state:?} says nothing"
            );
        }
        assert_eq!(Showing::Stack(vec![]).message(), None);
    }

    #[test]
    fn a_trigger_shows_a_slot_per_passage_in_order() {
        let trigger = Trigger::stack(
            vec!["one".into(), "two".into()],
            Recall::Reuse,
            Prefs::default(),
        );

        let passages: Vec<&str> = trigger
            .showing
            .slots()
            .map(|slot| slot.passage.as_str())
            .collect();
        assert_eq!(passages, ["one", "two"]);
    }

    #[test]
    fn a_commit_lands_in_the_slot_it_names() {
        let mut trigger = Trigger::stack(
            vec!["one".into(), "two".into()],
            Recall::Reuse,
            Prefs::default(),
        );
        let (artifact, provenance) = artifact("two", "deepseek", false);

        trigger.commit(
            1,
            Outcome::Explained {
                artifact: Box::new(artifact),
                provenance,
            },
        );

        let slots: Vec<Option<&Artifact>> = trigger
            .showing
            .slots()
            .map(|slot| slot.outcome.as_ref().and_then(Outcome::artifact))
            .collect();
        assert!(slots[0].is_none(), "the first has not arrived");
        assert_eq!(slots[1].expect("the second landed").passage, "two");
    }

    /// A run that could not start leaves every Passage with the reason, because
    /// every Passage is still on screen and every one of them is unexplained.
    #[test]
    fn a_failed_run_says_why_under_every_passage() {
        let mut trigger = Trigger::stack(
            vec!["one".into(), "two".into()],
            Recall::Reuse,
            Prefs::default(),
        );

        trigger.fail("no provider named \"nope\"");

        for slot in trigger.showing.slots() {
            assert_eq!(
                slot.outcome,
                Some(Outcome::Failed("no provider named \"nope\"".to_string()))
            );
        }
        assert_eq!(trigger.showing.state(), State::Result);
        assert!(!trigger.showing.has_explanation());
    }

    /// A run that could not start is every Passage's failure, because every
    /// Passage on screen is still showing its original and none of them will get
    /// an Explanation (spec §12).
    #[test]
    fn a_failed_whole_panel_run_fails_every_passage() {
        let mut trigger = Trigger::stack(
            vec!["one".into(), "two".into()],
            Recall::Reuse,
            Prefs::default(),
        );

        apply(
            &mut trigger,
            crate::worker::Event::Failed("no key".to_string()),
        );

        let failed = trigger
            .showing
            .slots()
            .all(|slot| slot.outcome == Some(Outcome::Failed("no key".to_string())));
        assert!(failed, "every Passage carries the reason");
    }

    #[test]
    fn the_chip_is_pending_until_an_answer_lands() {
        let mut trigger = Trigger::stack(vec!["one".into()], Recall::Reuse, Prefs::default());
        assert_eq!(trigger.showing.chip(), strings::PROVENANCE_PENDING);
        assert_eq!(trigger.showing.provenance(), None);

        let (artifact, provenance) = artifact("one", "ollama", true);
        trigger.commit(
            0,
            Outcome::Explained {
                artifact: Box::new(artifact),
                provenance,
            },
        );

        assert_eq!(trigger.showing.chip(), "ollama · m · 本地");
    }

    #[test]
    fn a_refusal_has_nothing_to_copy_and_a_result_does() {
        assert!(!Showing::Sensitive.has_explanation());
        assert_eq!(Showing::Sensitive.explained().len(), 0);

        let mut trigger = Trigger::stack(vec!["one".into()], Recall::Reuse, Prefs::default());
        assert_eq!(trigger.copy_text(), None, "nothing has answered yet");

        let (artifact, provenance) = artifact("one", "deepseek", false);
        trigger.commit(
            0,
            Outcome::Explained {
                artifact: Box::new(artifact),
                provenance,
            },
        );

        let text = trigger.copy_text().expect("there is something to copy");
        assert!(text.contains("### 原文\none"), "{text}");
    }

    #[test]
    fn a_failed_passage_is_not_copied_and_does_not_stop_the_others() {
        let mut trigger = Trigger::stack(
            vec!["one".into(), "two".into()],
            Recall::Reuse,
            Prefs::default(),
        );
        trigger.commit(0, Outcome::Failed("boom".to_string()));
        let (artifact, provenance) = artifact("two", "deepseek", false);
        trigger.commit(
            1,
            Outcome::Explained {
                artifact: Box::new(artifact),
                provenance,
            },
        );

        let text = trigger.copy_text().expect("the second answered");

        assert!(!text.contains("boom"), "{text}");
        assert!(text.contains("### 原文\ntwo"), "{text}");
    }

    /// The two triggers carry what they mean, and a bypass can name the provider
    /// it moves to: that is the whole difference between a key press and ⟳, and it
    /// has to survive the fallback path's focus wait.
    #[test]
    fn a_trigger_carries_its_recall() {
        assert_eq!(
            Trigger::loading(Recall::Reuse, Prefs::default()).recall,
            Recall::Reuse,
            "a key press reuses what the store holds"
        );
        assert_eq!(
            Trigger::loading(Recall::Bypass { provider: None }, Prefs::default()).recall,
            Recall::Bypass { provider: None }
        );
        assert_eq!(
            Trigger::loading(
                Recall::Bypass {
                    provider: Some("deepseek".to_string())
                },
                Prefs::default()
            )
            .recall,
            Recall::Bypass {
                provider: Some("deepseek".to_string())
            },
            "the cloud move's provider travels with the trigger"
        );
    }

    /// The limit in a refusal is core's number, not a second one kept here.
    #[test]
    fn a_too_long_refusal_carries_the_limit_the_splitter_applied() {
        let trigger = Trigger {
            showing: Showing::TooLong(TooManyChunks {
                chunks: PANEL_CHUNK_LIMIT + 2,
                limit: PANEL_CHUNK_LIMIT,
            }),
            recall: Recall::Reuse,
            prefs: Prefs::default(),
        };

        let message = trigger.showing.message().expect("it says why");
        assert!(
            message.contains(&PANEL_CHUNK_LIMIT.to_string()),
            "{message}"
        );
    }

    /// 复制 copies what the panel shows, switches included. This is the bug the
    /// first version had: the body obeyed `app.show_*` and the clipboard did not,
    /// so a hidden section came back in the pasted text.
    #[test]
    fn the_copied_text_obeys_the_section_switches_too() {
        let (artifact, provenance) = artifact("one", "deepseek", false);
        // The prefs travel with the trigger, which is the point: 复制 renders from
        // the switches the body was drawn with.
        let mut trigger = Trigger::stack(
            vec!["one".into()],
            Recall::Reuse,
            prefs_with(|app| {
                app.show_translation = false;
                app.show_grammar = false;
            }),
        );
        trigger.commit(
            0,
            Outcome::Explained {
                artifact: Box::new(artifact),
                provenance,
            },
        );

        let text = trigger.copy_text().expect("there is something to copy");

        assert!(!text.contains("### 翻译"), "{text}");
        assert!(!text.contains("### 语法"), "{text}");
        assert!(text.contains("### 原文"), "{text}");
    }

    /// Every switch off means there is nothing the person can see, so there is
    /// nothing to copy — not an empty string that would still report 已复制.
    #[test]
    fn a_copy_with_every_section_switched_off_is_nothing_to_copy() {
        let (artifact, provenance) = artifact("one", "deepseek", false);
        let mut trigger = Trigger::stack(
            vec!["one".into()],
            Recall::Reuse,
            prefs_with(|app| {
                app.show_original = false;
                app.show_comprehensible = false;
                app.show_glosses = false;
                app.show_grammar = false;
                app.show_translation = false;
            }),
        );
        trigger.commit(
            0,
            Outcome::Explained {
                artifact: Box::new(artifact),
                provenance,
            },
        );

        assert_eq!(trigger.copy_text(), None);
    }

    /// The section switches are the person's, and the renderer is still core's:
    /// switching one off removes exactly that section.
    #[test]
    fn the_section_switches_remove_only_what_they_name() {
        let (artifact, _) = artifact("one", "deepseek", false);
        let prefs = Prefs::of(
            &plainly_core::Config {
                app: plainly_core::App {
                    show_grammar: false,
                    show_translation: false,
                    ..plainly_core::App::default()
                },
                ..plainly_core::Config::default()
            },
            &plainly_core::Secrets::with_stores(plainly_core::EnvSecrets::empty(), None),
        );

        let kinds: Vec<plainly_core::render::SectionKind> =
            crate::document::sections(&artifact, &prefs)
                .into_iter()
                .map(|section| section.kind)
                .collect();

        assert!(!kinds.contains(&plainly_core::render::SectionKind::Grammar));
        assert!(!kinds.contains(&plainly_core::render::SectionKind::Translation));
        assert!(kinds.contains(&plainly_core::render::SectionKind::Original));
        assert!(kinds.contains(&plainly_core::render::SectionKind::ComprehensibleEnglish));
    }
}
