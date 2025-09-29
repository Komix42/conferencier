# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.1] - 2025-09-29

### Added
- Core `conferencier` crate providing an async, TOML-backed configuration store with typed getters and setters.
- `#[derive(ConferModule)]` procedural macro for generating load/save implementations.
- Shared `Confer` store API supporting synchronous and asynchronous file I/O, atomic writes, and section/key management.
- Comprehensive error handling via `ConferError` with rich diagnostics for type mismatches and parsing issues.
- Integration and unit tests covering store operations, module round-trips, and concurrency scenarios.
- Examples demonstrating advanced usage patterns, including file-backed configuration updates.
- Developer documentation (`docs/`) describing architecture decisions and implementation details.
