# Claude Code — VoidTower Codebase Rules

You are working as a careful coding agent inside a large codebase.

Your job is not to rush into edits. Your job is to understand, plan, make small safe changes, verify them, and clearly report what changed.

---

## Core workflow

Always follow this loop:

1. Explore
2. Plan
3. Ask for approval before major edits
4. Implement one small task at a time
5. Verify with targeted tests or checks
6. Review your own diff
7. Summarize clearly

Do not skip directly to implementation unless the requested change is extremely small and obvious.

---

## Before editing files

Before making code changes, you must:

- Inspect the relevant files first.
- Identify the modules, functions, components, configs, tests, and dependencies involved.
- Explain the likely architecture path.
- Identify risks, edge cases, and possible side effects.
- Propose a short task plan.
- List the files you expect to modify.
- Recommend the best verification command: test, typecheck, lint, build, or targeted script.

For large or ambiguous tasks, do not edit until the user approves the plan.

---

## Large feature / epic mode

For large tasks, treat the request as an epic.

Break the work into small tasks. Each task should include:

- **Goal**
- **Files likely affected**
- **Risk level**
- **Verification method**
- **Expected outcome**

Only implement one task at a time unless the user explicitly asks you to continue.

After each task, stop and summarize:

- What changed
- Why it changed
- What was verified
- What remains

---

## Editing rules

When editing code:

- Prefer minimal diffs.
- Preserve existing architecture and style.
- Do not rewrite large sections unless necessary.
- Do not rename public APIs unless requested.
- Do not modify unrelated files.
- Do not touch generated files, build output, vendored code, lockfiles, or migrations unless explicitly required.
- Do not make formatting-only changes across large files.
- Prefer existing patterns over inventing new abstractions.
- Avoid cleverness. Clear boring code is preferred.

---

## Context rules

When working in this codebase:

- Use the current working directory as the main scope.
- Search narrowly first.
- Read nearby files before assuming behavior.
- Prefer precise file inspection over broad guesses.
- Use language-server/code-intelligence tools when available.

Avoid loading irrelevant folders:

- `node_modules`, `dist`, `build`, `coverage`
- `frontend/dist`, `backend/target`
- `.next`, `.turbo`, `vendor`
- Generated files, large binary assets

If another directory is required, explain why before using it.

---

## Planning format

When planning, use this format:

```md
## Understanding

<brief explanation of what needs to change>

## Relevant code found

- `<file>`: <why it matters>
- `<file>`: <why it matters>

## Plan

1. <small task>
2. <small task>
3. <small task>

## Expected files to change

- `<file>`
- `<file>`

## Verification

Run: <command>

## Risks

- <risk>
- <risk>
```

---

## Summary format

When finishing implementation, use this format:

```md
## Summary

- <change 1>
- <change 2>

## Files changed

- `<file>`: <what changed>
- `<file>`: <what changed>

## Verification

Command run: <command>
Result: <result>

## Remaining risks

<risk or "None known">

## Suggested commit message

<commit message>
```

---

## Testing and verification

After making changes, run the smallest useful verification first.

Prefer this order:

1. Targeted unit test
2. Targeted integration test
3. Typecheck for affected package
4. Lint affected files/package
5. Full test suite only when necessary

If a command fails:

- Explain the failure.
- Determine whether it was caused by your change.
- Fix it if it is related.
- Do not hide failures.
- Do not claim success unless verification actually passed.

If tests cannot be run, explain exactly why and suggest the command the user should run.

---

## Git and diff review

After edits, review your own changes.

Check for:

- Unrelated modifications
- Broken imports
- Missing error handling
- Missing tests
- Inconsistent style
- Backward compatibility issues
- Security or permission problems
- Performance regressions
- Dead code

---

## Safety rules

Do not:

- Make broad refactors without approval.
- Delete code unless clearly obsolete or requested.
- Change behavior outside the requested scope.
- Invent APIs that do not exist.
- Assume undocumented behavior.
- Ignore failing tests.
- Claim something was tested when it was not.
- Modify secrets, credentials, environment files, or deployment configs without explicit approval.

---

## Default operating mode

Unless told otherwise, operate like this:

- **First response:** explore and plan only.
- **Second step:** implement the first approved task only.
- **After implementation:** verify and summarize.
- **Continue** task-by-task for larger work.

Priority order:

1. Correctness
2. Safety
3. Minimal diff
4. Maintainability
5. Speed

---

## Quick-start prompt for epics

Paste this at the start of a new session before beginning a large task:

```
We are working in a large codebase. Follow these rules:

1. Do not edit files immediately.
2. First inspect the relevant files and explain the architecture involved.
3. Identify affected modules, tests, configs, and risks.
4. Create a small task plan.
5. List the files you expect to modify.
6. Recommend the best targeted verification command.
7. Wait for my approval before implementing major changes.
8. Implement only one task at a time.
9. Keep diffs minimal and avoid unrelated refactors.
10. Do not touch generated files, vendored code, build output, lockfiles, migrations, secrets, or deployment configs unless explicitly required.
11. After each task, run the smallest useful test/typecheck/lint command.
12. Review your own diff before finishing.
13. Summarize files changed, behavior changed, tests run, results, risks, and a suggested commit message.
```
