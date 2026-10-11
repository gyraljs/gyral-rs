# Revision and examples

Use for substantial revision, a difficult editorial tradeoff, or a final review. This workflow is an original synthesis of the book's principles. Scale it to the request rather than running every step on every sentence.

## Diagnose before changing

Determine whether the main problem is the substance, reader assumptions, organization, reference tracking, sentence structure, word choice, or conventions. Fix the earliest consequential failure: polishing the wording of a missing premise cannot make the argument valid.

For a rewrite, make a small internal record of the claim, evidence, uncertainty, intended reader, and distinctive voice features. Use it to detect meaning drift. A light edit should not become a structural rewrite without a task-specific reason and appropriate user direction.

An effective order for a substantial revision is:

1. Check what is being claimed and supported.
2. Repair missing context or knowledge.
3. Make the topic, point, and progression recoverable.
4. Check references, attribution, and logical connections.
5. Repair grouping, information order, scope, and parallelism.
6. Refine diction, rhythm, and conventions.
7. Compare the result with the original meaning, voice, and brief.

This order is a starting point, not a prescribed number of passes. Stop when the task is satisfied. A final pass should not erase effective character merely because further edits are possible.

## Example: preserve voice while clarifying the referent

Original personal essay:

> “I love a clever system. I love it less at 2 a.m., when it has locked me out of my own work. That's why I started keeping a copy on my laptop.”

Suppose the preceding paragraph mentions several systems. The problem is the ambiguous *it*, not the colloquial rhythm.

Possible revision, if the login service is the intended referent:

> “I love a clever system. I love it less at 2 a.m., when the login service has locked me out of my own work. That's why I started keeping a copy on my laptop.”

Keep the repeated opening, humor, time, and first-person perspective. Do not replace the passage with “Local storage provides resilience against authentication outages” unless the user asks for a technical reformulation. Both versions can express related ideas, but only one preserves this essay's voice.

## Example: repair the mechanism before trimming

Supplied fictional facts: the service saves a request, its confirmation is lost, the client retries, and the service treats the retry as new.

Draft:

> “Confirmation failure creates a data integrity issue through insufficient idempotency.”

Revision for a mixed audience:

> “The service can save the same request twice. It saves the first request, but the client never receives the confirmation and sends it again. The service treats the repeat as a new request.”

The revision exposes the missing step: the first save succeeded despite the lost confirmation. If discussing the remedy, explain the relevant repeat behavior before introducing or relying on *idempotency*. Do not invent an implementation or claim that all duplicate scenarios are solved.

## Example: restructure without rewriting the author's position

Supplied draft order:

1. Three paragraphs on how the team selected a form builder.
2. A description of incomplete submissions.
3. A recommendation to shorten the application form.
4. Results from a small internal trial.

For a decision memo, a useful order may be the recommendation, the submission problem, the trial evidence and its limits, then the implementation context. For a reflective essay about tool selection, the original chronology might serve the purpose.

The category determines whether the original structure is a problem. Do not move a conclusion to the first line merely because it is conventional in another genre.

## Example: preserve uncertainty during compression

Supplied evidence: five employees completed a prototype task more quickly; no customer testing has taken place.

Overstated revision:

> “The new design helps customers finish faster.”

Faithful revision:

> “Five employees completed the task faster with the prototype. We have not tested it with customers.”

If a shorter sentence is needed, retain the key boundary:

> “The prototype was faster in a five-person internal trial.”

Check whether the omitted customer boundary remains clear from context. A shorter statement can still imply more than the evidence supports.

## Example: keep practical instructions practical

User asks for a two-step procedure, and supplies the approved steps:

> “Open Settings. Select Export.”

Do not add a scene, rhetorical hook, or explanation of the history of exporting. If the instructions need an expected result and one has not been supplied, determine whether it is necessary and verifiable. Do not invent the output format.

## Example: clarify a comparison without changing its data

Draft:

> “Option A has a success rate of 80 percent. Option B fails on 10 percent of requests.”

When the denominator and success/failure categories are comparable and exhaustive:

> “Option A succeeds on 80 percent of requests; option B succeeds on 90 percent.”

If pending or canceled requests form additional categories, that conversion may be invalid. Check before turning stylistic consistency into an arithmetic assertion.

## Example: critique rather than rewrite

User asks why a paragraph is hard to follow. Identify the reader's specific difficulty, cite a short portion of the draft, and show a representative repair. Explain what the repair changes and whether it depends on an assumption.

Do not respond with an unsolicited full rewrite that makes it hard to see the diagnosis. Conversely, when the user requests a rewrite, deliver one rather than only listing problems.

## Review questions that reveal material failures

- Can the intended reader identify the topic and point?
- Does the reader have the context needed to interpret each new idea?
- Are recurring people, objects, and concepts identifiable?
- Does each important claim connect to evidence or a stated premise?
- Does the order serve this category and purpose?
- Are modifiers and negatives clear in scope and focus?
- Does a long phrase interrupt an unresolved grammatical dependency?
- Have any factual boundaries, numbers, commitments, or attributions changed?
- Does the result still sound like the intended author?
- Have examples, imagery, and added transitions introduced unsupported facts?

In user-facing notes, prioritize consequential findings. Do not expose this entire checklist unless it is requested or useful for teaching.

## Reader feedback and editorial restraint

Ask for a paraphrase or observed task result when real reader feedback is available. “What would you do next?” often reveals more than “Is this clear?” Keep findings distinct from proposed repairs.

A reader saying “I lost track of who approved this” identifies a comprehension problem. Their suggestion to delete the approval discussion is only one possible solution. Naming the approver may be enough.

When no real reader test is available, describe your work as an editorial review. Do not claim audience validation. Preserve a successful sentence when changing it would merely exchange one acceptable preference for another.
