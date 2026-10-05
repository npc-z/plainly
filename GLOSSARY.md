# Plainly

Plainly makes difficult English understandable in English. It takes a Passage and returns an Explanation: a leveled paraphrase, plain-English glosses of what blocks it, an optional grammar note, and a translation — translation is the secondary artifact, never the point.

## Language

### The unit of work

**Passage**:
The stretch of hard English the learner hands over; the unit of work. One Passage yields one Explanation — longer input is split at paragraph boundaries rather than grown into one.
_Avoid_: document, input, text, snippet

**Explanation**:
The result of making one Passage comprehensible: leveled paraphrase, glosses, optional grammar note, and translation. The domain concept; the app's own metadata is attached to it, not part of it.
_Avoid_: artifact, result, output, answer

**Record**:
One row in the history store: an Explanation plus the Passage, Level, Native Language, provider, model, timestamps and versions.
_Avoid_: entry, log, history item

**Artifact**:
The serialized form of an Explanation — bytes on the wire or in the store, not a domain concept. Use it only when the encoding itself is the subject.
_Avoid_: (do not use as a synonym for Explanation)

**Lookup Key**:
The identity of an Explanation: everything that would make the same request — the Passage, the Level, the source and native languages, the prompt version, and the provider profile. Two lookups sharing a key are the same question, so the stored Explanation answers both instead of being generated again.
_Avoid_: cache key, hash, dedupe key

### Inside an Explanation

**Blocker**:
Something in the Passage that blocks this learner's comprehension — an unusual meaning, a phrasal verb, an idiom, a meaning-bearing collocation, or the shape of the sentence itself. Every Blocker is either glossed or explained under Grammar.
_Avoid_: difficult word, unknown term, vocabulary item

**Gloss**:
One plain-English line explaining a single Blocker; never harder than the expression it explains.
_Avoid_: definition, note, annotation

**Comprehensible English**:
The paraphrase: the Passage's meaning restated in more frequent, familiar English at the learner's Level, keeping tense, aspect, modality, negation and logical links.
_Avoid_: simplification, summary, rewrite, plain-English version

**Grammar**:
The section that carries the structural Blockers — inversion, an unclear clause attachment, a construction whose shape is the whole difficulty. Present only when the paraphrase and glosses leave the Passage readable.
_Avoid_: syntax note, grammar lesson

**Translation**:
The Explanation's last section: the Passage's meaning in the learner's Native Language, carrying natural meaning rather than a word-for-word mapping. Always last.
_Avoid_: literal translation, word-for-word

**Level**:
The learner's reading level on the A1–C1+ scale; it sets the vocabulary and how much syntax the Explanation may keep. A setting of the app, not something the model reports.
_Avoid_: difficulty, grade, CEFR score

**Native Language**:
The learner's first language, and the target of the Translation. A setting of the app — never inferred at runtime.
_Avoid_: target language, mother tongue
