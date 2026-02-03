---
status: planned
created: 2026-02-03
priority: high
parent: 001-clawlab-mvp
tags:
- ai
- prompts
- llm
depends_on:
- 003-vision-agent
created_at: 2026-02-03T08:59:22.375298506Z
updated_at: 2026-02-03T08:59:37.505695550Z
---

# Vision LLM Prompt Engineering

## Overview

Define the prompt templates, output schemas, and parsing logic for the Vision LLM that drives browser automation decisions.

## Design

### Core Files
- `src/agent/prompts/system.ts` - System prompt template
- `src/agent/prompts/action.ts` - Action request prompt
- `src/agent/prompts/parser.ts` - Response parsing and validation
- `src/agent/prompts/examples.ts` - Few-shot examples

### System Prompt Template
```typescript
const SYSTEM_PROMPT = `You are ClawLab, an AI agent that automates browser interactions to create product demos.

ROLE:
- Analyze screenshots to understand the current UI state
- Determine the next action to achieve the user's goal
- Provide precise element targeting for reliable automation

OUTPUT FORMAT:
Always respond with valid JSON matching the ActionResponse schema.`;
```

### Response Schema (Zod)
```typescript
const VisionResponseSchema = z.object({
  reasoning: z.string(),
  action: ActionSchema,
  confidence: z.number().min(0).max(1),
});
```

### Response Parser
```typescript
async function parseVisionResponse(raw: string): Promise<VisionResponse> {
  const jsonMatch = raw.match(/```(?:json)?\s*([\s\S]*?)```/) || [null, raw];
  const jsonStr = jsonMatch[1]?.trim() || raw.trim();
  
  try {
    const parsed = JSON.parse(jsonStr);
    return VisionResponseSchema.parse(parsed);
  } catch (error) {
    throw new ClawLabError('Failed to parse LLM response', 'LLM_PARSE_ERROR', true);
  }
}
```

### Provider-Specific Adaptations
- **Claude**: Use tool_use for structured output
- **GPT**: Use function calling
- **Gemini**: Use response_schema

## Plan

- [ ] Create system prompt template with clear instructions
- [ ] Implement action request prompt builder
- [ ] Define Zod schemas for all response types
- [ ] Build response parser with error recovery
- [ ] Add few-shot examples for common scenarios
- [ ] Create provider-specific adapters (Claude tools, GPT functions)
- [ ] Add prompt versioning for A/B testing

## Test

- [ ] System prompt produces consistent action formats
- [ ] Parser handles markdown-wrapped JSON
- [ ] Parser rejects malformed responses gracefully
- [ ] Few-shot examples improve action accuracy
- [ ] All providers produce valid ActionSchema responses

## Open Questions

1. **Confidence threshold**: At what confidence level should we retry vs proceed?
2. **History length**: How many previous actions to include? (context window limits)
3. **Screenshot resolution**: High-res for accuracy vs lower-res for speed/cost?
4. **Element disambiguation**: How to handle multiple matching elements?
5. **Provider fallback**: Should we try another LLM if one fails parsing?

## Notes

**Prompt Iteration**: Store prompt versions with performance metrics to enable A/B testing and continuous improvement.