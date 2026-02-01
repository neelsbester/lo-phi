# Tasks: [FEATURE_NAME]

**Plan Version:** [PLAN_VERSION]
**Date:** [DATE]

---

## Task Categories

Tasks are categorized by the constitution principle they primarily
serve:

- **STAT:** Statistical Correctness (Principle 1)
- **PERF:** Performance at Scale (Principle 2)
- **TRANS:** Transparent Decision-Making (Principle 3)
- **UX:** Ergonomic TUI/CLI (Principle 4)
- **TEST:** Rigorous Testing (Principle 5)

## Tasks

### [TASK-1]: [Task Title]

- **Category:** [STAT | PERF | TRANS | UX | TEST]
- **Priority:** [P0 | P1 | P2]
- **Dependencies:** [None | TASK-X]
- **Files:** `src/...`
- **Description:** [What needs to be done]
- **Acceptance Criteria:**
  - [ ] [Criterion]
- **Status:** Pending | In Progress | Done

---

## Dependency Graph

```
TASK-1 → TASK-2 → TASK-3
                 ↘ TASK-4
```

## Completion Checklist

- [ ] All tasks marked Done
- [ ] CI passes (clippy + fmt + tests)
- [ ] Benchmarks run (if PERF tasks present)
- [ ] CLAUDE.md updated (if architecture changed)
- [ ] Constitution compliance verified
