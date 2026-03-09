---
status: planned
created: 2026-03-09
priority: medium
tags:
- fleet
- orchestration
- coordination
- ai-native
- playbooks
- domains
parent: 054-agent-fleet-execution-layer
depends_on:
  - 068-ai-native-coordination-primitives
created_at: 2026-03-09T08:16:40.771740021Z
updated_at: 2026-03-09T08:16:40.771740021Z
---

# AI-Native Coordination Applied — Domain Playbooks

## Overview

Spec 068 defines five coordination primitives (speculative swarm, context mesh, fractal decomposition, generative-adversarial, stigmergic) grounded in what AI agents can do that humans structurally cannot. But primitives alone don't ship value — **applied compositions** do.

This spec maps AI-native primitives to concrete domain workflows across coding, finance, marketing, research, legal, and ops. Each playbook answers three questions:

1. **What does this domain do today?** The human+software status quo.
2. **What's the "motor on a horse carriage" version?** Agent fleet mimicking the existing workflow (faster, but same model).
3. **What's the "assembly line" version?** A fundamentally new production model only possible because agents can fork, observe, merge, and self-organize.

The gap between (2) and (3) is where the real value lives — and it's the gap most AI-agent products never cross because they stop at (2).

## Design

### Domain 1: Software Engineering

**Status quo:** Developer writes code → opens PR → reviewer reads diff → comments → author fixes → merge. Sequential, single-threaded, lossy handoffs. Review depth limited by reviewer's time and cognitive load.

**Horse carriage (org-chart agents):** Coding agent writes code → reviewer agent reads diff → posts comments → coding agent fixes. Same pipeline, faster typing speed. The bottleneck (sequential ideation and shallow review) is preserved.

**Assembly line (AI-native):**

**Speculative Swarm + Generative-Adversarial + Stigmergic** — composing three primitives:

1. **Feature implementation via speculative swarm.** Given a spec, fork 4 agents: one approaches via TDD, one via prototype-then-refactor, one via type-driven design, one via existing-pattern-extension. At checkpoints, cross-pollinate — the TDD fork adopts a clean abstraction from the type-driven fork; the prototype fork steals a test case from the TDD fork. Fragment-fuse the survivors into a final implementation that combines the best architectural choices from each approach. *No human team can execute 4 strategies simultaneously and cherry-pick the best parts of each execution.*

2. **Hardening via generative-adversarial.** The fused implementation enters an adversarial loop. The critic doesn't just read code — it generates adversarial inputs, constructs concurrency interleavings, attempts to violate invariants programmatically. Each round escalates: syntax → edge cases → race conditions → adversarial fuzzing. The code that survives 6 rounds of escalating attack is qualitatively different from code that passed a human review. *No human reviewer can programmatically generate adversarial inputs and increase attack sophistication each round.*

3. **Living codebase via stigmergic maintenance.** Post-merge, agents continuously watch the codebase. A dependency upgrade triggers security-auditor. A new public function triggers test-generator and doc-writer. A performance regression marker triggers profiler-agent. No ticket filed, no standup, no sprint planning. The codebase maintains itself through reactive artifact observation. *No human team can continuously monitor every file change and react in real-time.*

```yaml
fleet:
  playbooks:
    software-engineering:
      implementation:
        primitive: speculative-swarm
        strategies: [tdd, prototype-refactor, type-driven, pattern-extension]
        merge: fragment-fusion
        budget: { max_cost_usd: 5.00 }
      hardening:
        primitive: generative-adversarial
        escalation: [syntax, edge-cases, concurrency, adversarial-fuzz]
        termination: { consecutive_clean_rounds: 2 }
      maintenance:
        primitive: stigmergic
        agents: [security-auditor, test-generator, doc-writer, profiler]
        marker_decay: 7200s
```

**The new production model:** Code isn't "written and reviewed" — it's *evolved through speculative exploration, stress-tested through adversarial escalation, and maintained through autonomous stigmergic reaction.* The human's role shifts from author/reviewer to spec-writer and taste arbiter.

---

### Domain 2: Financial Analysis & Trading Strategy

**Status quo:** Analyst reads filings → builds model in Excel → presents to PM → PM makes trade decision. Deeply sequential. One analyst, one model, one perspective. Backtesting is afterthought.

**Horse carriage:** Agent reads filings → builds model → another agent reviews model. Same pipeline, more data ingested. Still one model, still one perspective.

**Assembly line:**

**Speculative Swarm + Context Mesh + Fractal Decomposition:**

1. **Multi-thesis exploration via speculative swarm.** Given an investment target, fork 6 agents: one builds a DCF model, one does comparable analysis, one analyzes insider trading patterns, one runs sentiment analysis on earnings calls, one stress-tests via Monte Carlo simulation, one constructs a bear case from short-seller reports. Cross-pollinate at checkpoints — the DCF fork adjusts growth assumptions after seeing the sentiment fork's findings; the bear-case fork incorporates the Monte Carlo fork's tail-risk distribution. Fragment-fuse into a multi-perspective thesis that no single analyst could produce. *No human analyst can simultaneously execute 6 analytical frameworks and revise each framework based on real-time findings from the other 5.*

2. **Market state via context mesh.** A shared knowledge graph continuously ingests: price feeds, filing events, macro indicators, sector rotations, option flow, news events. Agents don't query — they subscribe to relevant nodes. When a filing drops, the fundamentals agent is notified. When implied volatility spikes, the options agent is notified. When a macro indicator changes, all sector agents receive the delta. Knowledge gaps (e.g., "we have revenue growth but no margin forecast") are detected and claimed automatically.

3. **Deep-dive via fractal decomposition.** The DCF agent, when analyzing a conglomerate, fractal-splits into per-segment agents — each inherits full context of the parent's model but narrows scope to one business segment. Sub-agents may further split for geographic breakdown. Reunification produces a sum-of-the-parts valuation with segment-level granularity, all losslessly integrated because the sub-agents ARE the parent.

```yaml
fleet:
  playbooks:
    financial-analysis:
      thesis:
        primitive: speculative-swarm
        strategies: [dcf, comps, insider-patterns, sentiment, monte-carlo, bear-case]
        checkpoint_interval: 60s
        convergence_threshold: 0.7
      market-state:
        primitive: context-mesh
        sources: [price-feed, filings, macro, options-flow, news]
        propagation: reactive
      deep-dive:
        primitive: fractal-decomposition
        split_strategy: per-segment
        max_depth: 3
```

**The new production model:** Investment analysis isn't "one analyst builds one model" — it's a *multi-thesis swarm that stress-tests itself, fed by a reactive knowledge mesh, with fractal depth on complex entities.* The human's role shifts from model-builder to thesis curator and risk-appetite setter.

---

### Domain 3: Marketing & Content Operations

**Status quo:** Content team brainstorms → writer drafts → editor reviews → designer layouts → PM schedules. Linear pipeline. Each piece is one shot. Performance feedback arrives weeks later and rarely loops back into creation.

**Horse carriage:** Agent drafts → another agent edits → another creates images. Same pipeline, more throughput.

**Assembly line:**

**Speculative Swarm + Generative-Adversarial + Stigmergic + Context Mesh:**

1. **Multi-angle creation via speculative swarm.** Given a brief ("launch blog post for feature X"), fork agents with divergent creative strategies: one writes thought-leadership angle, one writes technical deep-dive, one writes customer-story framing, one writes contrarian/provocative take, one writes SEO-optimized listicle. At checkpoints, cross-pollinate narrative hooks and data points. Fragment-fuse: the final piece might use the thought-leadership structure, the technical deep-dive's evidence, and the contrarian take's opening hook. *No human writer can execute 5 creative directions simultaneously and cherry-pick the most effective elements from each.*

2. **Audience-adversarial testing.** Each content variant enters an adversarial loop with a "skeptical reader" critic agent. The critic doesn't check grammar — it models audience objections: "Why should I care?", "This claim is unsubstantiated", "I've heard this before", "This doesn't address my use case." The generator must address each objection, and the critic escalates from surface-level to deep skepticism. *No human editor can systematically model 4 audience personas and escalate objection sophistication each round.*

3. **Performance-reactive content loop via stigmergic + context mesh.** Post-publish, analytics events (page views, scroll depth, conversion, social shares) flow into the context mesh. When engagement drops below threshold, a content-refresher agent detects the marker decay and rewrites underperforming sections. When a competitor publishes on the same topic, a competitive-monitor agent detects it and updates the content mesh, triggering differentiation recommendations. When a new product feature ships, a feature-content agent detects the artifact change and drafts update content. *No human team can reactively update content based on real-time performance signals and competitive moves.*

4. **Campaign fractal decomposition.** An integrated campaign (product launch) fractal-splits: the parent agent decomposes into blog-agent, email-agent, social-agent, landing-page-agent, ad-copy-agent. Each inherits the full campaign context (positioning, target audience, key messages) but narrows to its channel. Reunification ensures consistent messaging across all channels — not because of a "brand guide document" but because all children ARE the parent. *No human team achieves true cross-channel consistency; fractal decomposition does it structurally.*

```yaml
fleet:
  playbooks:
    marketing:
      creation:
        primitive: speculative-swarm
        strategies: [thought-leadership, technical, customer-story, contrarian, seo-listicle]
        merge: fragment-fusion
      validation:
        primitive: generative-adversarial
        critic_modes: [relevance, evidence, originality, persona-fit]
        max_rounds: 5
      performance-loop:
        primitives: [stigmergic, context-mesh]
        sources: [analytics, competitor-feed, product-changelog]
        marker_decay: 86400s
      campaign:
        primitive: fractal-decomposition
        split_strategy: per-channel
        channels: [blog, email, social, landing-page, ad-copy]
```

**The new production model:** Content isn't "created, reviewed, published" — it's *speculatively explored, adversarially stress-tested against audience models, and continuously self-optimized through stigmergic reaction to performance signals.* The human's role shifts from writer/editor to creative director and brand guardian.

---

### Domain 4: Scientific Research & Literature Synthesis

**Status quo:** Researcher reads papers → takes notes → synthesizes → writes review → peer review → revision. Months per cycle. One researcher's perspective. Literature coverage limited by reading speed.

**Horse carriage:** Agent reads papers faster → summarizes → researcher synthesizes. Same bottleneck: synthesis is still single-threaded and single-perspective.

**Assembly line:**

**Context Mesh + Fractal Decomposition + Speculative Swarm:**

1. **Living literature graph via context mesh.** Instead of sequential paper reading, build a shared knowledge graph where every paper is decomposed into claims, methods, results, and limitations — as graph nodes with dependency edges. Agents continuously ingest new papers (arxiv feeds, citation alerts) and integrate them into the graph. When a new paper contradicts an existing claim, the conflict is surfaced automatically. When a gap is detected ("claim X cites method Y, but no paper in the graph validates Y on dataset Z"), a gap-filling agent claims it and searches for evidence. *No human researcher can maintain a live, conflict-detecting, gap-identifying knowledge graph across thousands of papers.*

2. **Deep paper analysis via fractal decomposition.** A seminal paper with multiple contributions gets fractal-split: the parent agent decomposes into methods-agent, theory-agent, experiments-agent, limitations-agent. Each inherits full paper context but narrows to its section. The methods-agent may further split to analyze each sub-method. Reunification produces a structured critique with depth impossible in a single pass. *No human reviewer can simultaneously analyze methods, theory, and experiments with full cross-referencing between sections.*

3. **Hypothesis generation via speculative swarm.** Given the literature graph's current state (including gaps and contradictions), fork agents to generate research hypotheses from different angles: one extrapolates trends, one combines methods from disjoint fields, one inverts assumptions in existing work, one identifies unexplained experimental anomalies. Cross-pollinate and fragment-fuse into a ranked hypothesis set. *No human researcher can simultaneously pursue multiple creative hypothesis-generation strategies across the full literature graph.*

```yaml
fleet:
  playbooks:
    research:
      literature:
        primitive: context-mesh
        sources: [arxiv-feed, citation-alerts, manual-papers]
        graph_nodes: [claims, methods, results, limitations]
        conflict: auto-detect-and-surface
      deep-analysis:
        primitive: fractal-decomposition
        split_strategy: per-contribution
        max_depth: 3
      hypothesis:
        primitive: speculative-swarm
        strategies: [trend-extrapolation, cross-field-synthesis, assumption-inversion, anomaly-detection]
        merge: ranked-list
```

---

### Domain 5: Legal Contract Analysis

**Status quo:** Associate reads contract → flags risks → partner reviews → negotiation. Associates miss things (fatigue, inexperience). Partners gate knowledge. Cross-contract pattern detection doesn't happen.

**Horse carriage:** Agent reads contract faster → flags risks. Same model: one-pass, one-reviewer, no cross-contract learning.

**Assembly line:**

**Fractal Decomposition + Generative-Adversarial + Context Mesh:**

1. **Contract fractal analysis.** A complex agreement fractal-splits: parent decomposes into indemnification-agent, IP-assignment-agent, termination-agent, liability-cap-agent, governing-law-agent. Each inherits full contract context (definitions, recitals, parties) but narrows to its clause cluster. Reunification produces a risk matrix with clause-level granularity and cross-clause interaction analysis (e.g., "the indemnification clause is unlimited but the liability cap is $1M — these conflict"). *No junior associate can simultaneously analyze all clause clusters with full awareness of cross-clause interactions.*

2. **Adversarial clause testing.** Each flagged clause enters an adversarial loop. The critic agent constructs worst-case scenarios: "Under this indemnification clause, if event X occurs, your maximum exposure is Y. Here's the chain of defined terms that leads to that conclusion." Each round escalates: obvious risks → multi-clause interaction risks → jurisdiction-specific interpretation risks → regulatory change risks. *No human reviewer can systematically construct worst-case scenario chains across all clause interactions.*

3. **Cross-contract pattern mesh.** A context mesh across all previously analyzed contracts detects patterns: "This non-compete clause is stricter than 90% of comparable agreements in this jurisdiction." When a new contract is added, its clausees automatically update the mesh, and anomaly detection flags outliers. *No law firm can maintain a live, queryable knowledge graph of all clause patterns across their entire contract corpus.*

```yaml
fleet:
  playbooks:
    legal:
      analysis:
        primitive: fractal-decomposition
        split_strategy: per-clause-cluster
        max_depth: 2
      stress-test:
        primitive: generative-adversarial
        critic_modes: [obvious-risk, cross-clause, jurisdiction, regulatory]
        max_rounds: 6
      pattern-detection:
        primitive: context-mesh
        corpus: all-analyzed-contracts
        anomaly: percentile-based
```

---

### Domain 6: DevOps & Incident Response

**Status quo:** Alert fires → on-call triages → escalates → multiple engineers join war room → someone finds root cause → fix → post-mortem. Sequential, high-latency, knowledge siloed in whoever happened to respond.

**Horse carriage:** Agent monitors alerts → triages → suggests fix. Faster triage, same sequential root-cause hunt.

**Assembly line:**

**Speculative Swarm + Stigmergic + Context Mesh:**

1. **Multi-hypothesis root cause via speculative swarm.** On alert, immediately fork agents pursuing different root-cause hypotheses: one checks recent deploys, one analyzes log patterns, one checks infrastructure metrics, one reviews dependency health, one searches for similar past incidents. Cross-pollinate: the deploy-checker shares a suspicious commit → the log-analyzer confirms correlated error spike → convergence prunes the infrastructure and dependency hypotheses. Resolution in minutes, not hours. *No war room can simultaneously investigate 5 hypotheses with real-time cross-hypothesis evidence sharing.*

2. **Continuous system health via stigmergic monitoring.** Agents watch their artifact domains (metrics, logs, deploy events, config changes, certificate expirations, dependency CVEs) and react to changes. A new CVE triggers the security-agent to assess exposure. A certificate nearing expiry triggers the cert-agent. A memory trend triggers the capacity-agent. No PagerDuty rules, no runbook — emergent reactive monitoring. *No SRE team can reactively monitor all artifact types and auto-remediate without explicit alerting rules.*

3. **Incident knowledge via context mesh.** Every incident contributes its root cause, symptoms, resolution, and related systems to a shared knowledge graph. When a new incident occurs, agents immediately surface relevant past incidents and proven remediations. Knowledge doesn't live in one engineer's head or a dusty wiki — it's a live, searchable, reactive graph.

```yaml
fleet:
  playbooks:
    incident-response:
      root-cause:
        primitive: speculative-swarm
        strategies: [recent-deploys, log-patterns, infra-metrics, dependency-health, past-incidents]
        convergence_threshold: 0.8
      monitoring:
        primitive: stigmergic
        agents: [security, cert, capacity, deploy-watcher, config-drift]
        reaction_debounce: 10s
      knowledge:
        primitive: context-mesh
        sources: [incident-history, runbooks, architecture-docs]
        propagation: reactive
```

### Cross-Domain Pattern

Across all six domains, the same structural shift emerges:

| Domain | Old model (human-shaped) | New model (AI-native) | Key primitive |
|---|---|---|---|
| **Coding** | Write → review → merge | Speculative explore → adversarial harden → stigmergic maintain | Swarm + GAN + Stigmergy |
| **Finance** | One analyst, one model | Multi-thesis swarm, live market mesh, fractal deep-dive | Swarm + Mesh + Fractal |
| **Marketing** | Brief → draft → edit → publish | Multi-angle swarm → audience-adversarial test → performance-reactive loop | Swarm + GAN + Stigmergy |
| **Research** | Read → synthesize → write | Live literature mesh → fractal analysis → hypothesis swarm | Mesh + Fractal + Swarm |
| **Legal** | Read → flag risks → review | Fractal clause analysis → adversarial stress-test → cross-corpus mesh | Fractal + GAN + Mesh |
| **DevOps** | Alert → triage → escalate → fix | Multi-hypothesis swarm → stigmergic monitoring → incident mesh | Swarm + Stigmergy + Mesh |

The human role in every domain shifts from **executor** to **director**: setting objectives, defining taste, and making judgment calls that require values — not labor.

## Plan

- [ ] Define `Playbook` config schema that maps domain workflows to primitive compositions.
- [ ] Implement playbook loader: parse YAML playbook definition → instantiate primitive pipeline.
- [ ] Build software-engineering playbook template (swarm → adversarial → stigmergic).
- [ ] Build financial-analysis playbook template (swarm + mesh + fractal).
- [ ] Build marketing playbook template (swarm → adversarial + stigmergic + fractal).
- [ ] Build research playbook template (mesh + fractal + swarm).
- [ ] Build legal playbook template (fractal → adversarial + mesh).
- [ ] Build incident-response playbook template (swarm + stigmergic + mesh).
- [ ] Add `clawden fleet playbook list` and `clawden fleet playbook describe <name>` CLI commands.
- [ ] Document composability patterns: which primitives compose well and which combinations are anti-patterns.

## Test

- [ ] Software engineering playbook: speculative swarm produces 4 implementation variants; fragment fusion selects best architecture + best test coverage; adversarial loop finds and fixes 2 edge cases; stigmergic agent auto-generates docs for new public API.
- [ ] Financial analysis playbook: speculative swarm produces 6 model variants; context mesh ingests a filing mid-analysis and propagates to all forks; fractal agent decomposes a conglomerate into 3 segment sub-agents.
- [ ] Marketing playbook: speculative swarm produces 5 content variants; adversarial critic models 3 audience personas; fragment fusion combines the strongest hook + evidence + structure across variants.
- [ ] Research playbook: context mesh detects contradiction between two papers; fractal analysis of a seminal paper produces per-section structured critique; hypothesis swarm generates 4 ranked hypotheses from mesh state.
- [ ] Legal playbook: fractal decomposition splits contract into 5 clause-cluster agents; adversarial agent constructs worst-case scenario chain spanning 3 defined terms; context mesh flags anomalous clause vs. corpus baseline.
- [ ] Incident response playbook: speculative swarm converges on root cause in <5 minutes simulated time; stigmergic agent detects CVE and auto-files remediation task; context mesh surfaces similar past incident.
- [ ] Invalid playbook config (referencing unknown primitive, circular composition) rejected at parse time.
- [ ] Budget enforcement: playbook respects per-step and total budget limits; exceeding mid-playbook triggers graceful degradation.

## Notes

These playbooks are **templates, not products.** ClawDen provides the primitives and the composition engine; users customize playbooks for their specific domain workflows, agent configurations, and quality thresholds.

The boundary with spec 068: that spec owns the five AI-native primitives and their implementation. This spec owns the domain-specific compositions and the playbook config schema. A playbook is a recipe; the primitives are the ingredients.