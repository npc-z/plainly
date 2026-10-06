//! A provider whose answers are a script.
//!
//! The explain path can be driven through content it did not choose and failures
//! it cannot cause, with no network and no model: a fake that says whatever the
//! test needs, in the order the test needs it.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;

use plainly_core::provider::{ExplainRequest, Provider, ProviderError};

/// Answers requests from a script, and remembers every request it was given.
pub struct FakeProvider {
    script: RefCell<VecDeque<Result<String, ProviderError>>>,
    seen: RefCell<Vec<ExplainRequest>>,
    calls: Cell<usize>,
}

impl FakeProvider {
    /// A provider that answers every request with `content`.
    pub fn saying(content: impl Into<String>) -> Self {
        Self::scripted([Ok(content.into())])
    }

    /// A provider that works through `script` in order.
    pub fn scripted(script: impl IntoIterator<Item = Result<String, ProviderError>>) -> Self {
        Self {
            script: RefCell::new(script.into_iter().collect()),
            seen: RefCell::new(Vec::new()),
            calls: Cell::new(0),
        }
    }

    /// How many requests it has been given.
    pub fn calls(&self) -> usize {
        self.calls.get()
    }

    /// The requests it has been given, in order.
    pub fn requests(&self) -> Vec<ExplainRequest> {
        self.seen.borrow().clone()
    }
}

impl Provider for FakeProvider {
    fn generate(&self, request: &ExplainRequest) -> Result<String, ProviderError> {
        self.calls.set(self.calls.get() + 1);
        self.seen.borrow_mut().push(request.clone());
        self.script
            .borrow_mut()
            .pop_front()
            .expect("the fake provider was asked more times than its script allows")
    }
}
