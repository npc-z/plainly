//! What the panel is showing, as a value — the conversation it is having.
//!
//! The panel is a GTK process, so anything that decides what appears on the
//! screen has to be reachable without a compositor if it is to be tested at all.
//! This module is that part: one trigger's outcome, the stack of Passages, the
//! provenance chip, and whether a local model's warning applies. `panel.rs` then
//! owns widgets and nothing else.
//!
//! It also owns what a trigger *means*: [`Trigger::press`] turns a key press or
//! ⟳ into the [`Recall`] the run will use, and [`Trigger::read`] turns what the
//! clipboard read came back with into a [`Showing`]. Both decisions are here,
//! where they are reachable without a display.
//!
//! The shape of a trigger is **the four states the spec names** (loading, result,
//! error, refusal), told apart as data rather than as the absence of a widget:
//!
//! - [`Showing::Loading`]: the clipboard has not been read yet;
//! - [`Showing::Stack`]: Passages are up, each with its own outcome;
//! - and the failures and refusals, which are one sentence and no artifacts.
//!
//! Telling "a refusal" from "a failed run" from "an empty clipboard" is what lets
//! the title bar know whether there is anything to copy, and it is the whole
//! reason the panel's states are not strings.

use std::collections::HashMap;

use plainly_core::clipboard::{Clipboard, Reading};
use plainly_core::split::{self, TooManyChunks};
use plainly_core::{Artifact, RunNotice};

use crate::clipboard;
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
        /// Warnings about this chunk that are not failures: the Explanation is
        /// on screen and can be copied, but something about it must be said
        /// (today: that the history could not keep it, ADR 0003).
        warnings: Vec<String>,
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
    /// The clipboard's owner marked the content secret.
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
/// The spec counts four — loading, result, error, refusal — and the panel's exit
/// paths have to work in all of them. The classification is a fact about
/// [`Showing`] that the tests pin rather than something the widget code branches
/// on: the title bar is above every state unconditionally, which is why there is
/// no `match State` in `panel.rs` for this to feed. It is built only for the
/// tests.
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
    /// stack answered by two providers is not a state a trigger can produce, and a
    /// chip that grew with the number of Passages would stop being readable at a
    /// glance, which is its whole job.
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
/// own widget — and a reason without "nothing was stored" is the sentence every
/// error state ends with.
pub fn failure_sentence(reason: &str) -> String {
    format!("{reason}\n{}", strings::NOTHING_STORED)
}

/// What set a trigger off.
///
/// A value, not the recall it resolves to: the recall for a ⟳ depends on the
/// configuration read at the **start** of that trigger (a panel is reused, so the
/// file may have changed since the last press).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// A key press: re-read the clipboard, reuse what the store holds.
    Refresh,
    /// ⟳: re-read the clipboard and go past the cache.
    ///
    /// It reads the clipboard again rather than re-running what is on screen:
    /// "generate this again" means the Passage the person last copied, and
    /// regenerating the previous run's Passage would answer a question they have
    /// moved on from.
    ///
    /// On a locally hosted model it also changes provider, when there is a cloud
    /// one it can change to: the note under the content tells the person to press
    /// this button for exactly that, so a button that re-ran the same local model
    /// would make the sentence a lie. Which provider that is is decided from the
    /// configuration read at the start of the trigger.
    Regenerate,
}

/// One trigger's run, as the panel accumulates it.
///
/// A value rather than a set of widget handles: the run arrives one Passage at a
/// time, the panel has to be able to say what it would show after any of them,
/// and the tests of that are the tests of a `Trigger`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trigger {
    /// What this trigger is showing.
    ///
    /// Private: the transitions ([`Trigger::read`], [`Trigger::commit`],
    /// [`Trigger::fail`], [`apply`]) are the only writers, so "the state owns the
    /// trigger" is a fact the compiler keeps rather than a comment. The panel
    /// reads it through [`Trigger::showing`].
    showing: Showing,
    /// Whether this run goes past the cache.
    recall: Recall,
    /// The configuration this trigger was started on.
    ///
    /// Carried here rather than read again when 复制 asks for its text: the body
    /// was drawn with *these* switches, and a copy rendered from a newer file would
    /// disagree with the screen it is supposed to be a copy of.
    prefs: Prefs,
}

impl Trigger {
    /// What this trigger is showing, borrowed.
    pub fn showing(&self) -> &Showing {
        &self.showing
    }

    /// The cache/provider instruction this run carries, borrowed.
    pub fn recall(&self) -> &Recall {
        &self.recall
    }

    /// The configuration snapshot the body and 复制 render from, borrowed.
    pub fn prefs(&self) -> &Prefs {
        &self.prefs
    }

    /// The trigger a press starts: Loading, with its recall resolved against the
    /// configuration read at the start of the trigger.
    ///
    /// A key press reuses what the store holds; ⟳ goes past the cache, and on a
    /// local model it names the cloud provider to move to when one is usable
    /// ([`Prefs::regenerate_recall`]).
    pub fn press(press: Press, prefs: Prefs) -> Self {
        let recall = match press {
            Press::Refresh => Recall::Reuse,
            Press::Regenerate => prefs.regenerate_recall(),
        };
        Self::loading(recall, prefs)
    }

    /// A run that has not read the clipboard yet.
    pub fn loading(recall: Recall, prefs: Prefs) -> Self {
        Self {
            showing: Showing::Loading,
            recall,
            prefs,
        }
    }

    /// The Stack state for these Passages, none answered yet.
    fn stacked(passages: &[String]) -> Showing {
        Showing::Stack(
            passages
                .iter()
                .map(|passage| Slot {
                    passage: passage.clone(),
                    outcome: None,
                })
                .collect(),
        )
    }

    /// The Passages are up and no Explanation has arrived.
    ///
    /// Test-only: the panel's own path reaches the Stack through [`Trigger::read`],
    /// which replaces only the `Showing` so the run settings it is carrying cannot
    /// be lost.
    #[cfg(test)]
    fn stack(passages: Vec<String>, recall: Recall, prefs: Prefs) -> Self {
        Self {
            recall,
            prefs,
            showing: Self::stacked(&passages),
        }
    }

    /// Take what the clipboard read came back with into the trigger.
    ///
    /// The judgement is core's ([`Clipboard::reading`], [`split::panel_chunks`]);
    /// this names the outcome so the surface can word each one differently, and so
    /// the rule is reachable from a unit test without a display.
    ///
    /// Returns the Passages the run must explain, or `None` when the trigger
    /// settled into one of the sentence states. What is left for the panel is the
    /// I/O: draw what the state says, and run the Passages it named.
    pub fn read(&mut self, result: Result<Clipboard, clipboard::Error>) -> Option<Vec<String>> {
        let read = match result {
            // An empty clipboard is not an error state, and the sentence it gets
            // is the panel's own: the read module's reason for finding no text is
            // not shown.
            Err(clipboard::Error::NoPassage(_)) => {
                self.showing = Showing::Nothing;
                return None;
            }
            // Anything else is an error, and says so in the panel's own language.
            Err(error) => {
                self.showing = Showing::Unreadable(error.to_string());
                return None;
            }
            Ok(read) => read,
        };

        match read.reading() {
            Reading::Sensitive => {
                self.showing = Showing::Sensitive;
                None
            }
            Reading::Unread => {
                self.showing = Showing::Unread;
                None
            }
            Reading::Passage(passage) => match split::panel_chunks(&passage) {
                Err(too_many) => {
                    self.showing = Showing::TooLong(too_many);
                    None
                }
                Ok(chunks) if chunks.is_empty() => {
                    self.showing = Showing::Nothing;
                    None
                }
                Ok(passages) => {
                    self.showing = Self::stacked(&passages);
                    Some(passages)
                }
            },
        }
    }

    /// One Passage's turn came out.
    fn commit(&mut self, index: usize, outcome: Outcome) {
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
    /// five Passages has to be able to see why there is no Explanation for it.
    fn fail(&mut self, reason: &str) {
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
    /// has to check rather than one they can trust.
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
pub fn apply(trigger: &mut Trigger, event: crate::worker::Event) {
    use crate::worker::{Event, Outcome as Ran};

    match event {
        Event::Chunk { index, outcome } => {
            let outcome = match outcome {
                Ran::Explained {
                    artifact,
                    provenance,
                    warnings,
                } => Outcome::Explained {
                    artifact,
                    provenance,
                    warnings,
                },
                Ran::Failed(reason) => Outcome::Failed(reason),
            };
            trigger.commit(index, outcome);
        }
        // A run-level notice is shown once in the status line, not accumulated
        // into the stack: it is a fact about the machine rather than about any
        // one Passage, and repeating it per chunk is noise.
        Event::Notice(_) => {}
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
                warnings: Vec::new(),
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

    /// A failed run puts the reason under every Passage.
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
    /// an Explanation.
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
                warnings: Vec::new(),
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
                warnings: Vec::new(),
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
                warnings: Vec::new(),
            },
        );

        let text = trigger.copy_text().expect("the second answered");

        assert!(!text.contains("boom"), "{text}");
        assert!(text.contains("### 原文\ntwo"), "{text}");
    }

    /// A chunk can carry a warning without stopping being an answer: the
    /// Explanation is still shown and still copied, and the warning travels with
    /// it so the panel can draw it under the body. This is the store-write
    /// failure case of ADR 0003 — losing the answer would be the one outcome the
    /// decision refuses.
    #[test]
    fn an_explained_passage_carries_its_warning_and_is_still_copied() {
        let (artifact, provenance) = artifact("one", "deepseek", false);
        let mut trigger = Trigger::stack(vec!["one".into()], Recall::Reuse, Prefs::default());
        let warning = strings::not_stored("disk full");

        apply(
            &mut trigger,
            crate::worker::Event::Chunk {
                index: 0,
                outcome: crate::worker::Outcome::Explained {
                    artifact: Box::new(artifact),
                    provenance,
                    warnings: vec![warning.clone()],
                },
            },
        );

        assert!(trigger.showing.has_explanation());
        assert!(
            trigger
                .copy_text()
                .expect("the answer is copyable")
                .contains("### 原文\none")
        );

        match &trigger.showing.slots().next().expect("one slot").outcome {
            Some(Outcome::Explained { warnings, .. }) => {
                assert_eq!(warnings, &[warning]);
            }
            other => panic!("expected an answered chunk, got {other:?}"),
        }
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

    /// 复制 copies what the panel shows, switches included: a copy that obeyed
    /// different switches than the body would bring back a section the person
    /// never saw.
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
                warnings: Vec::new(),
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
                warnings: Vec::new(),
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

/// The earlier turn each Passage repeats, by Lookup Key.
///
/// `None` is the first occurrence, which asks; `Some(earlier)` is a later
/// occurrence that shows that turn's answer without asking again. The whole key
/// matters and only part of it is the Passage, but the rest — Level, prompt
/// version, provider, model, thinking — is fixed for one run, so two equal
/// Passages in one trigger are one question. A `Bypass` turn names a provider for
/// the whole trigger, so the equivalence classes are unchanged even though the
/// plan uses the configured key; a per-chunk provider override would need the
/// actual key here.
pub(crate) fn first_occurrences<K: Clone + Eq + std::hash::Hash>(
    keys: &[K],
) -> Vec<Option<usize>> {
    let mut first: HashMap<K, usize> = HashMap::new();
    keys.iter()
        .enumerate()
        .map(|(index, key)| first.insert(key.clone(), index))
        .collect()
}

/// The run-level notices the panel has not reported yet.
///
/// A pure slice of the runner's list rather than a copy: the panel's status line
/// is the run's own words, in the order the run produced them.
pub(crate) fn unreported(notices: &[RunNotice], shown: usize) -> &[RunNotice] {
    &notices[shown.min(notices.len())..]
}

#[cfg(test)]
mod planning_tests {
    use super::{first_occurrences, unreported};
    use plainly_core::RunNotice;

    /// The panel does not pay twice for one question in a trigger: a repeated
    /// Lookup Key is planned as a repeat of the turn before it, so it shows that
    /// answer instead of asking again.
    #[test]
    fn a_repeated_key_is_planned_after_the_turn_it_repeats() {
        assert_eq!(
            first_occurrences(&["a", "b", "a", "c", "b"]),
            vec![None, None, Some(0), None, Some(1)]
        );
    }

    #[test]
    fn distinct_keys_are_each_asked() {
        assert_eq!(first_occurrences(&["a", "b", "c"]), vec![None, None, None]);
    }

    /// A run-level notice is shown once: after the opening reports the credential
    /// store, a later `Bypass` turn that adds one adds exactly that one.
    #[test]
    fn a_run_notice_is_reported_once_and_only_the_new_one_later() {
        let first = RunNotice::KeyringUnavailable {
            provider: "ollama".to_string(),
            reason: "the keyring is locked".to_string(),
        };
        let later = RunNotice::KeyringUnavailable {
            provider: "deepseek".to_string(),
            reason: "the keyring is locked".to_string(),
        };
        let mut shown = 0;
        let mut notices = vec![first];
        assert_eq!(unreported(&notices, shown).len(), 1);
        shown = notices.len();
        assert!(unreported(&notices, shown).is_empty(), "not said twice");

        notices.push(later);
        let new = unreported(&notices, shown);
        assert_eq!(new.len(), 1, "the later provider's notice is new");
        assert!(matches!(
            &new[0],
            RunNotice::KeyringUnavailable { provider, .. } if provider == "deepseek"
        ));
    }
}

#[cfg(test)]
mod trigger_tests {
    //! What a trigger's transitions mean, without a display.

    use super::*;
    use crate::clipboard;
    use plainly_core::clipboard::{Clipboard, MimeType, PASSWORD_HINT};
    use plainly_core::split::{CHUNK_WORDS, PANEL_CHUNK_LIMIT};

    const PASSAGE: &str = "The committee had already gone to ground.";

    fn clipboard(types: Vec<MimeType>) -> Clipboard {
        Clipboard {
            passage: PASSAGE.to_string(),
            types,
        }
    }

    fn hint(value: Option<&str>) -> MimeType {
        MimeType {
            mime: PASSWORD_HINT.to_string(),
            value: value.map(|value| value.as_bytes().to_vec()),
        }
    }

    /// A paragraph of exactly `count` words, so a test can count Passages.
    fn paragraph(tag: &str, count: usize) -> String {
        (0..count)
            .map(|word| format!("{tag}{word}"))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// One read, and the trigger it left behind.
    fn read(result: Result<Clipboard, clipboard::Error>) -> (Trigger, Option<Vec<String>>) {
        let mut trigger = Trigger::loading(Recall::Reuse, Prefs::default());
        let passages = trigger.read(result);
        (trigger, passages)
    }

    fn loading_slots(passages: &[String]) -> Showing {
        Showing::Stack(
            passages
                .iter()
                .map(|passage| Slot {
                    passage: passage.clone(),
                    outcome: None,
                })
                .collect(),
        )
    }

    #[test]
    fn a_secret_clipboard_settles_as_a_refusal() {
        let (trigger, passages) = read(Ok(clipboard(vec![hint(Some("secret"))])));

        assert_eq!(trigger.showing(), &Showing::Sensitive);
        assert_eq!(passages, None, "a refusal runs nothing");
    }

    #[test]
    fn a_clipboard_whose_marker_was_not_read_settles_as_unread() {
        let (trigger, passages) = read(Ok(clipboard(vec![hint(None)])));

        assert_eq!(trigger.showing(), &Showing::Unread);
        assert_eq!(passages, None);
    }

    #[test]
    fn an_unmarked_clipboard_is_one_passage_to_explain() {
        let (trigger, passages) = read(Ok(clipboard(vec![hint(Some("public"))])));

        assert_eq!(trigger.showing(), &loading_slots(&[PASSAGE.to_string()]));
        assert_eq!(passages, Some(vec![PASSAGE.to_string()]));
    }

    #[test]
    fn an_empty_clipboard_settles_as_nothing() {
        let (trigger, passages) = read(Ok(Clipboard {
            passage: "  \n".to_string(),
            types: vec![],
        }));

        assert_eq!(trigger.showing(), &Showing::Nothing);
        assert_eq!(passages, None);
    }

    /// The panel's hard limit is core's, not a number the surface keeps: over it
    /// the trigger settles as a refusal that carries how far over.
    #[test]
    fn more_than_the_panel_limit_settles_as_too_long() {
        let passage = (0..PANEL_CHUNK_LIMIT + 1)
            .map(|_| paragraph("w", CHUNK_WORDS))
            .collect::<Vec<_>>()
            .join("\n\n");

        let (trigger, passages) = read(Ok(Clipboard {
            passage,
            types: vec![],
        }));

        assert_eq!(
            trigger.showing(),
            &Showing::TooLong(TooManyChunks {
                chunks: PANEL_CHUNK_LIMIT + 1,
                limit: PANEL_CHUNK_LIMIT,
            })
        );
        assert_eq!(passages, None);
    }

    /// Up to the limit the Passages arrive in the clipboard's order, both in the
    /// state and in the list handed to the run.
    #[test]
    fn several_passages_arrive_in_order() {
        let paragraphs: Vec<String> = (0..3)
            .map(|n| paragraph(&format!("p{n}w"), CHUNK_WORDS))
            .collect();
        let passage = paragraphs.join("\n\n");

        let (trigger, passages) = read(Ok(Clipboard {
            passage,
            types: vec![],
        }));

        assert_eq!(trigger.showing(), &loading_slots(&paragraphs));
        assert_eq!(passages, Some(paragraphs));
    }

    /// An empty clipboard and a clipboard whose text cannot be read are told
    /// apart: no text is `Nothing`, a read that failed is an error state with the
    /// read module's reason.
    #[test]
    fn a_clipboard_that_holds_no_text_is_nothing_to_explain() {
        let (trigger, passages) = read(Err(clipboard::Error::NoPassage(
            "the clipboard holds no text".to_string(),
        )));

        assert_eq!(trigger.showing(), &Showing::Nothing);
        assert_eq!(passages, None);
    }

    #[test]
    fn a_clipboard_that_cannot_be_read_is_an_error_state() {
        let (trigger, passages) = read(Err(clipboard::Error::Failed(
            "cannot read the clipboard's type list".to_string(),
        )));

        assert_eq!(
            trigger.showing(),
            &Showing::Unreadable("cannot read the clipboard's type list".to_string())
        );
        assert_eq!(passages, None);
    }

    /// No secrets at all: the provider facts in these tests are about the file.
    fn no_secrets() -> plainly_core::Secrets {
        plainly_core::Secrets::with_stores(plainly_core::EnvSecrets::empty(), None)
    }

    /// A key press reuses what the store holds, and starts Loading.
    #[test]
    fn a_key_press_reuses_what_the_store_holds() {
        let trigger = Trigger::press(Press::Refresh, Prefs::default());

        assert_eq!(trigger.recall, Recall::Reuse);
        assert_eq!(trigger.showing(), &Showing::Loading);
        assert_eq!(trigger.prefs, Prefs::default());
    }

    /// ⟳ on a hosted provider is that provider again, past the cache — even
    /// though a cloud alternative is in the preferences.
    #[test]
    fn regenerate_on_a_hosted_provider_bypasses_the_same_provider() {
        let prefs = Prefs::of(
            &plainly_core::Config::default(),
            &plainly_core::Secrets::with_stores(
                plainly_core::EnvSecrets::from_pairs([(
                    plainly_core::env_var_name("deepseek"),
                    "k",
                )]),
                None,
            ),
        );

        assert!(!prefs.provider.local);
        assert!(
            prefs.cloud.is_some(),
            "a cloud alternative is in reach, so staying put is a decision"
        );
        assert_eq!(
            Trigger::press(Press::Regenerate, prefs).recall,
            Recall::Bypass { provider: None }
        );
    }

    /// ⟳ on a local model with a usable cloud provider names the cloud move: this
    /// is the note under the content's promise, decided where it can be tested.
    #[test]
    fn regenerate_on_a_local_model_moves_to_the_cloud() {
        let prefs = Prefs::of(
            &crate::support::local_config(),
            &plainly_core::Secrets::with_stores(
                plainly_core::EnvSecrets::from_pairs([(
                    plainly_core::env_var_name("deepseek"),
                    "k",
                )]),
                None,
            ),
        );

        assert_eq!(
            Trigger::press(Press::Regenerate, prefs).recall,
            Recall::Bypass {
                provider: Some("deepseek".to_string())
            }
        );
    }

    /// A local model with nothing to move to re-runs the same one, past the cache.
    #[test]
    fn regenerate_without_a_cloud_provider_stays_local() {
        let prefs = Prefs::of(&crate::support::local_config(), &no_secrets());

        assert_eq!(prefs.cloud, None);
        assert_eq!(
            Trigger::press(Press::Regenerate, prefs).recall,
            Recall::Bypass { provider: None }
        );
    }

    /// The prefs snapshot is the one read at the start of the trigger, and it
    /// travels with the trigger: the body and 复制 render from it, and resolving
    /// the recall must not drop it.
    #[test]
    fn a_press_carries_the_prefs_it_resolved_against() {
        let prefs = Prefs::of(&crate::support::local_config(), &no_secrets());
        let trigger = Trigger::press(Press::Regenerate, prefs.clone());

        assert_eq!(trigger.prefs, prefs);
    }

    /// The focus wait can install a trigger over one whose run has not finished;
    /// `read` is documented to overwrite whatever state it finds rather than
    /// panic, and this pins that.
    #[test]
    fn a_read_on_a_failed_trigger_overwrites_it() {
        let mut trigger = Trigger::loading(Recall::Reuse, Prefs::default());
        trigger.fail("no provider named \"nope\"");
        assert!(matches!(trigger.showing(), Showing::Failed(_)));

        let passages = trigger.read(Ok(clipboard(vec![hint(Some("secret"))])));

        assert_eq!(trigger.showing(), &Showing::Sensitive);
        assert_eq!(passages, None);
    }

    /// The sequence the panel actually runs: a press builds the Loading trigger,
    /// then the read settles it.
    #[test]
    fn a_press_then_a_read_is_one_trigger() {
        let prefs = Prefs::of(
            &crate::support::local_config(),
            &plainly_core::Secrets::with_stores(
                plainly_core::EnvSecrets::from_pairs([(
                    plainly_core::env_var_name("deepseek"),
                    "k",
                )]),
                None,
            ),
        );
        let mut trigger = Trigger::press(Press::Regenerate, prefs);

        assert_eq!(trigger.showing(), &Showing::Loading);
        let passages = trigger.read(Ok(clipboard(vec![hint(Some("public"))])));

        assert_eq!(passages, Some(vec![PASSAGE.to_string()]));
        assert_eq!(trigger.showing(), &loading_slots(&[PASSAGE.to_string()]));
        assert_eq!(
            trigger.recall,
            Recall::Bypass {
                provider: Some("deepseek".to_string())
            },
            "the press's recall survives the read"
        );
    }

    /// A read must not lose the trigger's own run settings: the recall the run
    /// will use and the switches the body and 复制 render from travel with it.
    #[test]
    fn a_read_keeps_the_triggers_recall_and_prefs() {
        let prefs = Prefs::default();
        let recall = Recall::Bypass {
            provider: Some("deepseek".to_string()),
        };
        let mut trigger = Trigger::loading(recall.clone(), prefs.clone());

        assert_eq!(
            trigger.read(Ok(clipboard(vec![hint(Some("public"))]))),
            Some(vec![PASSAGE.to_string()])
        );

        assert_eq!(trigger.recall, recall);
        assert_eq!(trigger.prefs, prefs);

        // A sentence state replaces the Showing, not the run settings: an error
        // or a refusal read keeps the recall and the switches it was started on.
        let mut refused = Trigger::loading(recall.clone(), prefs.clone());
        assert_eq!(refused.read(Ok(clipboard(vec![hint(Some("secret"))]))), None);
        assert_eq!(refused.showing(), &Showing::Sensitive);
        assert_eq!(refused.recall, recall);
        assert_eq!(refused.prefs, prefs);
    }
}
