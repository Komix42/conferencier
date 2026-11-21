# Example

```rust
use conferencier::{ConfigScope, ConfigStore};

#[derive(ConfigScope)]
#[confer(section = "App")]
struct AppConfig {
    #[confer(default = 8080)]
    port: u16,
    #[confer(ignore)]
    requests: u64,
}

let app = AppConfig::from_store(&ConfigStore::new())?;
let state = app.try_read().expect("no other guard exists");
assert_eq!(state.port, 8080);
assert_eq!(state.requests, 0);
# Ok::<(), conferencier::Error>(())
```
