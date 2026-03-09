---
status: planned
created: 2026-03-09
priority: medium
tags:
  - fleet
  - orchestration
  - ai-native
  - playbooks
  - sdd
  - leanspec
parent: 054-agent-fleet-execution-layer
depends_on:
  - 068-ai-native-coordination-primitives
  - 069-ai-native-domain-playbooks
created_at: 2026-03-09T17:06:00Z
updated_at: 2026-03-09T17:06:00Z
---

# AI-Native Spec-Driven Development Playbook

## Overview

Spec 069 maps AI-native coordination primitives to six domains (coding, finance, marketing, research, legal, devops). Every one of those playbooks *assumes specs already exist* — the coding playbook starts with "given a spec, fork 4 agents." But who writes the specs? How are they scoped, decomposed, validated, and kept alive as implementation diverges?

Spec-driven development (SDD) is itself a domain — and arguably the highest-leverage one to apply AI-native primitives to, because **every other playbook's quality is bounded by spec quality.** A speculative swarm executing against a vague spec produces vague implementations. An adversarial hardening loop testing against an incomplete spec misses the requirements that were never written down.

This spec defines playbooks for the SDD lifecycle as practiced with [LeanSpec](https://github.com/codervisor/lean-spec) — the lightweight, AI-native spec management framework. LeanSpec provides the substrate (spec storage, relationships, validation, MCP integration); this playbook defines how agent fleets *operate on that substrate* using the primitives from spec 068.

### Why SDD Is a Distinct Domain

SDD looks like "just writing documents" from the outside. From the inside, it involves activities with fundamentally different coordination needs than any domain in spec 069:

| SDD activity | Why it's hard | Why current practice is limited |
|---|---|---|
| **Scoping** — deciding what a spec covers | Requires simultaneous awareness of the entire project graph | Humans scope sequentially, one spec at a time, missing overlaps |
| **Decomposition** — splitting large concerns into spec-sized pieces | Requires exploring multiple decomposition strategies and evaluating trade-offs | Humans pick the first reasonable decomposition; can't try 4 strategies in parallel |
| **Consistency** — ensuring specs don't contradict each other | Scales quadratically with spec count (N specs = N² potential conflicts) | Humans rely on memory and ad-hoc cross-referencing |
| **Liveness** — keeping specs accurate as implementation evolves | Requires continuous monitoring of code, PRs, and test results | Humans update specs as an afterthought, if at all |
| **Gap detection** — finding what's not specified | Requires inference over the spec graph + implementation artifacts | Humans can't see negative space; only notice gaps when they cause bugs |

Each of these maps cleanly to one or more AI-native primitives.

## Design

### Playbook 1: Spec Exploration — Speculative Swarm

**Status quo:** Developer or PM identifies a need → writes one spec → team reviews → iterates. The spec's scope, structure, and decomposition reflect a single person's first instinct. Alternative framings are never explored.

**Horse carriage:** AI agent writes the spec faster. Same single-perspective output.

**Assembly line:**

Given a feature idea or problem statement, fork N agents to explore different spec strategies simultaneously:

1. **Divergent exploration.** Fork 4+ spec-authoring agents, each with a different strategy prompt:
   - **Monolith agent** — writes a single comprehensive spec covering the entire feature.
   - **Fracture agent** — decomposes into the maximum number of small, independent specs (optimizing for LeanSpec's <2000-token context economy).
   - **User-story agent** — frames from user/operator perspective with acceptance criteria.
   - **Architecture agent** — frames from system-design perspective with interface contracts and invariants.
   - **Adversarial agent** — frames from "what could go wrong" perspective, centering failure modes and edge cases.

2. **Cross-pollination at checkpoints.** At each checkpoint, agents share intermediate outputs. The fracture agent adopts the architecture agent's interface contracts. The user-story agent steals the adversarial agent's failure-mode scenarios. The monolith agent absorbs the fracture agent's scope boundaries as internal sections.

3. **Fragment fusion.** The merge agent assembles the final spec set by selecting the strongest fragments: the fracture agent's decomposition boundaries, the architecture agent's interface definitions, the user-story agent's acceptance criteria, the adversarial agent's edge cases. The result is a spec (or spec set) that no single authoring strategy could have produced.

*No human spec author can execute 5 framing strategies simultaneously and cherry-pick the best structural choices from each.*

```yaml
fleet:
  playbooks:
    sdd-exploration:
      primitive: speculative-swarm
      strategies:
        - monolith-comprehensive
        - fracture-context-economy
        - user-story-acceptance
        - architecture-contracts
        - adversarial-failure-modes
      checkpoints:
        interval: 1
        cross_pollinate: true
      merge: fragment-fusion
      budget: { max_agents: 6, timeout: 300s }
      leanspec:
        validate_tokens: true
        max_tokens_per_spec: 2000
        auto_number: true
```

### Playbook 2: Spec Hardening — Generative-Adversarial

**Status quo:** Spec goes through team review. Reviewers catch surface-level issues (typos, ambiguity) but rarely test for logical completeness, cross-spec consistency, or implementation feasibility. Review depth is limited by reviewer attention span.

**Horse carriage:** AI reviewer catches more issues. Still a single pass, still one perspective.

**Assembly line:**

A spec (or spec set) enters an adversarial loop with escalating attack sophistication:

1. **Round 1 — Clarity attacks.** Critic identifies: ambiguous terms, undefined acronyms, passive voice hiding responsibility ("the system should" — which system? which component?), unstated assumptions (what OS? what network topology?).

2. **Round 2 — Completeness attacks.** Critic attempts to construct scenarios the spec doesn't cover: edge cases in input ranges, concurrency scenarios, failure/recovery paths, upgrade/migration paths, security implications. Each uncovered scenario is a concrete test case the spec should address.

3. **Round 3 — Consistency attacks.** Critic cross-references against all existing specs in the LeanSpec project. Contradictions in terminology (spec A calls it "workspace", spec B calls it "project"). Conflicting constraints (spec A says "at most 10", spec B assumes "unbounded"). Dependency gaps (spec A depends on spec B which doesn't exist yet).

4. **Round 4 — Feasibility attacks.** Critic evaluates implementation feasibility: can the described behavior be implemented given the project's current architecture? Are the performance constraints achievable? Do the described interfaces conflict with existing code? This round requires reading actual source code, not just specs.

5. **Round 5 — Scope attacks.** Critic evaluates whether the spec is appropriately sized: too large (>2000 tokens, should be split per LeanSpec context economy)? Too small (a config flag doesn't need a spec)? Overlapping with existing specs (should be merged or cross-referenced)?

Termination: when the critic fails to find new issues for 2 consecutive rounds, or max rounds reached.

```yaml
fleet:
  playbooks:
    sdd-hardening:
      primitive: generative-adversarial
      generator: spec-author
      critic: spec-adversary
      max_rounds: 8
      escalation: progressive
      termination:
        consecutive_clean_rounds: 2
        quality_threshold: 0.90
      critic_modes:
        - clarity-ambiguity
        - completeness-edge-cases
        - cross-spec-consistency
        - implementation-feasibility
        - scope-appropriateness
      leanspec:
        cross_reference: all-project-specs
        read_source: true
```

### Playbook 3: Living Spec Graph — Context Mesh + Stigmergic

**Status quo:** Specs are written, implemented, and forgotten. They drift from reality within weeks. The dependency graph is manually maintained. Staleness is discovered only when someone reads an old spec and realizes it no longer matches the code.

**Horse carriage:** AI periodically scans specs and flags stale ones. Still batch-mode, still reactive.

**Assembly line:**

Two primitives compose to create a self-maintaining spec ecosystem:

**Context mesh** — a shared knowledge graph connecting specs, code artifacts, tests, PRs, and runtime behavior:

1. **Spec-code binding.** Each spec is linked to its implementing code artifacts (files, functions, modules). When code changes, affected spec nodes receive deltas.

2. **Gap detection.** The mesh continuously scans for missing nodes: "Spec X references interface Y, but no spec defines Y." "Code module Z has no covering spec." "Spec A's dependency B is still in `planned` status but A is `in-progress`." Any agent with relevant capability can claim and fill the gap.

3. **Conflict detection.** When a new spec is created or an existing spec is updated, the mesh checks for contradictions: overlapping scope with existing specs, conflicting constraints, incompatible interface definitions. Conflicts are surfaced immediately — not weeks later during implementation.

**Stigmergic maintenance** — agents watch the artifact space and react autonomously:

- **Staleness detector** watches `git diff` against spec-bound files. When implementation diverges from spec, it tags the spec with a `needs-update` marker and drafts a revision.
- **Relationship updater** watches spec creation/deletion events. When a new spec is created, it scans for related existing specs and proposes `depends_on`/`related` links.
- **Token watchdog** monitors spec token counts. When a spec grows beyond the LeanSpec budget (2000 tokens), it proposes a fractal split.
- **Status synchronizer** watches CI/test results. When all tests for a spec pass and the PR is merged, it proposes status advancement (`in-progress` → `complete`).
- **Board maintainer** reacts to spec state changes and keeps the LeanSpec kanban board accurate without human intervention.

*No human team can continuously monitor every code change, every PR, every test result, and reactively maintain spec accuracy across the entire project graph.*

```yaml
fleet:
  playbooks:
    sdd-living-graph:
      primitives: [context-mesh, stigmergic]
      mesh:
        nodes: [specs, code-files, tests, prs, ci-results]
        edges: [implements, tests, depends-on, related, contradicts]
        gap_detection: continuous
        conflict_detection: on-change
      stigmergic:
        agents:
          - id: staleness-detector
            watches: ["src/**/*.rs", "src/**/*.ts", "dashboard/**/*.tsx"]
            produces: ["specs/*/README.md"]
            markers: [staleness-score, drift-severity]
          - id: relationship-updater
            watches: ["specs/*/README.md"]
            produces: ["specs/*/README.md"]
            markers: [relationship-completeness]
          - id: token-watchdog
            watches: ["specs/*/README.md"]
            produces: ["specs/*/README.md"]
            markers: [token-budget-usage]
          - id: status-synchronizer
            watches: [".github/workflows/*", "test-results/*"]
            produces: ["specs/*/README.md"]
            markers: [implementation-completeness]
        marker_decay: 86400s
        reaction_debounce: 30s
      leanspec:
        project_dir: specs/
        validate_on_change: true
```

### Playbook 4: Spec Decomposition — Fractal Decomposition

**Status quo:** A PM or architect manually breaks a large feature into sub-specs. Decomposition reflects one person's mental model. Sub-specs lose context from the parent (the "management handoff" problem — each level of delegation loses information). Dependencies between sub-specs are discovered during implementation, not during planning.

**Horse carriage:** AI decomposes faster, but still single-pass, still one decomposition strategy.

**Assembly line:**

When a spec exceeds complexity or token thresholds, it fractal-splits:

1. **Self-analysis.** The parent agent analyzes its spec and identifies orthogonal concerns: identity vs. authorization vs. audit vs. enrollment (as spec 060 did for fleet identity).

2. **Context-preserving split.** The parent forks itself N times. Each child inherits the *full parent spec context* (including motivation, constraints, cross-references) but receives a scoping constraint: "you are responsible only for the identity concern." This is fundamentally different from a human architect writing a brief for a subordinate — the children ARE the parent, with zero information loss.

3. **Recursive depth.** Complex children may further split. The "authorization" child might split into "RBAC model", "policy engine", "permission propagation." Depth is bounded by LeanSpec's context economy principle — splitting stops when each leaf spec fits within the token budget.

4. **Lossless reunification.** After children draft their sub-specs, the parent reassembles and validates: are the sub-specs collectively exhaustive? Are their interfaces compatible? Are dependencies correctly declared? Do they sum to the original parent's intent? Because children are forks of the parent, this validation has full understanding of *why* each decomposition choice was made.

5. **Dependency graph generation.** The fractal process automatically generates `depends_on` relationships between sibling specs and between children and external specs. LeanSpec's relationship tracking captures these immediately.

```yaml
fleet:
  playbooks:
    sdd-decomposition:
      primitive: fractal-decomposition
      split_strategy: orthogonal-concerns
      max_depth: 3
      max_children_per_level: 6
      reunification: lossless-merge
      scope_isolation: true
      split_triggers:
        token_count: 2000
        concern_count: 3
      budget: { max_total_agents: 15 }
      leanspec:
        auto_number: true
        auto_link_depends_on: true
        validate_collective_exhaustiveness: true
```

### Composed Playbook: Full SDD Lifecycle

The four playbooks compose into a complete SDD lifecycle that has no human-process analogue:

```
Feature idea
    │
    ▼
┌─────────────────────┐
│  Spec Exploration    │  Speculative swarm: 5 strategies
│  (Playbook 1)       │  Fragment fusion → optimal spec set
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Spec Decomposition  │  Fractal split if oversized/complex
│  (Playbook 4)        │  Auto-generates dependency graph
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Spec Hardening      │  Adversarial escalation: 5 attack modes
│  (Playbook 2)        │  Cross-spec consistency check
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Implementation      │  (other domain playbooks from spec 069)
│  Coding / Testing    │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│  Living Spec Graph   │  Context mesh + stigmergic maintenance
│  (Playbook 3)        │  Continuous — runs forever post-creation
└─────────────────────┘
```

The lifecycle is not a pipeline — Playbook 3 (living graph) runs continuously from spec creation through implementation and beyond. Playbooks 1, 2, and 4 may re-trigger when the living graph detects that implementation has diverged enough to warrant spec revision.

```yaml
fleet:
  playbooks:
    sdd-lifecycle:
      stages:
        - name: explore
          playbook: sdd-exploration
          trigger: manual  # human provides feature idea
        - name: decompose
          playbook: sdd-decomposition
          trigger: auto    # fires if explore output exceeds thresholds
          condition: "spec.token_count > 2000 || spec.concerns > 3"
        - name: harden
          playbook: sdd-hardening
          trigger: auto    # fires on every spec from explore/decompose
        - name: maintain
          playbook: sdd-living-graph
          trigger: auto    # starts on spec creation, runs continuously
          lifecycle: persistent
      leanspec:
        mcp_server: "@leanspec/mcp"
        project_dir: specs/
```

### Integration with LeanSpec MCP

All playbooks interact with the LeanSpec project through its MCP server tools:

| Playbook action | LeanSpec MCP tool |
|---|---|
| Check existing specs before creating | `search`, `list` |
| View related specs for cross-referencing | `view`, `deps` |
| Create new specs from swarm output | `create` |
| Update spec metadata (status, tags) | `update` |
| Link generated dependencies | `link` |
| Validate token budget compliance | `tokens`, `validate` |
| Monitor project health | `board`, `stats` |

The MCP integration means playbooks operate on the same spec substrate that human developers use — there's no separate "AI spec format." Specs created by swarms, hardened by adversarial loops, and maintained by stigmergic agents are standard LeanSpec markdown files, visible on the kanban board, searchable, and human-editable.

## Plan

- [ ] Define `sdd-exploration` playbook template with 5-strategy speculative swarm config.
- [ ] Define `sdd-hardening` playbook template with 5-mode adversarial escalation.
- [ ] Define `sdd-living-graph` playbook template composing context mesh + stigmergic agents.
- [ ] Define `sdd-decomposition` playbook template with fractal split + auto-dependency linking.
- [ ] Define composed `sdd-lifecycle` playbook wiring the four stages together.
- [ ] Integrate playbook actions with LeanSpec MCP tools (`search`, `create`, `update`, `link`, `validate`, `tokens`).
- [ ] Implement staleness-detector stigmergic agent: watches code changes, scores spec drift, drafts revisions.
- [ ] Implement token-watchdog agent: monitors spec size vs. LeanSpec context economy budget, proposes splits.
- [ ] Implement relationship-updater agent: watches spec creation/deletion, proposes `depends_on`/`related` links.
- [ ] Add `clawden fleet playbook sdd` CLI entry point for triggering the lifecycle.
- [ ] Document LeanSpec MCP tool mapping and playbook-to-tool protocol.

## Test

- [ ] Spec exploration: speculative swarm produces 5 divergent spec framings for "add user authentication"; fragment fusion combines architecture agent's interface contracts + user-story agent's acceptance criteria + adversarial agent's failure modes into a spec no single strategy produced.
- [ ] Spec decomposition: a 3500-token spec fractal-splits into 3 sub-specs, each under 2000 tokens; `depends_on` relationships are auto-generated between siblings; reunification validates collective exhaustiveness.
- [ ] Spec hardening round escalation: round 1 finds 3 ambiguities; round 2 finds 2 uncovered edge cases; round 3 finds 1 cross-spec contradiction; round 4 (feasibility) finds no issues; round 5 confirms scope is appropriate; terminates.
- [ ] Cross-spec consistency: adversarial critic detects that spec A says "max 10 agents" while spec B says "unbounded fleet size"; flags contradiction with citations to both specs.
- [ ] Staleness detection: code change to `src/auth.rs` triggers staleness-detector; spec 060 (fleet identity) is tagged with `needs-update` marker; draft revision reflects the code delta.
- [ ] Token watchdog: spec grows to 2400 tokens via manual edit; token-watchdog proposes fractal split into 2 sub-specs with auto-numbered directories.
- [ ] Relationship updater: new spec 071 is created; relationship-updater scans existing specs, proposes `related: [060-agent-fleet-identity-auth]` based on content similarity.
- [ ] Status synchronizer: all tests for spec 042 pass in CI + PR merged; status-synchronizer proposes `status: complete` update.
- [ ] Living graph gap detection: spec references "config validation module" but no spec covers it; gap surfaced as actionable item on LeanSpec board.
- [ ] Full lifecycle: feature idea → exploration produces 5 variants → fusion creates 1 spec (2800 tokens) → auto-triggers decomposition into 2 sub-specs → each enters hardening (3 rounds each) → living graph begins monitoring. End-to-end produces 2 validated, properly-linked, board-visible specs from a single sentence input.
- [ ] LeanSpec MCP integration: all playbook actions use MCP tools (`create`, `update`, `link`, `validate`, `tokens`); no direct file manipulation bypasses LeanSpec.
- [ ] Budget enforcement: exploration swarm respects `max_agents: 6`; decomposition respects `max_total_agents: 15`; exceeding triggers graceful degradation, not failure.

## Notes

This is a **meta-playbook** — it produces the specs that other playbooks consume. The quality ceiling of every domain playbook in spec 069 is set by the quality of specs entering them. Investing in AI-native SDD is a force multiplier across all other fleet operations.

The boundary with spec 069: that spec owns the domain playbook schema and the six domain playbooks (coding, finance, marketing, research, legal, devops). This spec owns the SDD-specific playbooks that operate on the spec layer itself. The composed lifecycle here *feeds into* spec 069's coding playbook — once specs are explored, hardened, and decomposed, they become inputs to the coding swarm.

The boundary with spec 068: that spec owns the five AI-native primitives and their implementation. This spec composes those primitives into SDD-specific configurations but does not extend the primitive trait surface.

LeanSpec integration is via MCP tools, not direct library coupling. ClawDen playbooks invoke `@leanspec/mcp` the same way a human developer's AI assistant would — through the standard tool protocol. This keeps LeanSpec and ClawDen independently evolvable.
