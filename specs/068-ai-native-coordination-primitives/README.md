---
status: planned
created: 2026-03-09
priority: medium
tags:
- fleet
- orchestration
- coordination
- ai-native
- innovation
parent: 054-agent-fleet-execution-layer
depends_on:
  - 067-advanced-coordination-patterns
created_at: 2026-03-09T06:25:01.610088324Z
updated_at: 2026-03-09T06:25:01.610088324Z
---

# AI-Native Coordination Primitives — Beyond Organizational Metaphors

## Overview

Spec 067's Phase 1 maps real-world organizational structures (hierarchy, pipeline, committee, marketplace) onto agent fleets. This is valuable — and also exactly the mistake every industrial revolution's first wave makes. The spinning jenny mechanized hand-weaving. The first electric factories bolted motors onto the same belt-driven layouts designed for steam. The first digital offices scanned paper forms into PDFs.

The **second wave** — the one that creates orders-of-magnitude value — invents production models that are impossible without the new technology. Ford didn't just replace horses with engines; he invented the assembly line, which only works because electric motors deliver power on demand at any point (steam's centralized shaft-and-belt couldn't do that). Amazon didn't just sell books online; they invented infrastructure-as-a-service, which only works because compute is fungible (physical stores aren't).

This spec defines **coordination primitives that have no human organizational analogue** — they exploit properties unique to AI agents that humans fundamentally lack. These are not patterns borrowed from org charts. They are the assembly lines of the agent era.

### Why Organizational Metaphors Hit a Ceiling

Human coordination patterns exist because of **human constraints**:

| Human constraint | Org pattern it created | AI agents don't have this constraint |
|---|---|---|
| Humans can't be cloned | Fixed team rosters, hiring | **Zero fork cost** — spawn/destroy agents freely |
| Humans communicate in lossy natural language | Meetings, reports, handoff docs | **Lossless context transfer** — share full state, not summaries |
| Humans have fixed identities and skills | Job titles, departments, training programs | **Elastic identity** — mutate role/expertise instantly |
| Humans can only do one thing at a time | Sequential task assignment, scheduling | **Speculative parallelism** — execute N strategies, keep the best |
| Humans have ego, status, and politics | Management layers, conflict resolution, HR | **No social overhead** — zero coordination tax for consensus |
| Humans tire and context-switch with cognitive cost | 8-hour days, sprints, focus time | **Tireless and stateless** — no degradation over time |
| Human thought is opaque | Status meetings, standups, reports | **Perfect observability** — inspect any agent's full internal state |

Mapping agent fleets to org charts preserves all of these constraints by design. AI-native coordination starts by **discarding them**.

## Design

### Extended Trait Surface

The Phase 1 `CoordinationPattern` trait assumes fixed agent rosters and message-based coordination. AI-native patterns need additional capabilities:

```rust
trait AINativeCoordination: CoordinationPattern {
    /// Dynamically spawn new agents mid-task (zero fork cost)
    fn spawn(&mut self, template: &AgentTemplate, context: ContextSnapshot) -> AgentId;
    /// Merge multiple agents' states into one (elastic identity)
    fn merge(&mut self, agents: &[AgentId], strategy: MergeStrategy) -> AgentId;
    /// Observe another agent's full internal state (perfect observability)
    fn observe(&self, agent: AgentId) -> Option<&AgentState>;
    /// Fork an agent into N copies with divergent parameters (speculative execution)
    fn fork(&mut self, agent: AgentId, variants: Vec<VariantConfig>) -> Vec<AgentId>;
    /// Detect convergence across parallel execution paths
    fn convergence(&self, agents: &[AgentId], threshold: f64) -> ConvergenceResult;
    /// Prune agents that are no longer contributing novel progress
    fn prune(&mut self, agents: &[AgentId], criterion: PruneCriterion) -> Vec<AgentId>;
}

enum MergeStrategy {
    /// Take the best parts from each agent's output (fragment fusion)
    FragmentFusion { scorer: Box<dyn Fn(&Fragment) -> f64> },
    /// One agent's state wins entirely (competitive selection)
    WinnerTakeAll { metric: QualityMetric },
    /// Weighted blend of all agents' states
    WeightedBlend { weights: Vec<(AgentId, f64)> },
}
```

### Primitive 1: Speculative Swarm

**What it is:** Fork N agents to explore different strategies for the same task *simultaneously*, with midpoint cross-pollination and convergence-based pruning. The final output is assembled from the best fragments across all surviving branches.

**Why it's impossible with humans:** Humans can brainstorm ideas, but they can't *execute* 5 strategies in parallel and then fuse the best parts of each execution. An agent can be forked, run divergently, observed, and merged — a human cannot.

**Mechanism:**
1. **Seed phase** — the coordinator forks the originating agent N times, each with a different strategy prompt (e.g., "solve via recursion", "solve via iteration", "solve via reduction").
2. **Exploration phase** — all forks execute independently. At configurable checkpoints, each fork's intermediate state is broadcast to all others (cross-pollination). Forks may incorporate useful fragments from siblings.
3. **Convergence detection** — the coordinator continuously measures output similarity. When two branches produce >threshold overlap, the lower-quality branch is pruned (freed).
4. **Fragment fusion** — surviving branches' outputs are decomposed into scored fragments. A merge agent assembles the final output by selecting the highest-scoring fragment for each sub-problem.

**Not a committee. Not an ensemble.** Committees discuss and vote on one solution. Ensembles average independent predictions. Speculative swarm *executes divergently and fuses selectively* — it produces outputs that no single agent could have produced alone.

```yaml
fleet:
  swarms:
    problem-solver:
      base_agent: solver
      strategies:
        - prompt_suffix: "approach via divide-and-conquer"
        - prompt_suffix: "approach via constraint propagation"
        - prompt_suffix: "approach via analogy from similar domains"
        - prompt_suffix: "approach via first principles decomposition"
      checkpoint_interval: 30s
      convergence_threshold: 0.85
      merge: fragment-fusion
      max_forks: 8
      budget: { max_tokens: 500000, max_cost_usd: 2.00 }
```

### Primitive 2: Context Mesh

**What it is:** A shared, reactive knowledge graph where agents observe knowledge gaps and fill them autonomously — no routing, no handoffs, no manager deciding who knows what.

**Why it's impossible with humans:** Human knowledge is opaque and lossy. You can't observe what another person knows, detect gaps in their understanding, or reactively push precisely the knowledge they're missing. Agents can.

**Mechanism:**
1. **Shared context graph** — a DAG where nodes are knowledge claims (facts, code artifacts, decisions) and edges are dependencies. Every agent reads and writes to the same graph.
2. **Gap detection** — agents continuously scan the graph for missing dependencies: "Node X depends on Y, but Y doesn't exist." Any agent with relevant capability can claim the gap.
3. **Reactive propagation** — when a node is filled or updated, all agents who depend on it are notified with the delta. No polling, no status meetings.
4. **Conflict resolution** — if two agents fill the same gap, a brief compete-and-compare selects the higher-confidence version. Unlike human turf wars, this takes milliseconds.

**Not departmental routing.** Departments gate knowledge through managers and gateway agents. Context mesh makes *all knowledge visible to all agents simultaneously* — coordination emerges from information availability, not organizational structure.

```yaml
fleet:
  mesh:
    context_graph:
      storage: shared-kv
      propagation: reactive
      conflict: compete-and-compare
    agents:
      - id: researcher
        watches: ["requirements.*", "constraints.*"]
        publishes: ["findings.*", "evidence.*"]
      - id: architect
        watches: ["findings.*", "constraints.*"]
        publishes: ["design.*", "interfaces.*"]
      - id: implementer
        watches: ["design.*", "interfaces.*"]
        publishes: ["code.*", "tests.*"]
```

### Primitive 3: Fractal Decomposition

**What it is:** An agent facing a complex task *splits itself* into scoped sub-agents, each inheriting the parent's full context but narrowing to a specific sub-problem. Sub-agents may recursively split further. On completion, they reunify into the original.

**Why it's impossible with humans:** Hierarchical delegation requires a manager to understand the problem well enough to decompose it and brief separate workers *who don't share the manager's full context*. In fractal decomposition, the agent IS the workers — it splits with full context preservation and reunifies losslessly. There's no brief, no handoff, no information loss at each hierarchical level.

**Mechanism:**
1. **Split** — agent analyzes its task and identifies N orthogonal sub-problems. It forks itself N times, each fork receiving the full parent context plus a scoping constraint ("you are responsible only for sub-problem K").
2. **Recursive depth** — each child may further split if its sub-problem is still complex. Depth is bounded by config.
3. **Reunification** — when all children complete, their outputs are merged back into the parent agent. Because children were forks of the parent (not strangers), reunification is lossless — the parent can integrate sub-results with full understanding of *why* each child made its choices.
4. **Scope isolation** — during split, children can only modify artifacts within their scoped sub-problem. This prevents conflicting writes without locks.

**Not hierarchical delegation.** Hierarchy has information loss at every level (manager briefs worker, worker briefs sub-worker). Fractal decomposition has *zero information loss* because the children ARE the parent.

```yaml
fleet:
  fractal:
    solver:
      base_agent: architect
      split_strategy: orthogonal-subproblems
      max_depth: 4
      max_children_per_level: 5
      reunification: lossless-merge
      scope_isolation: true
      budget: { max_total_agents: 20 }
```

### Primitive 4: Generative-Adversarial Coordination

**What it is:** Two agent roles — generator and critic — locked in an escalating quality loop. The critic doesn't just review; it *actively tries to break* the generator's output. The generator doesn't just fix; it *anticipates and preempts* the critic's attack patterns. Quality emerges from adversarial pressure, not checklist compliance.

**Why it's impossible with humans:** Human code review has social dynamics: reviewers don't want to seem hostile, authors get defensive, review depth is limited by time and cognitive load. Agents have no ego — the adversarial pressure can be maximally intense without social cost. Additionally, the critic can *execute* the generator's code and construct adversarial inputs automatically, not just read and comment.

**Mechanism:**
1. **Generate** — generator agent produces initial artifact (code, plan, document).
2. **Attack** — critic agent actively attempts to break it: generate adversarial inputs, find logical flaws, construct edge cases, attempt to violate stated invariants.
3. **Escalation** — each round, the critic's attack sophistication increases (simple edge cases → combinatorial inputs → adversarial optimization). The generator sees the full history of attacks and adapts.
4. **Termination** — the loop ends when: (a) the critic fails to find new issues for K consecutive rounds, (b) a quality score exceeds the threshold, or (c) max rounds reached.
5. **Progressive difficulty** — unlike human review where depth is roughly constant, the adversarial agent can increase its "effort budget" each round, going from surface-level to deep semantic analysis.

```yaml
fleet:
  adversarial:
    code-hardening:
      generator: coding-agent
      critic: adversarial-tester
      max_rounds: 10
      escalation: progressive
      termination:
        consecutive_clean_rounds: 2
        quality_threshold: 0.95
      critic_modes:
        - syntax-and-types
        - edge-cases
        - concurrency-safety
        - adversarial-inputs
```

### Primitive 5: Stigmergic Coordination

**What it is:** Agents coordinate through the shared artifact space rather than through messages. Like ants depositing pheromones: agents observe changes to shared artifacts and react to them. No central coordinator, no task queue, no explicit routing.

**Why it's impossible with humans:** Humans can't continuously monitor a codebase and react in real-time to every change. Agents can subscribe to artifact mutations and trigger automatically. Human stigmergy (leaving notes on a whiteboard) is lossy and slow; agent stigmergy is precise and instant.

**Mechanism:**
1. **Artifact observation** — every agent subscribes to a set of artifact patterns (files, code regions, knowledge graph nodes). Changes trigger the observer.
2. **Reactive production** — when an agent detects a relevant change, it *produces new artifacts* in response, which may trigger other agents.
3. **Pheromone markers** — agents tag artifacts with metadata (confidence, completeness, needs-review) that influence other agents' prioritization. Markers decay over time if not refreshed.
4. **Emergent workflow** — no predefined pipeline or task graph. The workflow emerges from agent reaction patterns. A coding agent produces code → a testing agent detects new untested code → a docs agent detects undocumented API → a security agent detects un-audited endpoints. Each reaction is autonomous.

**Not event-driven architecture.** Event-driven systems have predefined event types and handlers. Stigmergic coordination has agents that *autonomously decide* what artifact changes are relevant and what to do about them. The same artifact change might trigger different agents differently depending on their current state.

```yaml
fleet:
  stigmergic:
    agents:
      - id: implementer
        watches: ["specs/*.md", "design/*.md"]
        produces: ["src/**/*.rs"]
        markers: [confidence, completeness]
      - id: tester
        watches: ["src/**/*.rs"]
        produces: ["tests/**/*.rs"]
        markers: [coverage, edge-case-depth]
      - id: documenter
        watches: ["src/**/*.rs"]
        produces: ["docs/**/*.md"]
        markers: [completeness, accuracy]
      - id: security-auditor
        watches: ["src/**/*.rs"]
        produces: ["security/*.report"]
        markers: [threat-level, audit-depth]
    marker_decay: 3600s
    reaction_debounce: 5s
```

### Composability

These primitives compose. A stigmergic fleet might use fractal decomposition within a single agent's reaction. A speculative swarm might use generative-adversarial loops to evaluate each branch. A context mesh might feed a speculative swarm's cross-pollination checkpoints.

The `CoordinationPattern` trait from spec 067 remains the extension point — AI-native patterns just implement a richer surface area.

## Analysis: The Industrial Revolution Lens

### The Pattern Across Revolutions

| Revolution | First wave (old model + new tech) | Second wave (new model only possible with new tech) |
|---|---|---|
| Steam (1760s) | Steam pumps replacing hand-pumps in mines | Factory system — centralized production with power-driven machinery |
| Electricity (1880s) | Electric motors replacing steam belts in same factory layouts | Assembly line — unit drive motors at each station enable Ford Model T |
| Information (1970s) | Computerized paper forms, digital filing cabinets | Internet-native business — Amazon, Google, SaaS, the long tail |
| AI Agents (now) | **Agent fleets mimicking org charts** (hierarchy, departments, committees) | **AI-native primitives** — speculative swarm, context mesh, fractal decomposition, stigmergy |

### What Assembly Lines Teach Us

Ford's assembly line wasn't "hire more workers." It exploited properties unique to electric motors:
- **Unit drive**: each machine has its own motor (no shared belt). Agents: each fork has its own context (no shared manager bottleneck).
- **Any layout**: machines can be arranged by workflow, not by power source proximity. Agents: coordination topology is unconstrained by communication overhead.
- **Continuous flow**: work moves to workers, not workers to work. Agents: context flows via mesh, not via meetings.

The assembly line **made previous organizational patterns (craft guilds, putting-out system) obsolete** — not by being faster at the old model, but by making the old model irrelevant.

### What This Means for Agent Coordination

Organizational metaphors (spec 067 Phase 1) are the **first wave** — valuable, necessary, and fundamentally limited. They will hit a ceiling because they preserve human constraints that agents don't have.

AI-native primitives are the **second wave** — they discard inapplicable constraints and exploit unique capabilities. The ceiling is much higher because the coordination model matches the medium.

The practical implication: **don't just build both**. Build Phase 1 as the stable foundation, but design every abstraction (traits, config schema, message protocol) with Phase 2 in mind. The `CoordinationPattern` trait must accommodate dynamic spawn/merge/fork/observe from day one, even if Phase 1 patterns don't use those methods.

## Plan

- [ ] Design `AINativeCoordination` trait extension with spawn/merge/fork/observe/prune methods.
- [ ] Extend `CoordinationAction` enum with `Spawn`, `Fork`, `Merge`, `Prune`, `Publish`, `Claim` variants.
- [ ] Implement speculative swarm: fork manager, checkpoint broadcaster, convergence detector, fragment fuser.
- [ ] Implement context mesh: shared context graph (DAG), gap detector, reactive propagator, conflict resolver.
- [ ] Implement fractal decomposition: self-splitting with scope isolation, recursive depth control, lossless reunification.
- [ ] Implement generative-adversarial: attack escalation engine, progressive difficulty modes, termination detector.
- [ ] Implement stigmergic coordination: artifact watcher, pheromone marker system, reaction debouncer, emergent workflow tracker.
- [ ] Integrate with budget controls: speculative swarm and fractal decomposition must respect max_total_agents and cost-tier limits.
- [ ] Extend `clawden.yaml` fleet config schema for all five primitives.
- [ ] Validate composability: at least one composed pattern (e.g., stigmergic + fractal) works end-to-end.

## Test

- [ ] Speculative swarm: 4 forks explore different strategies; cross-pollination at checkpoints incorporates sibling fragments; convergence prunes one redundant branch; fragment fusion assembles final output from 3 survivors' best parts.
- [ ] Context mesh: 3 agents observe shared graph; researcher publishes finding → architect reactively updates design; architect detects dependency gap → claims and fills it; implementer receives delta notification.
- [ ] Context mesh conflict: two agents fill the same gap simultaneously; compete-and-compare resolves within one cycle; losing agent is notified and can redirect effort.
- [ ] Fractal decomposition: agent splits into 3 scoped children; children produce results with scope isolation (no cross-writes); parent reunifies losslessly; output is coherent.
- [ ] Fractal depth limiting: agent attempts recursive split beyond max_depth; split is rejected; agent completes at current depth.
- [ ] Generative-adversarial: generator produces code; critic finds 3 issues in round 1 (syntax-level); generator fixes all 3; critic escalates to edge-cases in round 2; generator preempts 2 of 3 attacks; round 3 critic finds no new issues; terminates.
- [ ] Stigmergic: implementer writes new function (artifact change) → tester detects it and auto-generates tests → documenter detects undocumented API and adds docs → security auditor scans for vulnerabilities. No explicit task assignment occurred.
- [ ] Marker decay: pheromone marker set at t=0 loses influence by t=decay_interval; agents re-prioritize accordingly.
- [ ] Budget enforcement: speculative swarm with max_total_agents=6 refuses to fork a 7th agent; graceful degradation continues with 6.
- [ ] Composability: pipeline with speculative-swarm inner stage produces correct output; stigmergic fleet with fractal inner decomposition handles recursive artifact reactions.

## Notes

This spec deliberately does not cover distributed execution of AI-native patterns. Running a speculative swarm across multiple hosts requires spec 062's remote control channel. That's a future extension — get the single-host primitives right first.

The boundary with spec 067 Phase 1: that spec owns the `CoordinationPattern` trait and organizational patterns (hierarchy, pipeline, committee, departmental, marketplace, matrix). This spec owns the `AINativeCoordination` extension and the five AI-native primitives. Both share the same `AgentEnvelope` protocol and `MessageBus` from spec 065.