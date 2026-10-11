# Writing conventions

All project writing follows the pinned [Sense of Style skill](../.agents/skills/sense-of-style/SKILL.md).
The primary category is technical documentation. Readers are Rust contributors
and app developers who need to build the workspace, understand the JavaScript
boundary, and decide whether its tested behavior meets their needs.

Explain a concrete request before introducing an abstraction. Preserve exact
commands, identifiers, limits, units, and failure behavior. Put prerequisites
before instructions and expected results beside commands. API references support
lookup; conceptual documents explain responsibilities and tradeoffs.

Distinguish owner requirements, design defaults, measured results, and unknowns.
Do not convert a successful demo into a performance or production-readiness claim.
Record verification evidence in [verification](verification.md), while Beads
owns task status. Keep source attribution near the supported claim.

Review meaning, missing reader knowledge, organization, references, and sentences
before polishing wording. Follow the skill's revision reference for substantive
changes. Do not copy private books into docs, fixtures, or skill updates.
