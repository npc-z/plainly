# The rendered shape of an Explanation is a value core owns

`plainly_core::render` hands every surface one rendered shape: a list of `Section`s whose Key Help
body is a list of `GlossView`s — expression, gloss, and an Example that is already either present
(trimmed) or absent (`Option<String>`) — and whose empty sections, where a surface chooses to say
so, are `Body::Note`s. `render::sections(passage, explanation, &EmptyNotes)` is the one entry point;
the markdown document, the raw CSV, the panel's widgets and the desktop card all consume that value
instead of re-judging strings. The desktop's JavaScript renders the sections it is handed and owns
no ordering, emptiness or Example rule.

## Why

- **The visible rule had four copies.** ADR-0002 says "no Example, no Example line"; the check
  `!example.trim().is_empty()` lived in core's markdown renderer, the raw CSV, the panel's Gloss
  widget and the desktop's `app.js`. Four copies happened to agree, but nothing made them, and a
  whitespace-only Example was untested everywhere.
- **The section rules had two and a half implementations.** Core's `sections()` owns the set, the
  order and the empty-section policy: a document omits an empty Grammar or Key Help, a screen read
  in place keeps it and says why in the caller's words. The desktop detail bypassed `render`
  entirely — `app.js` hard-coded the five headings, the order and both empty-state rules, while
  `view.rs` flattened the Explanation into five fields and passed the Example through as a plain
  `String` with a comment handing the judgement to the frontend. It was the only surface whose
  rendering rules lived outside core and outside CI.
- **The cost had already arrived.** Adding the Example field (`832efcb`) changed 15 core files and
  every surface, `app.js` included. The next field or rule change would travel the same way unless
  the shape had an owner.
- **The value is the test surface.** "Present or absent? trimmed how? in what order? and what does
  an empty section hold?" are judgements, and the judgement that had drifted lived in a crate
  `.github` CI does not run. Keeping the shape in core puts the rule under core's tests next to the
  document renderer it must agree with; keeping the desktop a renderer of values puts the wire
  contract under a JSON test.

## Consequences

- **`Gloss.example` stays a `String`.** The provider contract still requires it and the store's
  migration shape does not change; absence is decided once, where the rendered shape is
  constructed (`GlossView::of`), by trimming — whitespace-only is absent, and `Some` carries the
  trimmed sentence.
- **Consumers cannot re-derive the rule.** `Body::Glosses` carries `GlossView`, so the markdown
  document, the CSV cell, the panel's widgets and the desktop card all see an `Option`; reading the
  raw field again would mean ignoring the type they were given.
- **`Body::Note` is a body, not a flag.** "The model said nothing here" is now a different kind of
  body from an Explanation's prose; a document still prints the caller's sentence as text, while a
  screen can keep its muted material. The panel's widget seam can tell the two apart for the first
  time.
- **The read-in-place policy is one function.** `render::sections` with caller-supplied notes
  replaces `render::panel`; the document policy is the same function with `EmptyNotes::silent()`,
  so "which sections exist and in what order" has one implementation and two policies on top.
- **The desktop wire is a DTO, not a second renderer.** `DetailView` carries `sections` (core's
  shape plus the surface's title) and no flat Explanation fields; JavaScript holds materials by
  `data-kind` only. The wire shape lives in `view.rs` because it is an in-repo IPC contract, so
  core's render types stay free of serde.
- **Wording stays with each surface.** The two empty-section sentences are still per-surface copy
  passed into core; the desktop and the panel keep their own, and the rule of when to use them is
  core's.

## Rejected

- **Let each surface keep its own judgement.** That is the state this replaces: four copies of one
  rule and a second implementation of the section rules in JavaScript, one of them outside CI.
- **Change `Gloss.example` to `Option` in the domain.** The wire schema requires a string, the
  store's column shape and migration depend on it, and "the model left it empty" would stop being
  distinguishable from "the Record predates Examples" at the data layer. Large assertion churn for
  a rendering concern.
- **Ship HTML or pre-rendered section text from core to the desktop.** It would move escaping and
  presentation into core and make the JavaScript a consumer of markup it cannot inspect; the
  template literal is the HTML seam by design.
- **Keep `Body::Prose` for the empty-section sentences.** A surface cannot then tell "the model
  said nothing here" from an Explanation's own prose — the panel already cannot, and the desktop
  would lose the muted material its manual checklist pins.
- **Derive `Serialize` on core's render types instead of a DTO.** It looks smaller, but it makes
  core speak the tag vocabulary of an in-repo IPC (`body.type`, `kind` strings). The `view.rs` DTO
  is a mapping, not a judgement, and keeps the contract beside the half that parses it.

## See also

- `.scratch/render-shape/spec.md` — the feature spec.
- `docs/adr/0002-the-example-is-required-but-not-backfilled.md` — the visible rule this decision
  gives an owner.
- `crates/plainly-core/src/render.rs`, `crates/plainly-panel/src/panel.rs`,
  `crates/plainly-desktop/src/view.rs`, `crates/plainly-desktop/ui/app.js` — the four consumers.
