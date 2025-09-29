# RFC-0002: Baseline Examples for conferencier

Date: 2025-09-29  
Status: Accepted  
Author: Komix42 <komix@ignore.cz>

## Document Scope

This RFC defines the requirements, scope, and validation steps for shipping baseline examples with the `conferencier` workspace. The goal is to provide runnable guidance that demonstrates both introductory and advanced usage of the configuration hub and derive macro.

## Motivation

- Offer copy-pasteable starting points for consumers evaluating the crate.
- Document idiomatic use of the `Confer` store and `#[derive(ConferModule)]` macro, including the preferred ergonomics for importing the derive.
- Exercise the full load → mutate → save lifecycle outside of unit tests, strengthening confidence that the crate behaves intuitively in real programs.

## Requirements

| Requirement | Description | Notes |
| --- | --- | --- |
| EX-1 | Provide at least one **basic** example that loads a single module, mutates values, and persists them. | Must illustrate defaults, optional fields, and saving back to TOML. |
| EX-2 | Provide at least one **advanced** example that uses multiple modules sharing the same `Confer` store. | Should cover renames, vector defaults, optional vectors, ignored runtime fields, and concurrent saves. |
| EX-3 | Examples must build with a single `use conferencier::ConferModule;` import, enabling the short `#[derive(ConferModule)]` syntax. | Demonstrates usability improvement over fully-qualified derives. |
| EX-4 | Advanced example reads its initial configuration from disk and writes an updated snapshot back to disk. | Ensures real-world persistence flows are documented. |
| EX-5 | Each example includes concise inline comments and `cargo run --example <name>` instructions. | Keeps documentation self-contained. |

## Out of Scope

- Hot-reload or dynamic watch loops.  
- Examples demonstrating unsupported field shapes (e.g., nested custom types).  
- Benchmarks or performance comparisons.

## Deliverables

1. `examples/basic_usage.rs` satisfying EX-1 and EX-3.  
2. `examples/advanced_usage.rs` satisfying EX-2, EX-3, and EX-4.  
3. `examples/data/advanced_config.toml` — sample input for the advanced scenario (EX-4).  
4. Documentation updates referencing the examples (see IMPL-001 update).

## Implementation Notes

- Both examples should reuse the `Confer` re-export of the derive macro (`use conferencier::ConferModule;`) and privately import the trait (`use conferencier::confer_module::ConferModule as _;`) to keep trait methods available without polluting the public API.
- Advanced example may persist its final snapshot to `target/advanced_usage_snapshot.toml` for developer inspection. The directory creation must propagate IO failures via `ConferError` to avoid `panic!`s in library-controlled code paths.
- Example source files should remain under 200 LOC to preserve readability.

## Validation

- `cargo run --example basic_usage`  
- `cargo run --example advanced_usage`

Additional checks: ensure `cargo test` continues to pass and that documentation references to the derive macro remain accurate.

## Future Work

- Add a tutorial-style README section referencing these examples.  
- Consider an example demonstrating integration with an HTTP server once higher-level components exist.  
- Explore a CLI workflow for exporting/importing snapshots.
