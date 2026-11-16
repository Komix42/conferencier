Groups of settings inside a section map to plain structs that derive `ConfigTable`. Mark the field holding them with `#[confer(table)]`:

```rust
use conferencier::{ConfigStore, ConfigScope, ConfigTable};

#[derive(ConfigTable)]
struct Server {
    #[confer(default = "localhost".into())]
    host: String,
    #[confer(default = 8080)]
    port: u16,
}

#[derive(ConfigTable)]
struct Upstream {
    url: String,
    #[confer(default = 1)]
    weight: u32,
}

#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(table, field_defaults)]
    server: Server,
    #[confer(table, default)]
    upstream: Vec<Upstream>,
}

#[tokio::main]
async fn main() -> conferencier::Result<()> {
    let store = ConfigStore::from_str(
        r#"
[App.server]
port = 9000

[[App.upstream]]
url = "http://10.0.0.1"

[[App.upstream]]
url = "http://10.0.0.2"
weight = 3
"#,
    )?;
    let scope = AppConfig::from_store(&store)?;

    let config = scope.read().await;
    assert_eq!(config.server.host, "localhost");
    assert_eq!(config.server.port, 9000);
    assert_eq!(config.upstream[1].weight, 3);
    Ok(())
}
```

A table type takes the same field attributes as a scope (`rename`, `default`, `ignore`, `table` and `field_defaults`), so tables can be nested to any depth. A table field has one of these shapes, where `T` implements `ConfigTable`:

| Field type | TOML | Key missing on load | Save |
| --- | --- | --- | --- |
| `T` | table: `[App.server]` or `server = { ... }` | explicit `default` or `field_defaults`; otherwise `MissingKey` | always written |
| `Option<T>` | table | `None` | `None` removes the key |
| `Vec<T>` | array of tables: `[[App.upstream]]` or `upstream = [{ ... }]` | explicit `default`; otherwise `MissingKey` | always written, including as `upstream = []` |

- An empty `Vec<T>` is saved as `upstream = []`; TOML does not allow adding `[[App.upstream]]` entries after that array, so remove or replace it first.
- `#[confer(table, default)]` calls the table field's Rust `Default`, not its fields' `confer` defaults. For example, a derived `Default` gives `0` to a `u16` field even if that field has `#[confer(default = 8080)]`. To preserve the old missing-table behavior, use `#[confer(table, field_defaults)]` instead: it decodes an empty table, giving `8080` or reporting a required inner key. Apply the same migration to nested table fields.
- `field_defaults` also applies to manually implemented `ConfigTable`s, which receive an empty table and the nested diagnostic path. Derived strict tables check the empty table before converting its fields.
- `#[confer(table)]` cannot be combined with `ignore`. Other shapes such as `Option<Vec<T>>`, `Box<T>` or maps are not supported.
- Errors keep a formatted table `path` and a logical key; diagnostic output quotes dotted or otherwise non-bare TOML key segments. For example, a missing `a.b` key reports `missing key App."a.b"`. Scalar-array conversion errors include the zero-based element index, e.g. `invalid value at App.ports: value out of range for u16 (at index 1)`.
- Tables are plain values: every successful `load` builds them anew. Ignored fields inside a table type are therefore initialized again on each load (with `Default::default()` or their `ignore = <expr>` initializer), unlike ignored fields of the scope itself. If the load fails, the scope keeps its previous state, but side effects of initializers that have already run are not undone.
- One struct can derive both `ConfigScope` and `ConfigTable`; `section` then applies only to the scope. A table-only derive rejects `section`. `#[derive(ConfigScope)]` alone does not make a struct usable as a table field.
- `#[confer(section = "App.server")]` does not refer to the nested table `[App.server]`: it names a top-level section whose name contains a dot (saved as `["App.server"]`). A nested table always belongs to the scope of its top-level section.
- A generic table field needs an explicit bound such as `where T: ConfigTable`. `ConfigTable` can also be implemented manually; see its documentation. Manual implementations can build the same diagnostic paths as the derive with `conferencier::table_path::{section_path, child_path, element_path}`, which are available without the `derive` feature.
- A strict outer struct does not make its nested table types strict. Add `#[confer(deny_unknown_keys)]` to each nested `ConfigTable` that should reject unknown keys; conversely, a strict table type remains strict wherever it is used.
