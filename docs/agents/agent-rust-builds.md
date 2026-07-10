# Agent Rust Builds

- Rust crate: `backend/`; repo root has no `Cargo.toml`.
- In sandboxed coding agents such as Codex, Claude Code, or Copilot, build with `scripts/codex-cargo-build.sh`.
- Pass extra Cargo args through the helper, e.g. `scripts/codex-cargo-build.sh --tests`.
- Do not remove or alter the user's global `sccache` Cargo config to make agent builds work.
- Normal user-terminal behavior and rationale: [../16-agent-rust-builds.md](../16-agent-rust-builds.md).
