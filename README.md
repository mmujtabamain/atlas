# Atlas Financer

A deterministic, user-controlled financial timeline and decision engine for households,
their people, accounts and companies — the product specified in [`plan.md`](plan.md)
(requirements v3.2). Desktop app in **Rust + gpui via gpui-kit 0.6**.

No AI anywhere in the calculation path. Every derived figure carries its complete
calculation chain and can answer *"why is this number this number?"* (§2.1); future
money is never shown as money already available (§2.4); authorization is part of the
data model, not a later feature (§7.1).

## Layout

| Path | What |
|---|---|
| `crates/atlas-core` | Engine: money (integer minor units), household model, reservations, timeline, forecast, provenance graph, authorization. No UI, no I/O. |
| `crates/atlas-app` | The gpui-kit window: shell, screens, the explain sheet, failure alerting. Binary `atlas`. |
| `docs/ui-implementation-plan.md` | Milestones M0–M11 and their tasks (mirrored on the DevBench board). |
| `scripts/shoot.sh` | Headless screenshots of every screen (Linux box). |
| `plan.md` | The requirements document this repo implements. |

## Build and run

```bash
cargo run --bin atlas -- --theme dark --screen household --viewer a
cargo test                      # engine unit tests + app UI integration tests
```

Options: `--theme light|dark`, `--size WxH`, `--screen <section>` (see `atlas --help`),
`--viewer a|b` (which fixture person is looking — private objects project to aggregates).

The app boots with the fictitious *plan household* fixture (`atlas_core::fixtures`), whose
numbers come from the plan's own worked examples (E01, E03, E07, §13.1, §10.2).

### Linux DevBench box

The box has no root and no GPU. `source ~/.cargo/env` first; the linker finds the vendored
system libraries through `.cargo/config.toml` and the `.sysroot` symlink into
`../gpui-lab/.sysroot` (built by `gpui-lab/scripts/setup-linux-sysroot.sh`). Keep
`~/.cargo/config.toml` at `jobs = 2` (2 GiB memory cgroup). Screenshots:
`scripts/shoot.sh [scenario]`, then `devbench media put shots/<name>.png`.

## Monitoring

`atlas_app::alerting` logs every engine entry point and failure path and, when
`DEVBENCH_NOTIFY_URL` and `DEVBENCH_NOTIFY_TOKEN` are set in the deployment environment,
posts panics and calculation failures to DevBench's notify endpoint
(`POST $DEVBENCH_NOTIFY_URL/api/notify/`, bearer token, `{"text","level","source":"atlas-app"}`).
Without them it degrades to local logging and the status bar says "Alerts: log only".
The token is a server-side secret: it is read from the environment only and never logged.
