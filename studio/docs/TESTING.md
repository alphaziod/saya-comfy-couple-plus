# Testing

Every command runs from the `studio/` directory.

## Unit and integration suites (no GPU, no network)

| Suite | Command |
|---|---|
| Rust core + shell | `cargo test --workspace --no-fail-fast` |
| Frontend logic, IPC contract, i18n parity | `node --test scripts/*.test.mjs` |
| Svelte / TypeScript | `npx svelte-check` (also part of `npm run build`) |
| Python helpers | `python3 -I scripts/purge-mock-artifacts.test.py`, `python3 -I scripts/casting-bridge.test.py` |

Ignored Rust tests need a real ComfyUI, the GPU or the installed Prompt Engine; run them only on purpose
with `--ignored`.

## Native smokes (real WebView, fake engine)

Build the test binary first (both features are required):

```bash
npm run build && cargo build -p saya-studio --features custom-protocol,smoke-test
```

Then run, for example:

```bash
bash scripts/ui-tour-native-smoke.sh
bash scripts/casting-lot-resume-native-smoke.sh
bash scripts/bypass-native-smoke.sh
```

How they work: `scripts/v12-native-smoke.py` starts the app in an isolated window with
`XDG_DATA_HOME`/`XDG_CONFIG_HOME`/`XDG_CACHE_HOME` under `test-results/<name>/`, sets
`SAYA_STUDIO_SANDBOX`, optionally starts `scripts/mock-comfy.py` (loopback, ephemeral port),
injects the `*-native-smoke.js` script, and waits for `finish_smoke(report)`.
`smoke_capture(name)` saves a window screenshot into the isolated data dir.
Reports and captures land in `test-results/<name>/<run>/`.

Smoke scripts navigate with `[data-nav=<id>]` (helper `nav()` in `scripts/ui-tour-native-smoke.js`)
and match some French UI labels on purpose: keep them in sync when changing visible text.

## Isolation rules

- Tests must fail rather than fall back to the user's real folders: a smoke build refuses to
  start without an isolated data dir, and `SAYA_STUDIO_SANDBOX` blocks image writes elsewhere.
- Fixtures are synthetic: generated PNGs, neutral SFW prompts, stub engines
  (`scripts/casting-stub-engine.py`, `scripts/*-fixture.py`). Never copy personal images,
  characters or prompts into tests.
- `scripts/purge-mock-artifacts.py` removes leftovers of old test runs from a real data dir only
  on proof (dry run first, journal written).

## GPU checks

Real renders are never part of the automatic suites. They need ComfyUI started by the user and
explicit approval; use fresh seeds for each check.
