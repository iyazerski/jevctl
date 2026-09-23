# jevctl

`jevctl` gives agents fast semantic judgments through a compact CLI and one generic MCP tool. It accepts shared JSON context plus batched `boolean`, `select`, and `scale` questions. TypeSafe models and API contracts remain internal.

## Install

macOS and Linux on x86_64 and ARM64 are supported.

```bash
curl -fsSL https://raw.githubusercontent.com/iyazerski/jevctl/main/install.sh | sh
```

The installer verifies the release checksum and installs to `~/.local/bin`.

`jevctl` expects `TYPESAFE_API_KEY` in its process environment. The binary and installer do not store or prompt for the key.

Check local readiness without making a request:

```bash
jevctl doctor
```

## Evaluate

Create `request.json`:

```json
{
  "context": {
    "request": "Implement only the known migration.",
    "options": {
      "generic": "Build a reusable framework.",
      "exact": "Implement the concrete migration."
    }
  },
  "questions": {
    "keep_generic": {
      "kind": "boolean",
      "prompt": "Should `options.generic` remain in the simplest complete plan?",
      "yes_when": "It is necessary for a known requirement.",
      "no_when": "It is speculative or outside scope."
    },
    "route": {
      "kind": "select",
      "prompt": "Which option best fits `request`?",
      "options": {
        "generic": "Choose only when broad reuse is required now.",
        "exact": "Choose when the concrete case is sufficient."
      }
    },
    "risk": {
      "kind": "scale",
      "prompt": "How risky is the selected implementation?",
      "levels": ["low", "medium", "high"]
    }
  }
}
```

Run one batched evaluation:

```bash
jevctl evaluate request.json
```

Compact output is the default:

```json
{"answers":{"keep_generic":0.04,"risk":1.2,"route":"exact"}}
```

Use `--full` only when probability distributions are needed for tuning or a consequential choice. Use `--pretty` for human-readable whitespace. Pipe JSON over stdin by omitting the path or passing `-`.

Validate locally without an API call:

```bash
jevctl validate request.json
```

## Question kinds

| Kind | Use it for | Compact answer |
| --- | --- | --- |
| `boolean` | Probability that a condition holds | Number from 0 to 1 |
| `select` | One option from a bounded set | Caller-defined option ID |
| `scale` | Degree over ordered levels | Zero-based numeric position |

Ask one narrow judgment per question and batch independent questions sharing context. Use deterministic code for arithmetic, exact parsing, lookup, execution, authorization, and policy.

## MCP

Run the stdio server:

```bash
jevctl mcp serve
```

The server exposes one tool named `evaluate`. MCP clients add their configured server prefix. The tool schema and instructions explain when semantic evaluation helps, how to construct each question kind, and when deterministic tools are more appropriate.

## Development

```bash
cargo fmt --all -- --check
cargo check --locked
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Run the opt-in live MCP smoke test when `TYPESAFE_API_KEY` is available:

```bash
cargo test --test mcp_protocol live_mcp_matches_compact_contract -- --ignored --exact
```

## License

MIT. See [LICENSE](LICENSE).
