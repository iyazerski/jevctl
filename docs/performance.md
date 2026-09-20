# Performance measurements

Measured on 2026-09-20 using an Apple M1 Pro, macOS 27.0, Rust 1.98.0, and the release profile from `Cargo.toml`.

| Path | Samples | Median | p95 | Target |
| --- | ---: | ---: | ---: | ---: |
| `jevctl --version` process startup | 100 | 2.92 ms | 3.13 ms | informational |
| `jevctl validate tests/fixtures/all-kinds.json` | 100 | 3.00 ms | 3.26 ms | <10 ms CLI overhead |
| Reused client against local HTTP | 200 | 0.158 ms | 0.383 ms | <10 ms |
| Persistent MCP local validation call | 200 | 0.031 ms | 0.042 ms | <5 ms |

The three-question live evaluation took 0.93–1.05 seconds across three CLI runs. This is reported separately because service time dominates local overhead.

For the same three-question request, the raw upstream response was 498 bytes. The compact public response was 60 bytes and the full normalized response was 217 bytes. Compact output reduced agent-visible response size by 88% while retaining every directly actionable answer.

The protocol tests also verify that an agent with no skill loaded sees one `evaluate` tool, all public question kinds, usage boundaries, compact/full controls, and no model or upstream primitive names.

Reproduce the local client measurement with:

```sh
cargo test --release client::tests::measures_local_client_round_trip -- --ignored --exact --nocapture
```
