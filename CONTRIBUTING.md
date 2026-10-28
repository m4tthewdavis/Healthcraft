# Contributing to HealthCraft

Thank you for helping. Read [AGENTS.md](AGENTS.md) first: its rules apply to people and AI agents
alike.

## Ground rules

- **Language:** Rust only.
- **License:** contributions are licensed under MIT OR Apache-2.0, like the project.
- **Sign-off (DCO):** every commit carries a `Signed-off-by:` line (`git commit -s`), certifying the
  [Developer Certificate of Origin](https://developercertificate.org/): you wrote the change or
  have the right to submit it under the project's license.
- **No real patient data**, ever: not in tests, fixtures, screenshots, logs or issues.
- **Clean-room:** don't copy code, screens or content from proprietary EHRs. Work from public
  standards and observed behaviour.
- **Never crash, no `unsafe`, layering:** see AGENTS.md. `cargo xtask ci` must pass.
- **Commits:** small and focused, with a clear subject line.

## Adding a command

1. **Domain logic** goes in the lowest crate that fits (`record` for invariants, `fhir` for FHIR
   mapping, `research` for projections), with unit tests.
2. **The command** goes in `crates/engine/src/<area>_cmds.rs`, returned by that module's `specs()`
   and registered in `registry()`. Fill in:
   - `id`: `area.verb` or `area.noun.verb`, stable once released;
   - `label`: what a menu shows;
   - `params`: a one-line summary agents read through `commands.list`;
   - `access`: which roles may run it (`Access` in `crates/engine/src/lib.rs`);
   - `run`: parse parameters into a `#[serde(deny_unknown_fields)]` struct, write through
     `Session::append` (never the store directly), return JSON.
3. **Tests:** behaviour, refused roles, bad parameters. Run `panic_hunt`.
4. **Docs:** update `README.md` › What works today when a capability changes.

## Development

```sh
cargo xtask ci                    # what CI runs
cargo run -p healthcraft          # desktop app with a synthetic patient
cargo run -p healthcraft-cli -- commands
```
