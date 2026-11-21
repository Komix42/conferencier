# Example

```rust
use conferencier::{ConfigTable, Error};
use conferencier::toml::Table;

#[derive(ConfigTable)]
struct Server {
    #[confer(default = "localhost".into())]
    host: String,
    port: u16,
}

let table: Table = "port = 8080".parse().unwrap();
let server = Server::from_table("App.server", table)?;
assert_eq!(server.host, "localhost");
assert_eq!(server.port, 8080);
assert_eq!(server.to_table("App.server")?.len(), 2);
# Ok::<(), Error>(())
```
