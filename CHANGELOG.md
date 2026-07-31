# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.1] - 2026-07-31

### Added

- Tauri v2 + React/TypeScript application skeleton (Vite, strict TS, Tailwind
  dark-theme tokens) opening an empty window titled with the app version.
- `protocol.rs` / `protocol.ts` boundary convention for all UI↔backend types.
- GitHub Actions CI matrix (`windows-latest` + `ubuntu-latest`): lint,
  typecheck, tests, `cargo fmt`/`clippy`, and a debug `tauri build`.
- GitHub Actions release pipeline (`tauri-action`) that builds installers
  from a `v*` tag into a draft release.
- Contributor docs: `LICENSE` (MIT), `CONTRIBUTING.md`, issue templates.
