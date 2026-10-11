# Sense of Style Writing

A reusable AI writing skill for drafting, revising, and critiquing nonfiction with the reader in mind. It applies principles from Steven Pinker's *The Sense of Style* to help an assistant clarify meaning, organize an argument, explain difficult ideas, and preserve the author's voice.

The repository contains a Markdown instruction package and focused reference guides. It has no application server, build step, executable scripts, or package dependencies. An AI host reads the instructions and applies them to the writing task you supply.

**Skill name:** `sense-of-style-writing` · **Maintainer:** [mikezupper](https://github.com/mikezupper) · **Status:** Private repository

This is an independent synthesis with original adaptations and examples. It is not an official or endorsed Steven Pinker product, and the source book is not included.

## Contents

- [What the skill does](#what-the-skill-does)
- [Supported writing categories](#supported-writing-categories)
- [Install and use](#install-and-use)
- [Example prompts](#example-prompts)
- [How the editorial workflow works](#how-the-editorial-workflow-works)
- [Before and after](#before-and-after)
- [Repository guide](#repository-guide)
- [Scope and limitations](#scope-and-limitations)
- [Customization and contributions](#customization-and-contributions)
- [Writing evaluations](#writing-evaluations)
- [Versions and releases](#versions-and-releases)
- [Attribution and licensing](#attribution-and-licensing)
- [Possible next additions](#possible-next-additions)

## What the skill does

The skill starts with three questions: what are we writing, who will read it, and what should readers understand, feel, or do afterward? It uses answers already present in the conversation and asks only when a missing answer would materially affect the result.

Once the brief is clear, it helps the assistant:

- **Find the point.** Identify the central claim, supporting information, and desired reader outcome before polishing sentences.
- **Bridge knowledge gaps.** Notice concepts, prerequisites, and causal steps that an expert author may assume a reader already knows.
- **Improve structure and coherence.** Put information in a useful order and make references, contrasts, and logical connections understandable.
- **Clarify sentences.** Repair ambiguous modifiers, difficult grouping, overloaded syntax, and unclear information order.
- **Preserve voice.** Keep effective diction, rhythm, humor, perspective, and deliberate repetition while removing obstacles to understanding.
- **Protect factual meaning.** Retain quantities, attribution, uncertainty, commitments, and the limits of the evidence.
- **Exercise editorial judgment.** Evaluate passive voice, jargon, hedges, abstraction, and sentence length in context rather than treating them as forbidden constructions.

You can request a **draft**, a **revision**, a **critique**, or a **teaching explanation**. A revision should return revised prose; a critique should explain consequential problems and show representative repairs. The amount of review should fit the task.

## Supported writing categories

| Category | Typical work | Editorial focus |
|---|---|---|
| Explanatory and educational | Articles, tutorials, conceptual guides | Show the mechanism, establish prerequisites, and explain limits. |
| Technical documentation | READMEs, API references, runbooks, procedures | Make information findable and actions precise. |
| Business and decision writing | Proposals, strategy memos, reports | Clarify the decision, evidence, alternatives, and tradeoffs. |
| Personal brand and thought leadership | Essays, newsletters, opinion pieces | Develop a recognizable perspective using supplied experience and evidence. |
| Short-form public writing | Social posts, announcements, captions | Preserve one useful idea within the actual space constraints. |
| Professional correspondence | Emails, introductions, requests, feedback | Make the intent and next step clear while fitting the relationship. |
| Persuasive and marketing writing | Landing pages, product descriptions, sales material | Connect reader needs to supported benefits and an appropriate invitation to act. |
| Research and analytical writing | Literature reviews, evidence summaries, analytical reports | Separate findings, interpretation, uncertainty, and synthesis. |

Hybrid work can combine categories. A technical proposal may lead with a decision and include procedural detail later; an analytical newsletter may preserve a personal voice while maintaining careful attribution. See the full [category guide](references/writing-categories.md).

## Install and use

### Local Codex setup

You need Git, access to this private GitHub repository, and a Codex installation that supports local skills. The SSH clone command below assumes that your GitHub SSH authentication is configured.

Clone the repository into your user skill directory:

```bash
mkdir -p "$HOME/.agents/skills"
git clone git@github.com:mikezupper/the-sense-of-style-writing-skill.git \
  "$HOME/.agents/skills/sense-of-style-writing"
```

If you already have a working checkout, you can link it instead. Replace the source path with its absolute location; use this alternative only if the destination does not already exist:

```bash
mkdir -p "$HOME/.agents/skills"
ln -s /absolute/path/to/the-sense-of-style-writing-skill \
  "$HOME/.agents/skills/sense-of-style-writing"
```

Codex supports user skills in `~/.agents/skills`, repository skills in `.agents/skills`, and symlinked skill folders. It detects changes automatically; restart it if the skill does not appear. These locations and behaviors are documented in the [official skills guide](https://learn.chatgpt.com/docs/build-skills).

Invoke the skill in a Codex conversation:

```text
Use $sense-of-style-writing to revise the draft below.
Category: professional correspondence.
Audience: an existing client.
Outcome: agree on a date for a project review.
Keep my warm, direct voice and preserve all proposed dates.

[Paste draft]
```

The invocation name comes from `SKILL.md`; it differs from the GitHub repository name. The included `agents/openai.yaml` supplies a display name, short description, and default prompt.

### Claude Code and OpenCode

Both hosts use the existing `SKILL.md` and `references/`; separate copies of the editorial instructions are unnecessary. `agents/openai.yaml` is optional OpenAI interface metadata, whereas `.agents/skills/` is an installation location. They serve different purposes.

| Host | Personal installation | Project installation | Invocation |
|---|---|---|---|
| Codex | `~/.agents/skills/sense-of-style-writing/` | `.agents/skills/sense-of-style-writing/` | `$sense-of-style-writing` |
| Claude Code | `~/.claude/skills/sense-of-style-writing/` | `.claude/skills/sense-of-style-writing/` | `/sense-of-style-writing` |
| OpenCode | `~/.agents/skills/sense-of-style-writing/` or `~/.config/opencode/skills/sense-of-style-writing/` | `.agents/skills/sense-of-style-writing/` or `.opencode/skills/sense-of-style-writing/` | Ask the agent to use the `sense-of-style-writing` skill. |

Claude Code supports symlinked skill folders. After the Codex clone above, expose the same checkout to Claude Code with:

```bash
mkdir -p "$HOME/.claude/skills"
ln -s "$HOME/.agents/skills/sense-of-style-writing" \
  "$HOME/.claude/skills/sense-of-style-writing"
```

Use this only if the destination does not already exist. Alternatively, clone directly into `~/.claude/skills/sense-of-style-writing`. See the [Claude Code skill documentation](https://code.claude.com/docs/en/skills).

OpenCode also discovers skills in `.claude/skills` and `~/.claude/skills`. If the skill is already installed in the shared `.agents` location, no additional copy is needed. Keep the containing folder named `sense-of-style-writing` to match its frontmatter. See the [OpenCode skill documentation](https://opencode.ai/docs/skills/).

These instructions were checked against host documentation on September 13, 2026; live behavior has not been tested in Claude Code or OpenCode. Other assistants may use the Markdown directly, but discovery and invocation depend on the host.

Keep the reference directory with the entry point so its relative links remain usable. Ordinary writing tasks do not require a local copy of the book or network access to the references. The AI host itself may require an online connection.

### Update an installed clone

If you used the clone command above:

```bash
git -C "$HOME/.agents/skills/sense-of-style-writing" pull --ff-only
```

For a symlinked installation, update the source checkout. If you have local edits, review and preserve them before updating.

## Example prompts

### Draft an explanation

```text
Use $sense-of-style-writing to draft a 600-word explanation of the
retry behavior described in these notes. Write for product managers
who understand APIs but do not know distributed systems. Help them
understand why duplicate requests occur. Use only the supplied facts.

[Paste notes]
```

### Revise a personal essay

```text
Use $sense-of-style-writing to revise this newsletter for founders.
Keep my first-person voice, dry humor, and central position. Clarify
the argument and restructure where necessary. Do not add experiences
or claims. Briefly explain any major structural changes.

[Paste draft]
```

### Critique a decision memo

```text
Use $sense-of-style-writing to critique this memo for an executive
team deciding whether to fund the proposal. Identify the three most
consequential clarity or reasoning problems and show a representative
repair for each. Do not rewrite the entire memo.

[Paste memo]
```

### Make a light edit

```text
Use $sense-of-style-writing for a light edit of this customer email.
Keep the structure and tone. Fix ambiguity and awkward sentences,
preserve every commitment, and return only the revised email.

[Paste email]
```

## How the editorial workflow works

1. **Establish the brief.** Use the conversation to determine category, audience, outcome, and any material constraints. Ask for missing essentials before drafting.
2. **Consult relevant guidance.** Read the category guide and only the references needed for the problem at hand.
3. **Check meaning and evidence.** Identify the actual claim, its support, and unresolved premises. Separate fact, inference, uncertainty, and recommendation.
4. **Shape the reader's path.** Supply necessary context and choose an order that fits the reader's task.
5. **Revise at the requested depth.** Improve organization and sentences while protecting voice and factual scope.
6. **Deliver and stop.** Return the requested artifact, adding only useful editorial notes or unresolved factual questions. Avoid repeated polishing that erases effective character.

These are editorial priorities, not a fixed number of passes. A short email should receive a proportionate review.

## Before and after

The following original examples illustrate the guidance. They are not measured benchmark results.

### Expose the missing mechanism

Given the fictional facts that a service saves a request, its confirmation is lost, and a retry is treated as new:

**Before**

> Confirmation failure creates a data integrity issue through insufficient idempotency.

**After, for a mixed audience**

> The service can save the same request twice. It saves the first request, but the client never receives the confirmation and sends it again. The service treats the repeat as a new request.

The revision supplies the sequence that the compressed technical description hides.

### Keep evidence within its limits

Given a prototype trial with five employees and no customer testing:

**Overstated**

> The new design helps customers finish faster.

**Faithful to the supplied evidence**

> Five employees completed the task faster with the prototype. We have not tested it with customers.

The improvement is accuracy, even though the faithful version is longer. More examples cover voice, restructuring, practical instructions, comparisons, and critique in [Revision and examples](references/revision-and-examples.md).

## Repository guide

```text
.
├── README.md
├── CONTRIBUTING.md
├── CHANGELOG.md
├── LICENSE
├── evals/
│   ├── README.md
│   ├── cases.json
│   ├── rubric.md
│   └── results-template.md
├── .gitignore
├── SKILL.md
├── agents/
│   └── openai.yaml
└── references/
    ├── writing-categories.md
    ├── reader-and-voice.md
    ├── structure-and-coherence.md
    ├── sentences-and-usage.md
    ├── revision-and-examples.md
    └── sources.md
```

| File | Purpose |
|---|---|
| [SKILL.md](SKILL.md) | Entry point: scope, brief, reference routing, workflow, and editorial boundaries. |
| [agents/openai.yaml](agents/openai.yaml) | Display metadata and the suggested invocation prompt. |
| [Writing categories](references/writing-categories.md) | Eight categories, their success criteria, and guidance for hybrid requests. |
| [Reader and voice](references/reader-and-voice.md) | Reader knowledge, communication stance, abstraction, uncertainty, and authorial character. |
| [Structure and coherence](references/structure-and-coherence.md) | Topic, point, paragraph progression, reference tracking, and logical relations. |
| [Sentences and usage](references/sentences-and-usage.md) | Parsing, information order, passive voice, usage judgments, punctuation, and rhythm. |
| [Revision and examples](references/revision-and-examples.md) | Diagnosis, worked editorial comparisons, review questions, and stopping criteria. |
| [Sources](references/sources.md) | Provenance, a chapter-to-concept map, public references, and optional private source consultation. |
| [CONTRIBUTING.md](CONTRIBUTING.md) | How to propose improvements and review changes to the guidance. |
| [Evaluations](evals/README.md) | Ten fictional cases, reviewer criteria, and a results template for manual comparisons. |
| [LICENSE](LICENSE) | MIT license for the original skill package. |
| [CHANGELOG.md](CHANGELOG.md) | Release history and notable changes to behavior, references, and installation. |

## Scope and limitations

The skill is intended for substantive nonfiction work. Fiction, poetry, and screenwriting fall outside its default scope. It is not an exhaustive grammar reference, an automated fact checker, or a guarantee of a particular model's output quality.

It instructs the assistant to preserve evidence and identify unresolved questions; it cannot supply missing evidence merely by improving prose. It also does not authorize research, publication, or sending correspondence beyond the user's task and the host's permissions.

The package includes a manual evaluation collection but no automated runner or measured model results. Its worked examples and evaluation criteria describe intended behavior; they do not establish audience comprehension or cross-model performance. Review consequential output against the original facts and brief.

Private source books, conversions, credentials, and client drafts should remain outside the shareable package. The `.gitignore` covers common local artifacts and dedicated private-source folders, but contributors should still inspect changes before committing.

## Customization and contributions

For a single piece, put voice, publication conventions, length, and output preferences in the prompt. For a recurring house style, maintain a focused reference with explicit scope and original examples. Keep the main skill concise and link to supporting guidance where it affects a decision.

Repository collaborators can use issues for unclear behavior, missing examples, or proposed improvements, and pull requests for concrete changes. See [CONTRIBUTING.md](CONTRIBUTING.md) for the review criteria.

## Writing evaluations

The [evaluation collection](evals/README.md) contains ten original fictional cases covering voice, evidence boundaries, commitments, critique scope, missing briefs, explanations, procedures, marketing, short-form writing, and contextual usage. Each case has three review criteria, including a critical requirement.

Run the same prompts against an exact skill revision and model, save the responses, then assess them using the separate reviewer rubric. A results template records the environment and evidence for each judgment. Comparing runs can reveal regressions without demanding identical prose. These are manual evaluation materials; no model runs or scores are claimed for this release.

## Versions and releases

The latest release is [v0.2.0](https://github.com/mikezupper/the-sense-of-style-writing-skill/releases/tag/v0.2.0). Read [CHANGELOG.md](CHANGELOG.md) for release history. Git tags identify exact revisions of the complete skill package; `main` contains ongoing development.

To install that version into a new directory:

```bash
mkdir -p "$HOME/.agents/skills"
git clone --branch v0.2.0 --depth 1 \
  git@github.com:mikezupper/the-sense-of-style-writing-skill.git \
  "$HOME/.agents/skills/sense-of-style-writing"
```

A tag checkout stays at that release rather than following `main`; the earlier `pull` instructions apply to a branch checkout. To move a pinned installation to another release, preserve any local edits, fetch tags, and check out the selected tag.

While the package is at `0.x`, minor releases cover substantive editorial or compatibility changes, and patch releases cover corrections that preserve intended behavior. Release notes should call out any changed expectations. Published tags stay fixed; corrections receive a new version. See [the release process](CONTRIBUTING.md#release-process).

## Attribution and licensing

The primary source is Steven Pinker's *The Sense of Style: The Thinking Person's Guide to Writing in the 21st Century* (2014). The skill's instructions, category guidance, workflows, and worked examples are independent adaptations. The [source guide](references/sources.md) records provenance, further references, and the limits of its scientific claims.

The original skill package is available under the [MIT license](LICENSE), copyright 2026 Mike Zupper. Preserve the copyright and license notices when reusing it.

The full book and companion deep dive are not bundled and are not required for ordinary use. This license covers the original repository materials, not the source book or third-party works referenced by the skill. Source attribution remains in [references/sources.md](references/sources.md).

## Possible next additions

These are proposals, not shipped capabilities or commitments:

- **Recorded evaluation runs:** use the collection to establish a baseline and compare future revisions or models.
- **Host-specific packaging:** add a plugin or another host's integration only when there is a concrete distribution need and it can be verified.
