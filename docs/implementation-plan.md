# jevctl implementation plan

## Goal

Build a small, fast Rust binary that gives agents one stable semantic-evaluation interface in two forms:

- a CLI for skills, shell pipelines, and CI;
- an MCP stdio server for tool-capable agents.

`jevctl` hides TypeSafe API contracts, model names, primitive names, response normalization, retries, and transport details. An agent supplies context plus a batch of simple questions and receives compact answers. CLI and MCP use the same protocol, validator, client, and formatter.

Task-specific skills are not part of this repository. They belong in `/Users/iyazerski/projects/dev-flows-plugin` and consume the stable `jevctl` interface.

## Design principles

1. **Agent-native contract.** Public inputs describe the judgment an agent needs, not the upstream API.
2. **Compact by default.** Normal output contains only question IDs and answer values.
3. **One call per batch.** Independent questions over shared context are evaluated together.
4. **No provider leakage.** Model versions, upstream field names, token usage, and wire formats stay internal.
5. **User-owned authentication.** `jevctl` reads `TYPESAFE_API_KEY`; it does not configure, store, or request credentials.
6. **Clear generic affordance.** MCP instructions explain when semantic evaluation helps and when deterministic tools are better.
7. **Minimal machinery.** No daemon, plugin system, provider abstraction, or workflow framework without measured need.

## Repository boundary

### `jevctl` owns

- the public evaluation request and response schemas;
- CLI and MCP interfaces;
- validation and compact formatting;
- the private TypeSafe adapter;
- HTTP connection reuse, timeouts, retries, and error normalization;
- generic protocol, integration, performance, and live smoke tests;
- releases, checksums, installer, and optional MCP registration.

### `dev-flows-plugin` owns

- skills and their trigger instructions;
- workflow-specific prompts, thresholds, and policies;
- plan lint, option comparison, claim verification, and future use cases;
- historical Codex workflow fixtures and task-level evaluations;
- guidance deciding when to call `jevctl` and how to use its result.

The CLI must not know about those workflows. The plugin must not depend on TypeSafe's API schema or model identifiers.

## Scope

### Included in v1

- `jevctl evaluate` for one batched evaluation from JSON;
- `jevctl validate` for offline request validation;
- `jevctl doctor` for a minimal local readiness check;
- `jevctl mcp serve` with one generic MCP tool named `evaluate`;
- three public question kinds: `boolean`, `select`, and `scale`;
- compact and full answer detail modes;
- environment-only authentication through `TYPESAFE_API_KEY`;
- bounded retries for documented transient upstream failures;
- stable exit codes and concise actionable errors;
- unit, CLI, HTTP, MCP protocol, performance, and opt-in live tests;
- cross-platform release artifacts following the useful parts of lspyx.

### Excluded from v1

- skills or hooks in this repository;
- API-key setup, login, keyring, config files, or command-line secret flags;
- public model/provider selection;
- a daemon or background service;
- response caching;
- multiple providers or provider traits;
- opinionated tools such as `plan_lint`, `review`, or `gate`;
- authorization or mutation decisions;
- template languages;
- verbose human reports or default telemetry output.

## Public evaluation contract

The public schema is owned by `jevctl` and versioned independently from TypeSafe.

### Request

```json
{
  "context": {
    "task": "Implement the exact two-predecessor migration.",
    "actions": {
      "generic": "Build a generic replacement framework.",
      "exact": "Implement the exact atomic transfer."
    }
  },
  "questions": {
    "keep_generic": {
      "kind": "boolean",
      "prompt": "Should the generic action remain in the simplest complete plan?",
      "yes_when": "It is authorized and necessary or clearly useful.",
      "no_when": "It is speculative, redundant, or outside scope."
    },
    "route": {
      "kind": "select",
      "prompt": "Which implementation best matches the request?",
      "options": {
        "generic": "Build a reusable framework.",
        "exact": "Implement only the requested migration."
      }
    },
    "risk": {
      "kind": "scale",
      "prompt": "How risky is the exact implementation?",
      "levels": ["low", "medium", "high"]
    }
  }
}
```

Rules:

- `context` accepts any JSON value and contains evidence relevant to all questions.
- Question IDs are caller-defined and preserved in output.
- `boolean` returns the probability that the answer is yes.
- `select` chooses one caller-defined option.
- `scale` returns a numeric position in the zero-based level range. With three levels, `0` is the first and `2` the last; intermediate values express uncertainty.
- Prompts are narrow and independent. Related questions sharing context should be batched.
- There is no public model, provider, endpoint, token, or upstream primitive field.

### Compact response

Compact output is the default for CLI and MCP:

```json
{"answers":{"keep_generic":0.04,"route":"exact","risk":1.7}}
```

It omits repeated kinds, explanations, provider metadata, model versions, and token usage. Consumers already know each question kind from their request.

### Full response

Full detail is opt-in for prompt tuning, evaluation, and threshold analysis:

```json
{
  "answers": {
    "keep_generic": {"value": 0.04},
    "route": {
      "value": "exact",
      "confidence": 0.92,
      "probabilities": {"generic": 0.08, "exact": 0.92}
    },
    "risk": {
      "value": 1.7,
      "probabilities": [0.05, 0.20, 0.75]
    }
  }
}
```

Full output still uses only `jevctl` concepts. It does not expose the upstream model, request shape, primitive names, usage, or raw response envelope.

## CLI interface

```text
jevctl evaluate [PATH|-] [--full] [--pretty]
jevctl validate [PATH|-]
jevctl doctor [--json]
jevctl mcp serve
jevctl --help
jevctl --version
```

Behavior:

- `PATH` defaults to `-`, meaning stdin.
- `evaluate` writes compact single-line JSON by default.
- `--full` requests the normalized full response.
- `--pretty` changes whitespace only and is intended for debugging.
- `validate` performs no network access and prints nothing on success.
- stdout contains only the result. Diagnostics go to stderr.
- non-interactive commands never print progress.
- help text includes short copy-pasteable stdin and file examples.
- the only credential source is `TYPESAFE_API_KEY` from the process environment.

`doctor` checks that the binary can run and reports whether `TYPESAFE_API_KEY` is available. It does not modify the shell, write configuration, open a browser, or perform a paid API request.

### Exit codes

| Code | Meaning |
| ---: | --- |
| 0 | Success |
| 2 | Invalid arguments, JSON, or evaluation request |
| 3 | `TYPESAFE_API_KEY` missing or rejected |
| 4 | Network or service failure |
| 5 | Invalid or incomplete service response |
| 6 | Transient retries exhausted |

Errors are one concise actionable line by default:

```text
jevctl: TYPESAFE_API_KEY is not set
jevctl: question "route" must define at least two options
jevctl: evaluation service unavailable after 3 attempts
```

Do not print raw upstream errors, authorization headers, credentials, or supplied context. A verbose developer diagnostic mode is not needed in v1.

## MCP interface

Run as `jevctl mcp serve`. Expose exactly one tool named `evaluate`. The MCP client prefixes it with the configured server name, so the tool name itself must not contain `jev` or `jevctl`.

### Tool input and result

The tool accepts the same `context` and `questions` fields as the CLI, plus optional `detail: "compact" | "full"`, defaulting to `compact`. Generate its JSON Schema from the same Rust types used by the CLI.

- `structuredContent` contains the normalized response object.
- text content contains compact JSON on one line for clients that do not consume structured content, even when `structuredContent` contains full detail.
- no prose, duplicated schema, model metadata, or success banner is added.
- errors are short and use public `jevctl` terminology.

### Server instructions and tool description

The metadata must teach an unfamiliar agent:

- **Why:** use this for fast semantic judgments when meaning, intent, relevance, support, similarity, or preference cannot be determined reliably by exact code.
- **When:** use `boolean` for probability of a yes/no claim, `select` for one option from a bounded set, and `scale` for ordered degree or severity.
- **How:** include concise evidence in `context`, ask one narrow judgment per question, give explicit criteria/options, and batch independent questions sharing context.
- **How to read it:** compact booleans are yes-probabilities, selects are option IDs, and scales use zero-based level positions. Request full detail only when distributions are needed.
- **When not:** do not use it for arithmetic, exact parsing, source lookup, deterministic checks, code execution, permission decisions, or prose generation.
- **Safety:** results are advisory semantic evidence. They do not grant authorization and do not replace tests or deterministic policy.

The tool description summarizes this boundary in a few precise sentences. It must be useful without a skill while staying short enough not to tax every agent context.

Follow lspyx's stdio lifecycle and subprocess-test approach. Unknown pre-initialization requests return `-32601`; unknown notifications are ignored; neither terminates the server.

## Internal architecture

```text
src/
├── main.rs             # thin process entrypoint
├── lib.rs              # command dispatch
├── cli.rs              # clap definitions and exit mapping
├── input.rs            # bounded stdin/file decoding
├── protocol.rs         # public request and normalized response types
├── validation.rs       # provider-independent local validation
├── client.rs           # reusable evaluation client and retry policy
├── typesafe.rs         # private upstream request/response adapter
├── commands.rs         # evaluate, validate, and doctor orchestration
└── mcp/
    ├── mod.rs          # rmcp server and tool metadata
    └── transport.rs    # pre-initialization handling

tests/
├── cli.rs
├── http.rs
├── mcp_protocol.rs
├── live_api.rs
└── fixtures/
    ├── boolean.json
    ├── select.json
    └── scale.json
```

`protocol.rs` is the only schema exposed to agents. `typesafe.rs` owns all translation to and from the current TypeSafe contract, including model selection. Upstream changes should normally require edits only in that adapter and its tests.

Do not add provider traits, repositories, service layers, or workflow abstractions while there is one provider and one operation.

## Private TypeSafe adapter

The adapter translates public question kinds to current TypeSafe primitives and converts returned distributions into normalized answers. Primitive names and wire types are private and must not appear in CLI help, MCP schemas, server instructions, or ordinary errors.

It owns:

- the internal model identifier and upgrades;
- the fixed production endpoint;
- upstream state, question, criteria, and envelope construction;
- upstream response validation;
- result normalization;
- upstream status and error mapping.

No CLI flag or MCP argument selects a model or sends a raw upstream request. A test-only base URL may be injected by integration tests, but it is not a documented user interface.

## Validation

Reject invalid input before making a paid request:

- non-empty questions map, IDs, and prompts;
- recognized question kinds only;
- non-empty boolean criteria when supplied;
- `select` has 2–255 unique non-empty option IDs and descriptions;
- `scale` has 2–10 non-empty ordered levels;
- response answer IDs exactly match requested IDs;
- boolean probabilities are finite and within 0–1;
- select results name a supplied option;
- scale results are finite and within the valid range;
- full distributions are finite, bounded, complete, and sum approximately to 1.

Do not invent undocumented input-size limits. Use a generous bounded input reader to prevent accidental unbounded memory use, and let the service report unsupported payload sizes.

## HTTP, retry, and authentication

Use one `reqwest::Client` per process with rustls, JSON support, connection pooling, and a bounded timeout.

- Read `TYPESAFE_API_KEY` once when an online client is created.
- Missing key fails before network access with `jevctl: TYPESAFE_API_KEY is not set`.
- Never accept a key through CLI arguments, stdin fields, MCP arguments, or config files.
- Never write shell profiles, `.env` files, MCP environment values, or keyring entries.
- The installer may register the command but must not add secrets to MCP configuration.
- Retry only documented transient responses, currently `429` and `529`.
- Respect `Retry-After`; otherwise use short bounded exponential backoff with jitter.
- Do not retry authentication, validation, malformed response, or ordinary client errors.
- Return normalized errors rather than raw upstream terminology.

The user is responsible for exporting `TYPESAFE_API_KEY` in the environment inherited by the CLI or MCP host.

## Output and token efficiency

Treat agent tokens as part of the performance budget.

- compact response is the default everywhere;
- output question IDs once and use scalar answers whenever possible;
- omit nulls, type tags, prose, usage, model, provider, and duplicated input;
- preserve deterministic key ordering for stable snapshots and parsing;
- generate MCP text and structured content from the same normalized object;
- keep help and descriptions clear without repeating long examples in every call;
- errors give the failing field and remediation in one line;
- require explicit `--full` or `detail: "full"` for distributions;
- never return chain-of-thought or request prose from the service.

Add snapshot tests for compact output size. Compare representative compact responses with equivalent raw upstream responses, and fail regressions that add fields without a documented agent need.

## Runtime performance

The network and semantic model dominate latency, so minimize local overhead without complicating the code.

- batch supplied questions into one upstream call;
- deserialize input once and serialize output once;
- avoid cloning shared context and large JSON values;
- reuse HTTP connections across MCP calls;
- use Tokio's current-thread runtime unless `rmcp` requires otherwise;
- keep logging disabled by default;
- perform no project discovery or configuration-file reads at startup;
- use thin LTO, one codegen unit, stripping, and abort-on-panic only after shutdown tests pass.

Measure separately:

- cold `jevctl --version` startup;
- offline validation throughput;
- CLI round-trip against a local mock server;
- persistent MCP round-trip against the same server;
- compact-output bytes and estimated tokens;
- live end-to-end latency.

Initial targets:

- validation median below 1 ms for representative batched input;
- CLI overhead below 10 ms excluding service time;
- persistent MCP overhead below 5 ms excluding service time;
- one upstream request per successful batch;
- no filesystem configuration reads during evaluation.

## Test plan

### Unit tests

- serialization and validation for all public kinds;
- compact and full normalization;
- scale boundaries and interpolation;
- private adapter mapping in both directions;
- incomplete, mismatched, or invalid upstream answers;
- retry classification, delay selection, and exhaustion;
- secret redaction, concise errors, and exit codes;
- compact-output snapshots and size budget.

### HTTP and CLI integration tests

- authorization is sent but never logged;
- private request translation is exact;
- successful responses normalize correctly;
- authentication, validation, overload, malformed response, missing answer, and timeout failures;
- `Retry-After`, retry count, exhaustion, and one-request normal execution;
- missing key fails before network access;
- stdin default, explicit `-`, and file input;
- compact, full, and pretty output;
- stdout/stderr separation and stable exit codes;
- `validate` and `doctor` do not access the network;
- no upstream implementation fields appear in help or results.

### MCP protocol tests

- initialize and list tools;
- the sole tool is named `evaluate`;
- schema and description explain all kinds and usage boundaries;
- compact default and full opt-in;
- on compact calls, text parses to the same object as `structuredContent`;
- on full calls, text matches the compact value projection of `structuredContent`;
- local validation and normalized service errors;
- repeated calls reuse one client;
- unknown pre-initialization request returns `-32601` and server remains available;
- unknown pre-initialization notification is ignored and initialization succeeds.

### Live and cross-repository tests

Live tests are opt-in and require `TYPESAFE_API_KEY` plus an explicit test flag. One batch covers all public kinds and verifies normalized shapes, ranges, latency, and absence of provider details.

Historical plan-lint, comparison, and claim-verification experiments stay in `dev-flows-plugin`. Once the interface is stable, run the same tasks through CLI and MCP and compare accuracy, latency, consistency, and agent-visible token use with the existing direct-API and MCP baselines.

## Delivery

Follow useful lspyx conventions:

- Rust 2024 edition and a thin binary over a reusable library;
- `cargo fmt`, `cargo check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` in CI;
- x86_64/aarch64 Linux musl and macOS release builds;
- tar archives and SHA-256 checksums;
- `install.sh` installs to `~/.local/bin`;
- optional Codex and Claude registration points to `jevctl mcp serve` only;
- registration relies on the host inheriting `TYPESAFE_API_KEY` and never embeds it.

Release artifacts contain the binary, README, license, and checksums, not skills. The README links to `dev-flows-plugin` for workflow-specific guidance once those skills are published.

## Implementation phases

### Phase 1: Public protocol

1. Scaffold the Rust package and quality checks.
2. Define provider-independent request, question, compact answer, and full answer types.
3. Implement validation and deterministic compact serialization.
4. Add bounded stdin/file loading.
5. Implement `validate` and minimal `doctor`.

Exit criterion: all public kinds validate and round-trip without TypeSafe concepts appearing in the schema.

### Phase 2: Private adapter and CLI

1. Implement private request and response types for the current TypeSafe API.
2. Map public kinds to the upstream contract.
3. Normalize upstream results into compact and full answers.
4. Add the reusable HTTP client, authentication, timeout, and retry policy.
5. Implement `evaluate` and mock HTTP/CLI tests.

Exit criterion: one live batched CLI evaluation succeeds with exactly one request and emits only the public response.

### Phase 3: MCP

1. Add `rmcp` and `jevctl mcp serve`.
2. Expose only `evaluate` from the shared public types.
3. Write concise metadata covering why, when, how, and non-use cases.
4. Return matching compact text and structured content.
5. Port lspyx-style subprocess and pre-initialization tests.

Exit criterion: CLI and MCP return equivalent answers, and an unfamiliar agent can call the tool correctly from its metadata alone.

### Phase 4: Performance and agent-interface validation

1. Measure startup, validation, mock CLI, and persistent MCP overhead.
2. Compare compact output bytes and estimated tokens with raw upstream output.
3. Optimize only allocations, serialization, or metadata shown by profiles.
4. Test tool selection and input construction with agents that have no `jevctl` skill loaded.
5. Confirm no model/version/upstream field leaks through schemas, help, output, or errors.

Exit criterion: runtime targets pass and compact output is materially smaller without losing agent-required information.

### Phase 5: Cross-repository workflow evaluation

1. Add or update skills in `/Users/iyazerski/projects/dev-flows-plugin` after the public contract stabilizes.
2. Port the earlier historical workflow cases there.
3. Run plan lint, option comparison, and claim verification through CLI and MCP.
4. Compare with the established direct-API and existing-MCP results.
5. Tune prompts and thresholds in the plugin, not `jevctl`.

Exit criterion: the engine reproduces the promising earlier results with lower agent-visible output cost, while workflow behavior stays outside this repository.

### Phase 6: Distribution

1. Complete README and compact examples.
2. Add release profiles, multi-target workflow, checksums, and installer.
3. Verify installation in a clean temporary environment.
4. Verify MCP registration without credentials in generated configuration.
5. Run format, check, clippy, unit/integration/protocol tests, live smoke, and release builds.

## Definition of done

- CLI and MCP share one public schema, validator, adapter, client, and formatter.
- Agents see only `context`, generic question kinds, and normalized answers.
- A normal batch makes one TypeSafe request.
- Compact output is default and meets its size budget.
- The MCP tool is named `evaluate` and is self-explanatory without a skill.
- Model versions and TypeSafe wire contracts are absent from public interfaces.
- `TYPESAFE_API_KEY` is the only credential mechanism, and configuration is the user's responsibility.
- Invalid requests fail locally before tokens or network time are spent.
- Retry behavior matches current TypeSafe guidance.
- MCP survives unknown pre-initialization requests and notifications.
- Runtime overhead and live latency are measured separately.
- Workflow skills and historical evaluations live in `dev-flows-plugin`.
- CI, release builds, checksums, installer, and documentation are complete.
- No credential or supplied context leaks through logs, errors, or artifacts.
