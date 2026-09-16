# targets/

One signed `target.toml` per (model, platform) — model identity, checkpoint pins,
per-platform baselines, scoring floors/bands, gates, telemetry provider. Bundles
what is triplicated today (MLXFastConstants, benchmark.yml env, fixtures, R2 keys).

**Nothing is checked in yet — this directory holds only this README.** No `target.toml`
bundle has been authored; the values it would carry still live in
`crates/bench-core/src/constants.rs` and the track fixtures.

Planned, one per (model, platform) — so named after the track's `track_id`, not its release
branch, which can serve more than one platform (`docs/track-release-branches.md`):

| bundle | track |
|---|---|
| `qwen38-27b.m5.toml` | qwen 3.8 27B MLX (release branch `main`) |
| `qwen38-27b.rtx-pro-6000-blackwell.toml` | qwen 3.8 27B CUDA (release branch `qwen3.8-27b-cuda-v1`) |
| `gemma4-26b-a4b.m5.toml` | gemma 4 26B A4B MLX (release branch `gemma4-26b-a4b-mlx-v1`) |

The Qwen 3.8 125B-A6B tracks are not in the table. They already declare every scored value in
their track fixture, which is the consolidation this directory was planned to give them, so a
bundle would add a third place to look.
