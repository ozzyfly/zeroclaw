# AGENTS.md — ZeroClaw Agent Engineering Protocol

This file defines the default working protocol for coding agents in this repository.
Scope: entire repository. `CLAUDE.md` carries the identical protocol for Claude
agents — keep the two files in sync when editing either.

For PR-process detail (templates, superseded-PR attribution layout, handoff format),
see `CONTRIBUTING.md` and `docs/pr-workflow.md`. This file stays focused on the
load-bearing engineering protocol.

## 1) Project Snapshot (Read First)

ZeroClaw is a Rust-first autonomous agent runtime optimized for:

- high performance
- high efficiency
- high stability
- high extensibility
- high sustainability
- high security

Core architecture is trait-driven and modular. Most extension work should be done by implementing traits and registering in factory modules.

Key extension points:

- `src/providers/traits.rs` (`Provider`)
- `src/channels/traits.rs` (`Channel`)
- `src/tools/traits.rs` (`Tool`)
- `src/memory/traits.rs` (`Memory`)
- `src/observability/traits.rs` (`Observer`)
- `src/runtime/traits.rs` (`RuntimeAdapter`)
- `src/peripherals/traits.rs` (`Peripheral`) — hardware boards (STM32, RPi GPIO)

## 2) Deep Architecture Observations (Why This Protocol Exists)

These codebase realities should drive every design decision:

1. **Trait + factory architecture is the stability backbone** — extension points are explicit and swappable. Add features via trait implementation + factory registration, not cross-cutting rewrites.
2. **Security-critical surfaces are first-class and internet-adjacent** — `src/gateway/`, `src/security/`, `src/tools/`, `src/runtime/` carry high blast radius. Defaults are secure-by-default (pairing, bind safety, limits, secret handling); keep it that way.
3. **Performance and binary size are product goals** — the `Cargo.toml` release profile and dependency choices optimize for size and determinism. Convenience dependencies and broad abstractions can silently regress these goals.
4. **Config and runtime contracts are user-facing API** — `src/config/schema.rs` and CLI commands are effectively public interfaces. Backward compatibility and explicit migration matter.
5. **The project runs in high-concurrency collaboration mode** — CI, docs governance, and label routing are part of the delivery system. PR throughput is a design constraint.

## 3) Engineering Principles (Normative)

These principles are mandatory by default. They are implementation constraints, not slogans.

### 3.1 KISS

**Why here:** Runtime + security behavior must stay auditable under pressure.

- Prefer straightforward control flow over clever meta-programming.
- Prefer explicit match branches and typed structs over hidden dynamic behavior.
- Keep error paths obvious and localized.

### 3.2 YAGNI

**Why here:** Premature features increase attack surface and maintenance burden.

- Do not add new config keys, trait methods, feature flags, or workflow branches without a concrete accepted use case.
- Do not introduce speculative “future-proof” abstractions without at least one current caller.
- Keep unsupported paths explicit (error out) rather than adding partial fake support.

### 3.3 DRY + Rule of Three

**Why here:** Naive DRY can create brittle shared abstractions across providers/channels/tools.

- Duplicate small, local logic when it preserves clarity.
- Extract shared utilities only after repeated, stable patterns (rule-of-three).
- When extracting, preserve module boundaries and avoid hidden coupling.

### 3.4 SRP + ISP

**Why here:** Trait-driven architecture already encodes subsystem boundaries.

- Keep each module focused on one concern.
- Extend behavior by implementing existing narrow traits whenever possible.
- Avoid fat interfaces and “god modules” that mix policy + transport + storage.

### 3.5 Fail Fast + Explicit Errors

**Why here:** Silent fallback in agent runtimes can create unsafe or costly behavior.

- Prefer explicit `bail!`/errors for unsupported or unsafe states.
- Never silently broaden permissions/capabilities.
- Document fallback behavior when fallback is intentional and safe.

### 3.6 Secure by Default + Least Privilege

**Why here:** Gateway/tools/runtime can execute actions with real-world side effects.

- Deny-by-default for access and exposure boundaries.
- Never log secrets, raw tokens, or sensitive payloads.
- Keep network/filesystem/shell scope as narrow as possible unless explicitly justified.

### 3.7 Determinism + Reproducibility

**Why here:** Reliable CI and low-latency triage depend on deterministic behavior.

- Prefer reproducible commands and locked dependency behavior in CI-sensitive paths.
- Keep tests deterministic (no flaky timing/network dependence without guardrails).
- Ensure local validation commands map to CI expectations.

### 3.8 Reversibility + Rollback-First Thinking

**Why here:** Fast recovery is mandatory under high PR volume.

- Keep changes easy to revert (small scope, clear blast radius).
- For risky changes, define rollback path before merge.
- Avoid mixed mega-patches that block safe rollback.

## 4) Repository Map (High-Level)

- `src/main.rs` — CLI entrypoint and command routing
- `src/lib.rs` — module exports and shared command enums
- `src/config/` — schema + config loading/merging
- `src/agent/` — orchestration loop
- `src/gateway/` — webhook/gateway server
- `src/security/` — policy, pairing, secret store
- `src/memory/` — markdown/sqlite memory backends + embeddings/vector merge
- `src/providers/` — model providers and resilient wrapper
- `src/channels/` — Telegram/Discord/Slack/etc channels
- `src/tools/` — tool execution surface (shell, file, memory, browser)
- `src/peripherals/` — hardware peripherals (STM32, RPi GPIO); see `docs/hardware-peripherals-design.md`
- `src/runtime/` — runtime adapters (currently native)
- `docs/` — task-oriented documentation system (hubs, unified TOC, references, operations, security proposals, multilingual guides)
- `.github/` — CI, templates, automation workflows

## 4.1 Documentation System Contract (Required)

Treat documentation as a first-class product surface, not a post-merge artifact.

Canonical entry points:

- root READMEs: `README.md`, `README.zh-CN.md`, `README.ja.md`, `README.ru.md`, `README.fr.md`, `README.vi.md`
- docs hubs: `docs/README.md`, `docs/README.zh-CN.md`, `docs/README.ja.md`, `docs/README.ru.md`, `docs/README.fr.md`, `docs/i18n/vi/README.md`
- unified TOC: `docs/SUMMARY.md`

Supported locales (current contract): `en`, `zh-CN`, `ja`, `ru`, `fr`, `vi`

Collection indexes (category navigation):

- `docs/getting-started/README.md`
- `docs/reference/README.md`
- `docs/operations/README.md`
- `docs/security/README.md`
- `docs/hardware/README.md`
- `docs/contributing/README.md`
- `docs/project/README.md`

Runtime-contract references (must track behavior changes):

- `docs/commands-reference.md`
- `docs/providers-reference.md`
- `docs/channels-reference.md`
- `docs/config-reference.md`
- `docs/operations-runbook.md`
- `docs/troubleshooting.md`
- `docs/one-click-bootstrap.md`

Required docs governance rules:

- Keep README/hub top navigation and quick routes intuitive and non-duplicative.
- Keep entry-point parity across all supported locales (`en`, `zh-CN`, `ja`, `ru`, `fr`, `vi`) when changing navigation architecture.
- If a change touches docs IA, runtime-contract references, or user-facing wording in shared docs, perform i18n follow-through for currently supported locales in the same PR:
  - Update locale navigation links (`README*`, `docs/README*`, `docs/SUMMARY.md`).
  - Update localized runtime-contract docs where equivalents exist (at minimum `commands-reference`, `config-reference`, `troubleshooting` for `fr` and `vi`).
  - For Vietnamese, treat `docs/i18n/vi/**` as canonical. Keep `docs/*.<locale>.md` compatibility shims aligned if present.
- Keep proposal/roadmap docs explicitly labeled; avoid mixing proposal text into runtime-contract docs.
- Keep project snapshots date-stamped and immutable once superseded by a newer date.

## 5) Risk Tiers by Path (Review Depth Contract)

Use these tiers when deciding validation depth and review rigor.

- **Low risk**: docs/chore/tests-only changes
- **Medium risk**: most `src/**` behavior changes without boundary/security impact
- **High risk**: `src/security/**`, `src/runtime/**`, `src/gateway/**`, `src/tools/**`, `.github/workflows/**`, access-control boundaries

When uncertain, classify as higher risk.

## 6) Agent Workflow (Required)

1. **Read before write** — inspect existing module, factory wiring, and adjacent tests before editing.
2. **Define scope boundary** — one concern per PR; avoid mixed feature+refactor+infra patches.
3. **Implement minimal patch** — apply KISS/YAGNI/DRY rule-of-three explicitly.
4. **Validate by risk tier** — docs-only: lightweight checks; code/risky changes: full relevant checks and focused scenarios.
5. **Document impact** — update docs/PR notes for behavior, risk, side effects, and rollback. If CLI/config/provider/channel behavior changed, update corresponding runtime-contract references. If docs entry points changed, keep all supported locale README/docs-hub navigation aligned.
6. **Respect queue hygiene** — if stacked PR, declare `Depends on #...`; if replacing an old PR, declare `Supersedes #...`.

### 6.1 Branch / Commit / PR Flow (Required)

All contributors (human or agent) must follow the same collaboration flow:

- Create and work from a non-`main`, non-`dev` branch.
- Commit changes to that branch with clear, scoped commit messages.
- Open a PR to `dev`; do not push directly to `dev` or `main`.
- `main` is reserved for release promotion PRs from `dev`.
- Wait for required checks and review outcomes before merging.
- Merge via PR controls (squash/rebase/merge as repository policy allows).
- Branch deletion after merge is optional; long-lived branches are allowed when intentionally maintained.

### 6.2 Worktree Workflow (Required for Multi-Track Agent Work)

Use Git worktrees to isolate concurrent agent/human tracks safely and predictably:

- Use one worktree per active branch/PR stream to avoid cross-task contamination.
- Keep each worktree on a single branch; do not mix unrelated edits in one worktree.
- Run validation commands inside the corresponding worktree before commit/PR.
- Name worktrees clearly by scope (for example: `wt/ci-hardening`, `wt/provider-fix`) and remove stale worktrees when no longer needed.
- PR checkpoint rules from section 6.1 still apply to worktree-based development.

### 6.3 Code Naming Contract (Required)

Apply these naming rules for all code changes unless a subsystem has a stronger existing pattern.

- Use Rust standard casing consistently: modules/files `snake_case`, types/traits/enums `PascalCase`, functions/variables `snake_case`, constants/statics `SCREAMING_SNAKE_CASE`.
- Name types and modules by domain role, not implementation detail (for example `DiscordChannel`, `SecurityPolicy`, `MemoryStore` over vague names like `Manager`/`Helper`).
- Keep trait implementer naming explicit and predictable: `<ProviderName>Provider`, `<ChannelName>Channel`, `<ToolName>Tool`, `<BackendName>Memory`.
- Keep factory registration keys stable, lowercase, and user-facing (for example `"openai"`, `"discord"`, `"shell"`), and avoid alias sprawl without migration need.
- Name tests by behavior/outcome (`<subject>_<expected_behavior>`) and keep fixture identifiers neutral/project-scoped.
- If identity-like naming is required in tests/examples, use ZeroClaw-native labels only (`ZeroClawAgent`, `zeroclaw_user`, `zeroclaw_node`).

### 6.4 Architecture Boundary Contract (Required)

Use these rules to keep the trait/factory architecture stable under growth.

- Extend capabilities by adding trait implementations + factory wiring first; avoid cross-module rewrites for isolated features.
- Keep dependency direction inward to contracts: concrete integrations depend on trait/config/util layers, not on other concrete integrations.
- Avoid creating cross-subsystem coupling (for example provider code importing channel internals, tool code mutating gateway policy directly).
- Keep module responsibilities single-purpose: orchestration in `agent/`, transport in `channels/`, model I/O in `providers/`, policy in `security/`, execution in `tools/`.
- Introduce new shared abstractions only after repeated use (rule-of-three), with at least one real caller in current scope.
- For config/schema changes, treat keys as public contract: document defaults, compatibility impact, and migration/rollback path.

## 7) Change Playbooks

### 7.1 Adding a Provider

- Implement `Provider` in `src/providers/`.
- Register in `src/providers/mod.rs` factory.
- Add focused tests for factory wiring and error paths.
- Avoid provider-specific behavior leaks into shared orchestration code.

### 7.2 Adding a Channel

- Implement `Channel` in `src/channels/`.
- Keep `send`, `listen`, `health_check`, typing semantics consistent.
- Cover auth/allowlist/health behavior with tests.

### 7.3 Adding a Tool

- Implement `Tool` in `src/tools/` with strict parameter schema.
- Validate and sanitize all inputs.
- Return structured `ToolResult`; avoid panics in runtime path.

### 7.4 Adding a Peripheral

- Implement `Peripheral` in `src/peripherals/`.
- Peripherals expose `tools()` — each tool delegates to the hardware (GPIO, sensors, etc.).
- Register board type in config schema if needed.
- See `docs/hardware-peripherals-design.md` for protocol and firmware notes.

### 7.5 Security / Runtime / Gateway Changes

- Include threat/risk notes and rollback strategy.
- Add/update tests or validation evidence for failure modes and boundaries.
- Keep observability useful but non-sensitive.
- For `.github/workflows/**` changes, include Actions allowlist impact in PR notes and update `docs/actions-source-policy.md` when sources change.

### 7.6 Docs System / README / IA Changes

- Treat docs navigation as product UX: preserve clear pathing from README -> docs hub -> SUMMARY -> category index.
- Keep top-level nav concise; avoid duplicative links across adjacent nav blocks.
- When runtime surfaces change, update related references (`commands/providers/channels/config/runbook/troubleshooting`).
- Keep multilingual entry-point parity for all supported locales (`en`, `zh-CN`, `ja`, `ru`, `fr`, `vi`) when nav or key wording changes.
- When shared docs wording changes, sync corresponding localized docs for supported locales in the same PR (or explicitly document deferral and follow-up PR).
- For docs snapshots, add new date-stamped files for new sprints rather than rewriting historical context.

## 8) Validation Matrix

Default local checks for code changes:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Preferred local pre-PR validation path (recommended, not required):

```bash
./dev/ci.sh all
```

Notes:

- Local Docker-based CI is strongly recommended when Docker is available.
- Contributors are not blocked from opening a PR if local Docker CI is unavailable; in that case run the most relevant native checks and document what was run.

Additional expectations by change type:

- **Docs/template-only**:
    - run markdown lint and link-integrity checks
    - if touching README/docs-hub/SUMMARY/collection indexes, verify EN/ZH/JA/RU navigation parity
    - if touching bootstrap docs/scripts, run `bash -n bootstrap.sh scripts/bootstrap.sh scripts/install.sh`
- **Workflow changes**: validate YAML syntax; run workflow lint/sanity checks when available.
- **Security/runtime/gateway/tools**: include at least one boundary/failure-mode validation.

If full checks are impractical, run the most relevant subset and document what was skipped and why.

## 9) Collaboration and PR Discipline

- Follow `.github/pull_request_template.md` fully (including side effects / blast radius).
- Keep PR descriptions concrete: problem, change, non-goals, risk, rollback.
- Use conventional commit titles. Prefer small PRs (`size: XS/S/M`) when possible.
- Agent-assisted PRs are welcome, **but contributors remain accountable for understanding what their code will do**.
- When a PR supersedes another contributor's PR and carries forward substantive code or design, preserve authorship: add one `Co-authored-by:` trailer per materially incorporated contributor and list superseded PR links in the body. Full commit/PR template detail lives in `CONTRIBUTING.md` and `docs/pr-workflow.md`.

### 9.1 Privacy/Sensitive Data and Neutral Wording (Required)

Treat privacy and neutrality as merge gates, not best-effort guidelines.

- Never commit personal or sensitive data in code, docs, tests, fixtures, snapshots, logs, examples, or commit messages.
- Prohibited data includes (non-exhaustive): real names, personal emails, phone numbers, addresses, access tokens, API keys, credentials, IDs, and private URLs.
- Use neutral project-scoped placeholders (for example: `user_a`, `test_user`, `project_bot`, `example.com`) instead of real identity data.
- Test names/messages/fixtures must be impersonal and system-focused; avoid first-person or identity-specific language.
- If identity-like context is unavoidable, use ZeroClaw-scoped roles/labels only (for example: `ZeroClawAgent`, `ZeroClawOperator`, `zeroclaw_user`) and avoid real-world personas.
- If reproducing external incidents, redact and anonymize all payloads before committing.
- Before push, review `git diff --cached` specifically for accidental sensitive strings and identity leakage.

Reference docs:

- `CONTRIBUTING.md`
- `docs/README.md`
- `docs/SUMMARY.md`
- `docs/docs-inventory.md`
- `docs/commands-reference.md`
- `docs/providers-reference.md`
- `docs/channels-reference.md`
- `docs/config-reference.md`
- `docs/operations-runbook.md`
- `docs/troubleshooting.md`
- `docs/one-click-bootstrap.md`
- `docs/pr-workflow.md`
- `docs/reviewer-playbook.md`
- `docs/ci-map.md`
- `docs/actions-source-policy.md`

## 10) Anti-Patterns (Do Not)

- Do not add heavy dependencies for minor convenience.
- Do not silently weaken security policy or access constraints.
- Do not add speculative config/feature flags “just in case”.
- Do not mix massive formatting-only changes with functional changes.
- Do not modify unrelated modules “while here”.
- Do not bypass failing checks without explicit explanation.
- Do not hide behavior-changing side effects in refactor commits.
- Do not include personal identity or sensitive information in test data, examples, docs, or commits.

## 11) Handoff and Vibe Coding Guardrails

When handing off work (agent -> agent / maintainer), state: what changed, what did not change, validation run and results, remaining risks/unknowns, next recommended action.

When working in fast iterative mode:

- Keep each iteration reversible (small commits, clear rollback).
- Validate assumptions with code search before implementing.
- Prefer deterministic behavior over clever shortcuts.
- Do not “ship and hope” on security-sensitive paths.
- If uncertain, leave a concrete TODO with verification context, not a hidden guess.
</content>
</invoke>
