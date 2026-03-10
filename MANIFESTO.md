# The HIEF (Hybrid Intent-Evaluation Framework) Manifesto

> HIEF (Hybrid Intent-Evaluation Framework) is a persistent memory layer for AI coding agents.

## Why

AI coding agents are powerful but forgetful. They generate code at
extraordinary speed, but they:

- **Forget** everything between sessions
- **Can't see** the whole codebase at once
- **Don't know** what other agents are doing
- **Can't verify** their own output quality systematically

HIEF exists to give agents memory, context, coordination, and guardrails —
without trying to replace the agent's reasoning or become a framework.

## Core Beliefs

### 1. Context Must Be Local and Instant

The most impactful thing you can do for AI-assisted development is solve
context retrieval. Agents need a searchable, AST-aware index of the
codebase that survives between sessions and updates incrementally. This
index must be local — developer tools that add latency get abandoned.

### 2. The Code Is the Spec

In brownfield codebases, the code IS the specification. Agents need to
understand the existing code, not read separate specification documents.
Heavy upfront documentation rots; machine-readable conventions endure.

### 3. Conventions Over Specifications

The most successful developer tools succeed through convention over
configuration. Instead of writing prose specifications, define
machine-readable conventions that agents can verify automatically.
A `.hief/conventions.toml` file is worth a thousand pages of markdown.

### 4. Evaluation Must Be Continuous

Binary pass/fail testing is insufficient for probabilistic outputs.
Quality criteria must be project-specific, scored on a spectrum, tracked
over time, and enforced in CI. Golden sets are the ground truth.

### 5. Coordination Must Be Lightweight

When multiple agents work on the same codebase, they need to know who's
doing what. But task coordination should be lightweight — don't compete
with project management tools. Intents are for agent coordination, not
human project planning.

### 6. Sidecar, Not Agent

HIEF provides memory. The host agent provides reasoning. HIEF does not
contain an LLM, does not execute code, and does not make decisions. It is
a composable tool that integrates with existing agent workflows, not a
replacement for them.

### 7. Humans Must Remain in the Loop

No agent may approve its own work. Quality regressions halt deployment
automatically. The audit trail is always available. Trust is built through
transparency, not automation.

## What HIEF Is

A single Rust binary that provides:

- **Code Index** — AST-aware chunking with keyword, structural, and
  semantic search
- **Intent Graph** — Lightweight DAG for task coordination and provenance
- **Evaluation Engine** — Golden-set quality scoring with regression
  detection
- **MCP Server** — Standard protocol interface for any compatible agent

## The HIEF Workflow

```
Search → Intend → Execute → Verify → Review
```

1. **Search**: Agent uses HIEF to understand the codebase
2. **Intend**: Agent declares what it will change (for non-trivial work)
3. **Execute**: Agent makes changes, following conventions
4. **Verify**: Agent runs evaluation, checks for regressions
5. **Review**: Human reviews the code and eval scores

## Commitments

- **Local-first**: Your code never leaves your machine
- **Open standards**: MCP protocol, open source, composable
- **Minimal footprint**: Single binary, single DB file, ~15 dependencies
- **Auditable**: Every action is traceable
- **Agent-agnostic**: Works with Claude Code, Cursor, Copilot, Windsurf, Goose
- **Adoption over mandate**: Let the tool prove itself; don't sell methodology
