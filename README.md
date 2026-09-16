# mlxfast-bench

The trusted, reproducible benchmarker for the MLXFast inference challenge. It is a Rust
workspace. It drives interchangeable engines behind a frozen wire protocol: on a Mac, the
generic `bench-worker` from the Layr-Labs `mlx-swift-lm` fork, which runs the track's Runner;
on a DGX Spark, a thin adapter over `ds4-resident`. A third-party engine that speaks the same
wire can take part from any hardware.

This repository owns the normative Engine Protocol v1, the scoring core, and every measurement
path. Each track's engine repository resolves a published `benchd` pair from the dist channel.
It never builds it.

## Documentation

Start at [docs/README.md](docs/README.md) — the index, with each document's class
(normative contract · runbook · architecture · history) and the citation rule.

The two entry points:

- [docs/overview.md](docs/overview.md) — read first: what benchd owns, one scored run
  step by step, and how an engine repository resolves benchd.
- [docs/architecture.md](docs/architecture.md) — the system as it runs: the components,
  the wire, the paired per-box measurement topology, the measured window, where each
  scored value comes from, the two box shapes, and one diagram.
- [docs/runner-contract.md](docs/runner-contract.md) — the runner boundary (draft, not yet
  ruled) that `bench-worker` and `bench_core::runner_manifest` implement.
- [docs/track-release-branches.md](docs/track-release-branches.md) — how model tracks
  bind to this repo: benchd is developed and published from `main`, the track id is the
  platform namespace and the R2 key prefix, and how an engine repo resolves benchd.

Superseded planning material lives under [docs/history/](docs/history/) and is retained
as a record, not as guidance.

## Branches, tracks and the dist channel

For the Qwen 3.8 125B-A6B project, a **branch is a project** and a **track id is a
platform namespace**. They are not the same string.

| Thing | Value | Rule |
|---|---|---|
| project branch | `main` | benchd is developed and published from `main`. The `qwen3.8-125b-a6b-v1` branch is `main` plus merge and republish commits. It stays until each 125B engine repository re-pins to `main`. ONE branch serves both engines (MLX and CUDA). |
| track id | `qwen3.8-125b-a6b-mlx-v1`, `qwen3.8-125b-a6b-cuda-v1` | The platform token before `-v{N}` keys every platform fact (`bench_core::constants::Platform`). The R2 prefix is the track id. |
| dist channel `branch` field | `main` on `main`; `qwen3.8-125b-a6b-v1` on that branch | The channel manifest names the branch it was published from, never a track id. One channel carries one binary for each platform; the platform is keyed by directory, not by branch. |

A run resolves its platform from the track id it declares (`--contract` `track_id` or
`MLXFAST_QWEN_MTP_TRACK_ID`). Nothing in the code reads a branch name.

## Workspace layout

| crate | role | state |
|-------|------|-------|
| `bench-protocol` | Engine Protocol v1 wire types + JSON Schema (normative) | live |
| `bench-core` | golden schema · score formula · floors · bands · sealing · conformance kit | live |
| `bench-runner` | engine lifecycle · parent-side timing · phase barriers · paired baseline | live |
| `benchd` | the CLI; everything below runs through it | live |

### `benchd` subcommands

Implemented: `iterate` (engine end-to-end → sealed `score.json`, with the
`--mode official` / local-iterate / local-submit flows, plus the
`--capture-baseline` mode for stored-pair tracks), `official` (alias for
`iterate --mode official`), `calibrate-baseline` (one box's health band for the
control leg), `correctness` (the conformance kit, with `--manifest`),
`validate-golden`, `validate-weights`, `parity-diff`, `prefill-decompose`,
`harness-hash`, `weights-digest`, `measure-job` (the older paired pair-loop seam →
`results.json`) and `overlay-timing` (its LOCAL merge).

Two scored paths coexist. `iterate --mode official` is the ranked path of the
Qwen 3.8 125B-A6B tracks. It is **paired and per box**: one run measures the
`official_pairs` pairs the track fixture declares, on one box in one job, and each
pair is a serial-control leg on the organizer-staged reference tree followed by
the candidate leg. The score is the live ratio, `prefill_gain ^ 0.25 *
decode_gain ^ 0.75`, at batch size 1 on one stream. These tracks read no stored
baseline pair. The `measure-job` → `overlay-timing` seam is the flow the earlier
tracks score through, and the 125B tracks never enter it.

Every scored value comes from the `--contract` track fixture. benchd holds no
per-track table. A value the fixture does not declare is a refusal, by name, before
any engine starts. `--contract` is therefore required on every path: `measure-job`,
`overlay-timing`, `validate-golden` and `calibrate-baseline` refuse without it by
name, `iterate --mode official` refuses when the command line is parsed, and the
local modes of `iterate` refuse as soon as they resolve the window shape or the
model shape. There is no local default.

benchd reads the fixture one time, digests the bytes it parsed, certifies every
declared value at parse time, and seals `metrics.contract_sha256` and
`metrics.contract_sources` in `score.json`
(`crates/bench-core/src/contract.rs`). One schema reads the fixture, and
[docs/architecture.md](docs/architecture.md) section 6 lists every field of it.

The fixture declares the batch size as `scored_batch_size`. That width selects the
measured path. A width of 1 selects the single-stream path. A width above 1 selects
the batched cohort path. Today only a width of 8 carries a certified series tag.

A benchd built from this `main` refuses a fixture that does not carry the keys of a
group it needs. Publish the engine fixtures, and the public cut of each engine
repository, before the benchd that requires them.

One timed window loads the weights one time, on every platform. The whole window
runs over one attached worker (`official::run_timed_window`). The worker resets
its cache and drains its allocator at the start of each measured phase.
benchd asserts the drain fail-closed at every phase close: `phase_diagnostics`
must report `cache_memory` of zero, or the run is refused. Capture again, on the
official path, any baseline pair that was measured under the earlier
fresh-process-per-phase shape.

Declared but **not implemented**: `transform`, `submit`. Both print
"not implemented in this wave".

`deploy/` holds `Dockerfile.dist-linux`, the reproducible Linux dist build. benchd
itself is not containerized. It runs natively on a Mac box. On a Spark it runs inside
the box's one container, as a staged and manifest-verified binary pair. `targets/` is
where the signed per-(model, platform) `target.toml` bundles will go. **Today it holds
only a README.** Those values live in the track fixture
(`crates/bench-core/src/contract.rs`).

## Status

Shipped and driving live ranked windows.
**Measurement and scoring live here, not in the engine repo.** On the Qwen 3.8
125B-A6B tracks `benchd iterate --mode official` measures the pairs and seals
`score.json`; on the earlier tracks `benchd measure-job` seals `results.json` and
the A-3 overlay computes the published score over it. An engine reports raw
profiling only. `scripts/benchmark.sh` is the harness root the engine repo's
`benchmark.json` invokes, and its hash is load-bearing.

Known incomplete surfaces, stated plainly:

- `benchd transform` and `benchd submit` are declared and unimplemented.

Build host is an M5 (native aarch64).

## Publishing `dist/`

Consumers do not build benchd. They download the binaries for their platform and
verify each sha256 against the one manifest beside them.

The channel publishes **two binaries**, from one `source_commit`, for each platform.
The roster is `DIST_BINARIES` in [`scripts/dist-lib.sh`](scripts/dist-lib.sh):

| binary | role |
|---|---|
| `benchd` | the measurement harness every run drives. |
| `record-correctness-golden` | the golden **author**. An engine repo's golden re-author tooling drives it, and it must be the same build that later validates the goldens it writes. Building it on the box from source is the "the box builds its own harness" hole the channel exists to close, so it ships here. |

One directory per platform, holding both binaries and the manifest:

| path | platform | build with |
|---|---|---|
| `dist/benchd`, `dist/record-correctness-golden`, `dist/benchd.manifest.json` | macOS aarch64 (`aarch64-apple-darwin`) | `./scripts/build-dist.sh`, on a Mac |
| `dist/linux-aarch64/` — the same three names | Linux aarch64 (`aarch64-unknown-linux-gnu`) | `./scripts/build-dist-linux.sh`, on any machine with Docker |
| `dist/linux-x86_64/` — the same three names | Linux x86_64 (`x86_64-unknown-linux-gnu`), for a participant's workstation; no ranked box runs it | `BENCHD_DIST_TARGET=x86_64-unknown-linux-gnu ./scripts/build-dist-linux.sh`, on any machine with Docker |

The manifests have the same shape. Publish every platform from the same
`source_commit`, so that one commit describes the whole channel.

```json
{
  "version": "0.0.0",
  "branch": "qwen3.8-125b-a6b-v1",
  "source_commit": "<40 hex>",
  "target_triple": "aarch64-apple-darwin",
  "sha256": "<benchd sha256>",
  "bytes": <benchd bytes>,
  "binaries": {
    "benchd": {"sha256": "<64 hex>", "bytes": <int>},
    "record-correctness-golden": {"sha256": "<64 hex>", "bytes": <int>}
  }
}
```

**The six top-level fields are unchanged and still describe `benchd` alone.** A
fetcher written before the second binary existed reads the same `sha256` and the same
`bytes`, for the same file, out of the new manifest — nothing it parses moved.
`binaries` adds one line per published binary.

**Every per-binary entry is one line, and that is load-bearing.** Consumers parse this
file with anchored, one-key-per-line `sed` (the offline path on a ranked box has a
shell and `shasum` and nothing else). Pretty-printing the nested objects would put a
bare `"sha256":` at the start of a line, an old fetcher's `manifest_field sha256` would
then return three values, and it would refuse. `scripts/test-dist-manifest.sh` holds
that down, with a pretty-printed negative control.

A consumer reads a per-binary entry with the same `sed` style it already uses for the
top-level fields:

```bash
# manifest_binary_field <manifest> <binary name> <sha256|bytes>
manifest_binary_field() {
  sed -n "s/^[[:space:]]*\"$2\"[[:space:]]*:[[:space:]]*{.*\"$3\"[[:space:]]*:[[:space:]]*\"\{0,1\}\([^\",}]*\)\"\{0,1\}.*}[[:space:]]*,\{0,1\}[[:space:]]*\$/\1/p" "$1"
}
```

**`main` is itself a channel.** `dist/` on `main` carries the full roster for all three
platforms, with manifest `branch: main`, published from `main`'s own source. The 125B
engine repositories still resolve their pinned pair from the `qwen3.8-125b-a6b-v1` branch.
To move a track to a `main` pin, change one line in that repository's
`tools/fetch-benchd.sh`.

### Release checklist

Do these steps in order, on a Mac, before you publish.

1. Run the Rust/Swift byte-budget parity test and make sure it passes:

       cargo test -p benchd -- --ignored rust_and_swift_agree_on_every_shared_fixture

   The test compiles the pinned Swift enforcer with `swiftc -O` and compares it
   with the Rust enforcer on every shared fixture. It is `#[ignore]`, so the
   default suite does not run it and a developer machine without a Swift
   toolchain is not blocked. A publish is the point where the two enforcers must
   agree, because the published `benchd` is the one that measures a submission.
   The test fails on a Mac without `swiftc`: install the Swift toolchain, do not
   skip the step.
2. Run `./scripts/build-dist.sh`, and the Linux builds if you publish them.
3. `git add -f dist` and commit.

Enable the pre-commit hook once for each clone with
`git config core.hooksPath .githooks`.
The hook rebuilds the staged macOS binaries — every name in the roster, not only
`benchd` — and refuses a commit that the current source does not produce. The hook
cannot rebuild the Linux set, because the container builds a pushed commit and not the
working tree; for that set it checks that the staged manifest describes every staged
binary.

`scripts/build-dist-linux.sh` fetches `source_commit` by sha in the container
(`deploy/Dockerfile.dist-linux`). Push the commit before you build it, and keep it
reachable from the branch tip after you build it. **Merge a pull request that
carries `dist/` with a merge commit. A squash merge makes `source_commit`
unreachable, and the manifest then makes a claim that no one can check.**

### What the engines read

The Mac paths do not move and the six top-level manifest fields do not move, so
`tools/fetch-benchd.sh` in the MLX engine repo needs no change to keep resolving
`benchd`. An engine that also wants `record-correctness-golden` fetches that name
from the same directory and verifies it against its `binaries` entry; that is an
engine-repo change, and this repo only states what the channel offers.

The CUDA engine runs on Linux aarch64, so its `tools/fetch-benchd.sh` must select
the platform. That engine repo owns the change; this repo only states what the
channel offers. The change has two parts:

1. Read the channel from the platform directory, not from `dist/`:

       ${BASE_URL}/refs/heads/${BRANCH}/dist/linux-aarch64/benchd.manifest.json
       ${BASE_URL}/refs/heads/${BRANCH}/dist/linux-aarch64/benchd

2. Read `target_triple` from the manifest, and refuse a manifest that does not
   name `aarch64-unknown-linux-gnu`. Without this check the Mac pair passes every
   other test, and the box installs a binary that it cannot run.

Nothing else changes. The offline path, `BENCHD_DIST_LOCAL`, and the sha256 and
bytes checks are the same for the two platforms.

The CUDA engine already resolves the channel and already selects its platform
directory; `benchd.pin` was removed there, so nothing pins one binary any more.

## Related repos

- `Layr-Labs/mlx-swift-lm` — the fork: the CBv2 engine, `Libraries/MLXRunners` and
  `bench-worker`. The Swift half of the runner contract.
- `Layr-Labs/mlxfast-qwen38-125b-a6b-engine` — the Qwen 3.8 125B-A6B MLX track's engine
  repo (spawns `bench-worker` from the fork submodule).
- `Layr-Labs/cudafast-qwen38-125b-a6b-engine` — the same track on CUDA (ds4).
- `Layr-Labs/ds4` — the CUDA engine; `ds4-resident` holds the weights.
- `mlxfast-qwen-38-27b-mtp-engine-dev`, `mlxfast-gemma4-26b-a4b-engine-dev` — the older
  tracks' engine repos (stored-pair tracks; channel-tip benchd).
