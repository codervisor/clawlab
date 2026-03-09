---
status: planned
created: 2026-03-09
priority: medium
tags:
- fleet
- orchestration
- ai-native
- cost-optimization
- nemosis
- distillation
depends_on:
- 068-ai-native-coordination-primitives
- 069-ai-native-domain-playbooks
parent: 054-agent-fleet-execution-layer
created_at: 2026-03-09T09:20:25.854864694Z
updated_at: 2026-03-09T09:20:25.854864694Z
---

# Nemosis Integration — Teacher-Student Knowledge Distillation for Fleet Cost Optimization

## Overview

Specs 068–069 define coordination primitives and domain playbooks that assume uniformly capable agents — every fork, every swarm member, every adversarial critic runs on the same high-capability model. This works for correctness, but the economics are brutal. A speculative swarm forking 8 GPT-4-class agents for a single code task burns $2–5 per invocation. A stigmergic maintenance fleet running 24/7 with frontier models could cost more than the human team it replaces.

[Nemosis](https://github.com/codervisor/nemosis) solves this through **knowledge distillation** — a teacher-student iterative training loop that collapses expert reasoning into lightweight `SKILL.md` artifacts. A high-capability Teacher model (e.g., Claude Opus, GPT-4) executes a task with full multi-turn reasoning. Nemosis captures those execution traces, distills them into a Strict Contract (JSON Schema) + Strategic Context (Markdown Instructions), and produces a `SKILL.md` that enables a cheaper Student model (e.g., GPT-4o-mini, Claude Haiku) to replicate the behavior at a fraction of the cost.

This spec integrates Nemosis into ClawDen's fleet execution layer so that AI-native coordination primitives can **automatically optimize their cost profile** by distilling expensive patterns into cheap skill-equipped agents, with a memory layer that enables iterative refinement.

### Why This Matters for ClawDen

| Fleet primitive | Cost without Nemosis | Cost with Nemosis |
|---|---|---|
| Speculative swarm (8 forks) | 8× frontier model | 1× frontier teacher + 7× distilled student |
| Stigmergic maintenance (5 watchers) | 5× frontier 24/7 | 5× student after initial skill distillation |
| Generative-adversarial (6 rounds) | 12× frontier (generator + critic) | 2× frontier for novel rounds + student for patterns already distilled |
| Fractal decomposition (15 agents) | 15× frontier | 1–3× frontier for splitting logic + 12× student for leaf execution |

The key insight: **most agent work within a fleet is repetitive pattern execution, not novel reasoning.** Nemosis captures the patterns; students replay them. The teacher only fires when the student encounters something outside its distilled skill set.

### The Nemosis Workflow Inside ClawDen

1. **Capture** — When a fleet executes a coordination pattern (swarm, adversarial, etc.), the process supervisor (spec 064) captures high-fidelity execution traces from all participating agents.
2. **Distill** — Nemosis's Rust-based Distiller analyzes the traces and synthesizes a `SKILL.md` per agent role, collapsing multi-turn reasoning chains into declarative skill contracts.
3. **Osmose** — On subsequent fleet invocations, the scheduler checks the skill registry. If a distilled skill exists for the agent role, it assigns a student model pre-loaded with the `SKILL.md` instead of the frontier teacher.
4. **Audit** — The teacher model periodically evaluates student outputs against quality thresholds. If quality drifts below tolerance, Nemosis triggers a re-distillation cycle, refining the skill with new traces.

## Design

### Architecture: Fleet-Nemosis Integration

Nemosis operates as a **sidecar service** within the fleet execution layer, interacting with three existing subsystems:

```
┌─────────────────────────────────────────────────────────┐
│                  Fleet Execution Layer                   │
│                                                         │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  │
│  │   Process     │  │  Message     │  │    State     │  │
│  │  Supervisor   │  │    Bus       │  │ Persistence  │  │
│  │  (spec 064)   │  │  (spec 065)  │  │  (spec 066)  │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  │
│         │                 │                  │          │
│         ▼                 ▼                  ▼          │
│  ┌─────────────────────────────────────────────────┐    │
│  │           Nemosis Integration Layer              │    │
│  │                                                  │    │
│  │  ┌──────────┐ ┌──────────┐ ┌────────────────┐   │    │
│  │  │  Trace   │ │Distiller │ │ Skill Registry │   │    │
│  │  │ Capture  │ │  (Rust)  │ │  (SKILL.md DB) │   │    │
│  │  └──────────┘ └──────────┘ └────────────────┘   │    │
│  │  ┌──────────┐ ┌──────────┐ ┌────────────────┐   │    │
│  │  │  Audit   │ │ Cost     │ │ Memory Layer   │   │    │
│  │  │  Loop    │ │ Tracker  │ │ (iter. refine) │   │    │
│  │  └──────────┘ └──────────┘ └────────────────┘   │    │
│  └─────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────┘
```

### Trace Capture

The process supervisor already manages agent lifecycle (spawn, health, restart). Nemosis hooks into the agent stdout/stdin JSON-Lines pipe to capture execution traces:

```rust
struct ExecutionTrace {
    /// Unique trace ID
    trace_id: TraceId,
    /// The agent role within the coordination pattern
    agent_role: AgentRole,
    /// The coordination primitive being executed
    primitive: PrimitiveKind,
    /// Ordered sequence of reasoning steps
    steps: Vec<TraceStep>,
    /// Model used for this execution
    model: ModelId,
    /// Total token consumption
    token_usage: TokenUsage,
    /// Quality score (from convergence, adversarial rounds, etc.)
    quality_score: Option<f64>,
}

struct TraceStep {
    /// Input context provided to the agent
    input: serde_json::Value,
    /// The agent's reasoning (chain-of-thought if available)
    reasoning: Option<String>,
    /// Output produced
    output: serde_json::Value,
    /// Tool calls made during this step
    tool_calls: Vec<ToolCall>,
    /// Duration of this step
    duration: Duration,
}
```

Traces are persisted via spec 066's SQLite backend in a `traces` table, linked to the fleet run and agent ID.

### Distillation Pipeline

The Distiller consumes traces and produces `SKILL.md` artifacts:

1. **Pattern extraction** — analyzes N traces for the same agent role, identifies invariant reasoning patterns (the "logic collapse" step from Nemosis).
2. **Schema synthesis** — produces a JSON Schema capturing the input/output contract of the agent role.
3. **Context generation** — produces markdown strategic context: when to apply the pattern, edge cases observed in traces, quality heuristics.
4. **Validation** — runs the synthesized skill through a test suite derived from the original traces, ensuring the student model can replicate >95% of teacher outputs.

```rust
trait Distiller {
    /// Analyze traces and produce a distilled skill
    fn distill(&self, traces: &[ExecutionTrace]) -> Result<DistilledSkill>;

    /// Validate a skill against held-out traces
    fn validate(&self, skill: &DistilledSkill, test_traces: &[ExecutionTrace]) -> ValidationResult;

    /// Refine an existing skill with new traces (iterative improvement)
    fn refine(&self, skill: &DistilledSkill, new_traces: &[ExecutionTrace]) -> Result<DistilledSkill>;
}

struct DistilledSkill {
    /// The agent role this skill covers
    role: AgentRole,
    /// The coordination primitive context
    primitive: PrimitiveKind,
    /// Generated SKILL.md content
    skill_md: String,
    /// Input/output JSON Schema
    contract: serde_json::Value,
    /// Recommended student model tier
    recommended_model: ModelTier,
    /// Quality threshold below which re-distillation triggers
    quality_floor: f64,
    /// Number of traces used in distillation
    trace_count: usize,
    /// Distillation generation (increments on each refine cycle)
    generation: u32,
}

enum ModelTier {
    /// Frontier models (Claude Opus, GPT-4)
    Frontier,
    /// Mid-tier models (Claude Sonnet, GPT-4o)
    MidTier,
    /// Cost-optimized models (Claude Haiku, GPT-4o-mini)
    Student,
}
```

### Memory Layer & Iterative Refinement

The memory layer is what separates Nemosis from one-shot distillation. It maintains a persistent knowledge base per agent role:

- **Success memory** — patterns that consistently produce high-quality outputs. These get baked into the `SKILL.md` as primary instructions.
- **Failure memory** — patterns that led to quality drops or audit failures. These become negative examples and guardrails in the skill.
- **Edge-case memory** — inputs that triggered teacher escalation. Accumulating edge cases eventually triggers a re-distillation cycle that folds them into the student skill.
- **Drift detection** — tracks quality scores over time. When the moving average drops below `quality_floor`, the audit loop triggers re-distillation with accumulated new traces.

```yaml
fleet:
  nemosis:
    enabled: true
    trace_capture: true
    distillation:
      min_traces_for_distill: 10
      validation_split: 0.2
      quality_threshold: 0.95
    memory:
      backend: sqlite
      success_retention: 1000
      failure_retention: 500
      edge_case_retention: 200
      drift_window: 50
    audit:
      interval: 100  # every 100 fleet runs
      teacher_model: claude-opus
      quality_floor: 0.90
      auto_redistill: true
    cost:
      track_savings: true
      report_interval: daily
```

### Scheduler Integration

The fleet scheduler (spec 064's process supervisor + spec 065's task orchestration) gains a **model selection** step:

1. Before spawning an agent for a coordination primitive, check the skill registry for a matching `(role, primitive)` pair.
2. If a distilled skill exists with `quality_score >= quality_floor`:
   - Spawn a student-tier model with the `SKILL.md` pre-loaded.
   - Attach a lightweight quality monitor that samples outputs.
3. If no skill exists, or quality is below floor:
   - Spawn the frontier teacher model.
   - Enable trace capture for future distillation.
4. For novel tasks (first execution of a new pattern):
   - Always use teacher model.
   - Capture traces from the start.

### Applied to Coordination Primitives

**Speculative swarm cost optimization:**
- First run: all 8 forks use frontier teacher. Total cost: 8× teacher.
- After 10 runs: Nemosis distills skills for the 3 most common strategy roles. Those forks switch to students. Cost: 5× teacher + 3× student ≈ 60% reduction.
- After 50 runs: only 1–2 "exploration" forks remain on teacher for genuinely novel strategies. Cost: 2× teacher + 6× student ≈ 80% reduction.

**Stigmergic maintenance cost optimization:**
- Staleness detector, relationship updater, token watchdog — highly repetitive patterns perfect for distillation.
- After initial training period: all 5 watcher agents run on student models. Teacher fires only for anomalous code changes that don't match any distilled pattern.
- Steady-state cost: ~90% reduction from frontier pricing.

**Generative-adversarial cost optimization:**
- Early rounds (syntax, edge-cases) are highly patternable — distill to students.
- Later rounds (adversarial fuzzing, concurrency attacks) retain teacher for creativity.
- Typical split: rounds 1–3 on students, rounds 4+ on teacher. ~50% cost reduction.

## Plan

- [ ] Define the `ExecutionTrace` and `TraceStep` data model in `clawden-core`.
- [ ] Add trace capture hooks to the process supervisor's agent stdout/stdin pipe.
- [ ] Implement the `traces` table in the SQLite persistence layer (spec 066).
- [ ] Integrate Nemosis's Rust Distiller as a library dependency or subprocess.
- [ ] Build the skill registry (SKILL.md storage + quality metadata + model tier mapping).
- [ ] Implement the memory layer (success/failure/edge-case/drift tracking).
- [ ] Add model selection logic to the fleet scheduler.
- [ ] Implement the audit loop (periodic teacher evaluation + auto-redistillation).
- [ ] Add cost tracking and reporting (per-run savings, cumulative reduction).
- [ ] Wire fleet config (`fleet.nemosis.*`) into ClawDen's config management.

## Test

- [ ] A teacher agent's execution traces are captured and persisted correctly.
- [ ] The Distiller produces a valid `SKILL.md` from ≥10 traces for a given agent role.
- [ ] A student model loaded with a distilled skill produces outputs within 95% quality of the teacher on held-out traces.
- [ ] The scheduler correctly routes to student models when a qualifying skill exists.
- [ ] The audit loop detects quality drift and triggers re-distillation.
- [ ] Cost tracking reports accurate per-run and cumulative savings.
- [ ] The memory layer accumulates edge cases and folds them into refined skills.

## Notes

- Nemosis is at initial-commit stage (`codervisor/nemosis`). Integration depends on its Distiller API stabilizing. The `Distiller` trait in this spec may need to adapt to Nemosis's actual interface.
- The `SKILL.md` format follows the [AgentSkills.io](https://agentskills.io/) specification, which ClawDen already uses for its own skill infrastructure.
- This spec intentionally avoids mandating *which* student/teacher models to use — that's a config decision. The architecture is model-agnostic.
- Relationship to spec 025 (LLM Provider API Key Management): the scheduler's model selection needs access to multiple provider API keys to route between teacher and student tiers.
- Distributed fleet scenario (spec 062): trace capture works the same way over remote agent control channels — the `AgentEnvelope` protocol already carries agent outputs that include the traces.

## Plan

<!-- Break down implementation into steps -->

<!-- 💡 TIP: If your plan has >6 phases or this spec approaches 
     400 lines, consider using sub-spec files:
     - IMPLEMENTATION.md for detailed implementation
     - See spec 012-sub-spec-files for guidance on splitting -->

- [ ] Task 1
- [ ] Task 2
- [ ] Task 3

## Test

<!-- How will we verify this works? -->

- [ ] Test criteria 1
- [ ] Test criteria 2

## Notes

<!-- Optional: Research findings, alternatives considered, open questions -->
