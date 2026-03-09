---
status: planned
created: 2026-03-09
priority: medium
tags:
- fleet
- orchestration
- coordination
- enterprise
depends_on:
- 065-agent-message-bus-task-orchestration
parent: 054-agent-fleet-execution-layer
created_at: 2026-03-09T06:10:00.624611698Z
updated_at: 2026-03-09T06:10:00.624611698Z
---

# Advanced Coordination Patterns — Enterprise-Scale Agent Organization

## Overview

Spec 065 establishes master-worker as the foundational coordination pattern — a single leader delegates subtasks to flat workers and aggregates results. This is intuitive for small teams but doesn't model the richer organizational structures found in real enterprises.

This spec defines pluggable coordination patterns that map to how real organizations work: hierarchical delegation, cross-department collaboration, peer consensus, assembly-line pipelines, and market-driven task allocation.

Child spec `068-ai-native-coordination-primitives` extends these with patterns that have no human organizational analogue — exploiting zero fork cost, lossless context transfer, and speculative parallelism to achieve outcomes impossible with human teams.

All patterns build on the same `AgentEnvelope` protocol and `MessageBus` from spec 065 — they are coordination strategies, not transport changes.

## Design

### Coordination Pattern Trait

Abstract the coordination logic into a pluggable trait so the task engine is generic over patterns:

```rust
trait CoordinationPattern {
    /// Decompose a task into work units for the available agents
    fn decompose(&self, task: &Task, agents: &[AgentInfo]) -> Vec<WorkUnit>;
    /// Route a work unit to the appropriate agent(s)
    fn route(&self, unit: &WorkUnit, agents: &[AgentInfo]) -> Vec<AgentId>;
    /// Process a completed work unit result and decide next steps
    fn on_result(&mut self, result: &TaskResult) -> CoordinationAction;
    /// Check if the overall task is complete
    fn is_complete(&self) -> bool;
}

enum CoordinationAction {
    /// Forward result to next stage/agent
    Forward { to: AgentId, payload: TaskAssignment },
    /// Broadcast result to a set of agents for review/voting
    Broadcast { to: Vec<AgentId>, payload: AgentEnvelope },
    /// Aggregate into final result
    Aggregate,
    /// Wait for more results before deciding
    Wait,
    /// Escalate to a higher-level coordinator
    Escalate { to: AgentId },
}
```

### Pattern 1: Hierarchical Delegation

Recursive master-worker. A leader delegates to sub-leaders who further delegate to their workers, forming a tree.

- `TaskAssignment` gains a `parent_task_id` field to form task trees.
- Sub-leaders are agents that have both a leader role (can decompose/delegate) and a worker role (receive tasks from above).
- Result aggregation bubbles up: leaf workers → sub-leaders → top leader.
- Depth limit in config to prevent unbounded recursion.

```yaml
fleet:
  teams:
    engineering:
      leader: architect
      sub_teams:
        frontend:
          leader: frontend-lead
          workers: [ui-agent, css-agent]
        backend:
          leader: backend-lead
          workers: [api-agent, db-agent]
      coordination: hierarchical
      max_depth: 3
```

### Pattern 2: Pipeline (Assembly Line)

Sequential stages where output of one agent/team feeds into the next. Each stage can internally use master-worker parallelism.

- Task engine maintains a `stages: Vec<PipelineStage>` with ordered execution.
- Each stage defines input/output schema for type-safe handoff.
- Stage failure halts the pipeline (with configurable retry or skip).
- Supports conditional branching: stage output can route to different next stages.

```yaml
fleet:
  pipelines:
    code-review:
      stages:
        - name: plan
          agent: planner
        - name: implement
          team: coding-team
          coordination: master-worker
        - name: review
          agent: reviewer
        - name: merge
          agent: merger
          condition: "review.approved == true"
```

### Pattern 3: Committee (Peer Consensus)

Equal-rank agents deliberate collectively. Unlike blind majority-vote (spec 065), agents see each other's responses and can iterate.

- Rounds-based: each round, all committee members submit a response.
- After each round, all responses are broadcast to all members.
- Members can revise their position in the next round.
- Termination: unanimous agreement, quorum threshold, or max rounds reached.
- Useful for code review panels, architectural decisions, quality assessment.

```yaml
fleet:
  teams:
    review-board:
      members: [reviewer-1, reviewer-2, reviewer-3]
      coordination: committee
      consensus:
        strategy: quorum
        quorum_threshold: 0.66
        max_rounds: 3
```

### Pattern 4: Departmental (Cross-Team Routing)

Multiple specialist teams with a top-level router that directs entire task categories to the right department. Departments have independent internal coordination.

- A router agent (or rule-based classifier) assigns tasks to departments.
- Each department is a self-contained team with its own coordination pattern.
- Inter-department communication goes through department gateway agents.
- Supports escalation paths when a department can't handle a task.

```yaml
fleet:
  departments:
    research:
      team: research-team
      capabilities: [web-search, analysis, summarization]
    engineering:
      team: coding-team
      capabilities: [code, test, deploy]
    qa:
      team: qa-team
      capabilities: [review, test, security-audit]
  router:
    agent: dispatcher
    strategy: capability-match
```

### Pattern 5: Marketplace (Task Bidding)

Inverts control flow — instead of top-down assignment, tasks are posted and agents bid.

- Task posted to a job board with requirements (capabilities, deadline, priority).
- Qualified agents submit bids (estimated time, confidence, cost).
- Allocation strategy selects the winner: lowest-cost, highest-confidence, fastest.
- Agents can decline or timeout, triggering re-auction.
- Self-organizing: no explicit leader needed.

```yaml
fleet:
  marketplace:
    agents: [agent-1, agent-2, agent-3, agent-4]
    coordination: marketplace
    allocation: lowest-cost
    bid_timeout_seconds: 10
```

### Pattern 6: Matrix (Multi-Team Membership)

Agents belong to multiple teams simultaneously with dynamic role switching.

- Agent declares available capacity across teams (e.g., 60% frontend, 40% review).
- Scheduling layer resolves conflicts when multiple teams claim the same agent.
- Priority-based preemption: higher-priority team can claim agent from lower-priority work.
- Avoids deadlocks via capacity reservation and timeout-based release.

## Plan

- [ ] Define `CoordinationPattern` trait and `CoordinationAction` enum.
- [ ] Refactor spec 065's master-worker logic to implement `CoordinationPattern`.
- [ ] Implement hierarchical delegation with `parent_task_id` and depth limiting.
- [ ] Implement pipeline coordination with stage sequencing and conditional branching.
- [ ] Implement committee consensus with round-based deliberation.
- [ ] Implement departmental routing with capability-based team dispatch.
- [ ] Implement marketplace bidding with configurable allocation strategies.
- [ ] Implement matrix multi-team membership with capacity scheduling.
- [ ] Extend `clawden.yaml` fleet config to support all organizational pattern types.
- [ ] Implement pattern composability — sub-patterns within pipeline stages and nested coordination.
- [ ] Add `clawden fleet patterns` command to list and describe available patterns.
- [ ] Design trait extension points for AI-native patterns (spec 068) from day one.

## Test

- [ ] Master-worker from spec 065 works identically when expressed as a `CoordinationPattern`.
- [ ] Hierarchical: 3-level tree delegates and aggregates correctly; depth limit enforced.
- [ ] Pipeline: 4-stage pipeline passes results sequentially; conditional branch skips a stage.
- [ ] Committee: 3 agents reach quorum after 2 rounds of deliberation.
- [ ] Departmental: router sends coding task to engineering, research task to research team.
- [ ] Marketplace: 3 agents bid on a task; lowest-cost wins; declined bid triggers re-auction.
- [ ] Matrix: agent serving two teams respects capacity split; preemption works on priority.
- [ ] Invalid config (cycle in hierarchy, pipeline with no stages) rejected at parse time.
- [ ] `CoordinationPattern` trait is extensible: adding a new pattern requires only a new impl, no changes to the engine.
