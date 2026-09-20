# R0-02 release evidence subprocess-boundary handoff — 2026-09-20

Status: implemented and unit-verified; release-qualified: blocked.

Implementation commit: c439d70 ([verified] harden release evidence subprocess boundaries)
Branch: dev

Implemented

- Added scripts/process_supervisor.py for explicit-argv child execution with parent-death signaling, process-group cleanup, Linux child-subreaper descendant cleanup, bounded Git-target file output, and stdin forwarding.
- Hardened scripts/release_gate.py and scripts/repo_truth.py for bounded concurrent diagnostics, fail-closed timeout/required-gate behavior, malformed manifest/schema rejection, repository-contained path validation, symlink rejection, deterministic Git evidence, safe launch-failure classification, and credential redaction including quoted, structured, truncated, URL, AWS/access-key, and consecutive-flag cases.
- Added focused regression coverage in scripts/test_release_gate.py and scripts/test_repo_truth.py.
- Updated CONTRIBUTING.md, docs/release-gates.md, docs/internal/agent-knowledge/system-map.md, and docs/internal/agent-knowledge/documentation-backlog.md.

Independent review

- Final independent staged-diff review passed: security_concerns=[] and logic_errors=[]; the only non-blocking suggestion was documenting the Linux-specific process-supervisor prerequisite, which is now recorded in docs/release-gates.md and the system map.
- Static added-line scan found no os.system, shell=True, eval/exec, or pickle.loads patterns.

Verification evidence

- `PYTHONDONTWRITEBYTECODE=1 python3 -m unittest scripts.test_release_gate scripts.test_repo_truth -v` — passed, 39 tests (21 release-gate, 18 repository-truth).
- `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile scripts/process_supervisor.py scripts/release_gate.py scripts/test_release_gate.py scripts/repo_truth.py scripts/test_repo_truth.py` — passed.
- `git diff --cached --check` — passed before commit.
- `python3 -c ... run_git(..., "hash-object", "--stdin", input_text="hello")` — passed; stdin forwarding returned Git exit 0 and the expected object hash.
- Two `python3 scripts/repo_truth.py --repo . --json --check` runs were byte-identical; SHA-256: f80dbe8e9284a28e06f4616ae4eae3db2ea04f39697795dd297429dc1127b1ef.
- Changed-scope release report: `python3 scripts/release_gate.py --repo . --scope changed --json --output dev-data/release-gate-current.json` returned exit 1 by design because required pre-existing environment gates failed. repository-truth-tests, repository-truth-check, agent-package-contracts, and diff-check passed; repository-hygiene failed on pre-existing tracked docs/internal evidence/handoff paths; supply-chain failed because cargo deny is not installed (`cargo: no such command: deny`). The machine-readable artifact is dev-data/release-gate-current.json and is generated evidence, not release qualification.
- `bash scripts/check-repository-hygiene.sh` — blocked by the existing tracked docs/internal policy violation; exact paths are preserved in the release artifact.
- `bash scripts/check-schema-migration-ownership.sh` — apparent pass is not trusted because the environment reports `rg: command not found` before printing its success line.

Limitations and blockers

- No runtime/browser/Docker qualification was claimed; this sandbox has no host Docker/browser qualification path.
- The process-supervisor implementation and qualification are Linux-specific.
- Release qualification remains blocked by the pre-existing repository-hygiene policy state and missing cargo-deny tool. The cgroup filesystem is present but not writable in this sandbox, so no cgroup containment claim is made.
- Unrelated pre-existing untracked `scripts/__pycache__/` and `testing/` paths were preserved and not staged or modified.

Retrospective

- Learned that nested bounded subprocesses must not inherit the runner's file-size limit; apply the output limit only inside the Git target and supervise process trees separately.
- Verified that explicit argv, canonical repository paths, symlink rejection, random launch-failure markers, structured/truncated redaction, process-tree cleanup, and Git stdin forwarding are covered by executable tests.
- Reusable commands/fixtures are the two focused unittest suites, the deterministic repo_truth double-run, the changed-scope release artifact command, and the checkout-local executable-wrapper fixtures (the sandbox `/tmp` mount is noexec).
- End-user/developer documentation changed in CONTRIBUTING.md and docs/release-gates.md; future documentation remains mapped in docs/internal/agent-knowledge/documentation-backlog.md.

Next dependency-ready slice

- Resolve the R0-02 release qualification prerequisites as one bounded operational slice: provide cargo-deny in the verification environment and reconcile the repository-hygiene policy for the tracked docs/internal continuity artifacts, then rerun the unchanged release-gate manifest without changing this subprocess contract.
