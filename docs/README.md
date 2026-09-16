# docs/

Every document here carries a **class**. The class tells you what the document is for and
how much weight a citation to it carries.

| class | meaning |
|---|---|
| **normative contract** | Binding. Code cites it as its definition; a change here is a change to behaviour. |
| **runbook** | Operational procedure. Follow it as written. |
| **architecture** | Describes the shape of the system. Explanatory, not binding. |
| **governance** | Records a ruling or a ledger. Binding as a rule, not as a spec. |
| **history** | A record of superseded work. Kept for reasoning and provenance. **Never guidance.** |

A second tag, **legacy path**, marks a living document that governs only the older tracks
(`qwen3.8-27b-mtp-v1`, `gemma4-26b-a4b-mlx-v1`) or an operator flow the Qwen 3.8 125B-A6B
tracks do not enter. Such a document is still binding where it applies; it is not the
description of a ranked 125B run.

## Reading order

1. [`overview.md`](overview.md), then [`architecture.md`](architecture.md), for the shape and
   the diagram.
2. [`runner-contract.md`](runner-contract.md) and
   [`../crates/bench-protocol/PROTOCOL.md`](../crates/bench-protocol/PROTOCOL.md), for the two
   boundaries benchd drives.
3. [`track-release-branches.md`](track-release-branches.md), for how a track binds to this repo
   and how it finds its denominator.
4. [`runbook-new-engine.md`](runbook-new-engine.md), then the box runbooks, to stand something
   up.

## Index: the live system

| document | class | what it is |
|---|---|---|
| [`overview.md`](overview.md) | architecture | Read first. What benchd owns, the workspace, one scored run step by step, Engine Protocol v1 at a glance, how an engine repository resolves benchd, where the scoring inputs live, what a run records, and the agreed direction. |
| [`architecture.md`](architecture.md) | architecture | The system as `main` runs it: the components, Engine Protocol v1, one component diagram, the measured window, where each scored value comes from with every contract field listed, the two box shapes, the published pair, the tracks, and the open items. |
| [`runner-contract.md`](runner-contract.md) | normative contract (**draft, not ruled**) | The runner boundary: `Runner`, the static manifest and its canonical digest, hello derivation, the verb mapping in `bench-worker`, the resource passthrough, and what benchd changed. Cited by section number from `runner_manifest.rs`, `engine_resource.rs`, `correctness.rs`. |
| [`../crates/bench-protocol/PROTOCOL.md`](../crates/bench-protocol/PROTOCOL.md) | normative contract | Engine Protocol v1 and its additive extensions (v1.1 free-run, v1.2 cohort, runner and resident identity). Frozen; lives with the crate that owns it, with the JSON Schema beside it. |
| [`spec-config-design.md`](spec-config-design.md) | normative contract | The per-module speculative configuration wire surface (`spec` / `effective_spec`). Cited as the contract by the frozen Protocol v1 JSON Schema and by 14 sites in `bench-protocol` / `bench-runner`. |
| [`model2-calibration.md`](model2-calibration.md) | normative contract | Track `qwen3.8-27b-mtp-v1`. The model-2 new-series calibration regime of the `measure-job` decode-only flow: the series fence, native-vs-model-2 segregation, the series-scoped band gate. Enforced in `crates/benchd/src/measure_job.rs`. The band VALUES are still to be measured on box. |
| [`single-stream-prefill-window.md`](single-stream-prefill-window.md) | normative contract | The wire contract for the prefill half of the Qwen 3.8 125B-A6B composite: where benchd splits its own clock between `free_decode_begin` and `free_decode_run`, what the engine must do inside each verb, and what the paired run seals. |
| [`scored-regime-and-prefill-window.md`](scored-regime-and-prefill-window.md) | normative contract | What a `measure-job` track declares in its fixture that it scores (batch size + composite exponents, one regime per `track_id`), and what benchd does with the prefill half of the free-run timed window. The declaration arms the certification. §3 is the standing limit: no track that scores through that seam may declare a nonzero prefill exponent until a work-placement invariant exists. Enforced in `crates/bench-core/src/{contract,prefill_window}.rs` and at four scoring seams in `crates/benchd`. |
| [`track-release-branches.md`](track-release-branches.md) | governance | How a track binds to this repo: benchd is developed and published from `main`, the `track_id` is the platform namespace and the R2 key prefix, and `main` and a project channel branch each carry a published `dist/`. The two benchd resolution channels, the two baseline kinds (live control leg, stored pair), model identity, scored regime, golden authoring, and the fixture-before-benchd deploy order. |
| [`parity-completion-gate.md`](parity-completion-gate.md) | governance | **SIGNED, frozen.** The definition of done for the `qwen3.8-27b-mtp-v1` MLX parity program, against the Qwen 3.6 27B corpus. Changes require a new ruling, not an edit. Its figures are that program's, not the 125B tracks'. |
| [`window-preflight.md`](window-preflight.md) | runbook | The mandatory pre-lock gate for every GPU window. |
| [`runbook-new-engine.md`](runbook-new-engine.md) | runbook | A new engine track, from a runner in the fork to an open ranked row: the four things a track is, the four steps, and what still takes hand work. |
| [`box-setup-runbook.md`](box-setup-runbook.md) | runbook | Overview of box setup for both platforms: shared pieces, box calibration, differences, faults. |
| [`runbook-box-setup-cuda.md`](runbook-box-setup-cuda.md) | runbook | Stand up a DGX Spark (CUDA) ranked box: one container per box, fleet peer copy, reference tree and box calibration, readiness checklist. |
| [`runbook-box-setup-mlx.md`](runbook-box-setup-mlx.md) | runbook | Stand up a Mac (MLX) ranked box, with a readiness checklist. |
| [`qwen38-125b-a6b-baseline-capture.md`](qwen38-125b-a6b-baseline-capture.md) | runbook | Calibrate one ranked box for the Qwen 3.8 125B-A6B tracks: what the paired ranked run measures, the health band `benchd calibrate-baseline` writes, the per-leg resident engines, and every refusal by name. |
| [`official-baseline-capture.md`](official-baseline-capture.md) | runbook | STORED-PAIR tracks only. How to capture a PENDING track's official baseline pair with `benchd iterate --capture-baseline`, and what the capture record is for. |
| [`EVICTED.md`](EVICTED.md) | governance | The redirect ledger for paths evicted from `main`, plus the eviction and citation rules. |

## Index: legacy paths

| document | class | what it is |
|---|---|---|
| [`official-baseline-capture.md`](official-baseline-capture.md) | runbook, **legacy path** | Capture a stored-pair track's official baseline pair with `iterate --capture-baseline`. A live-control-leg track refuses the mode by name and calibrates its box instead. |
| [`model2-calibration.md`](model2-calibration.md) | normative contract, **legacy path** | The measure-job serial band, the series fence and the band gate, enforced in `measure_job.rs`. The 125B paired path uses the per-box calibration file instead. |
| [`window-preflight.md`](window-preflight.md) | runbook, **legacy path** | The pre-lock gate for operator-driven GPU windows on the 27B track's M5 boxes (`scripts/window-preflight.sh`). Yukon-dispatched 125B jobs take the GPU lock inside the job and do not run it. |
| [`parity-completion-gate.md`](parity-completion-gate.md) | governance, **signed and frozen** | The definition of done for MLX parity of the 27B track, signed 2026-08-20. Changes require a new ruling. |

## Index: history

| document | superseded by |
|---|---|
| [`history/architecture-split-design.md`](history/architecture-split-design.md) | The original benchmarker/engine split design (was `docs/architecture.md` until 2026-09-08). Its wire and its red/green cycles shipped; its `target.toml`, container image, `bench-agent` peer and TCP bridge did not. `architecture.md` is the live description. |
| [`history/dgx-spark-implementation-plan.md`](history/dgx-spark-implementation-plan.md) | The DGX Spark plan of the 27B era. The 125B CUDA track later did land on DGX Spark, on the ds4 engine, but nothing in this plan (vLLM, NVFP4, its numbers) transfers. |
| [`history/execution-plan.md`](history/execution-plan.md) | The split shipped. Kept for the ticket decomposition and acceptance criteria. |
| [`history/dependency-graph.md`](history/dependency-graph.md) | A snapshot of the build-out issue graph, not a live tracker view. |
| [`history/fuzz-corpus-report.md`](history/fuzz-corpus-report.md), [`history/fuzz-corpus-report.txt`](history/fuzz-corpus-report.txt) | The M-4 loader fuzz corpus freeze report. The live check is `scripts/fuzz-corpus-check.sh` against the frozen corpus fixture. |

## Documents that live elsewhere

| where | what |
|---|---|
| each engine repo | `benchmark.json`, the track fixture and its golden pins, `benchmark.yml`, the box tools (`resident-up.sh` / `serve-up.sh`, `ranked-box-preflight.sh`, `calibrate-box.sh`, `new-track.sh`), the participant contract |
| the `mlx-swift-lm` fork | `Libraries/MLXRunners` and `bench-worker`, the Swift half of the runner contract |
| the top-level [`../README.md`](../README.md) | the workspace layout, the `benchd` verbs, and how `dist/` is published |
| [`../targets/README.md`](../targets/README.md) | the planned `target.toml` bundle, one per model and platform. Nothing is checked in yet |

## The citation rule

1. **Unpinned citations may only target LIVING documents.** Writing `docs/foo.md` or
   `docs/foo.md:120` in code, a script, or emitted output promises the reader can open that
   path in the working tree.
2. **Citations to `history/` must be `@sha`-pinned.** History documents are frozen records;
   an unpinned cite to one implies it still describes the system, which it does not.
3. **Citations to evicted paths must be `@sha`-pinned and listed in
   [`EVICTED.md`](EVICTED.md).** A pin must resolve *in this repo*.
4. **Line-numbered pins are re-verified when repinned.** If you move a pin to a different
   sha, open the file at that sha and confirm the lines still say what the citation claims.
5. **Section-numbered pins survive a rewrite only if the section keeps its subject.**
   `docs/architecture.md §2` means Engine Protocol v1; `runner-contract.md` keeps the draft's
   section numbers for the same reason. A cite to a section that no longer exists is repinned
   to a living section with the same subject, or to a `@sha`-pinned history document.
