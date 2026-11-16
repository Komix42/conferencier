Conferencier is primarily meant for reading configuration. Saving works, but it rewrites the configuration rather than editing it:

- `ConfigStore::save_str` and `ConfigStore::save_file` regenerate the whole document from the stored values. Comments are lost, keys may be reordered, inline tables become `[section.table]` headers, and values are written in canonical notation (for example `0x1F90` becomes `8080` and `'text'` becomes `"text"`).
- A scope's `save` owns its section, including all tables nested in it: it removes every key and sub-table that is not mapped to a non-ignored field, at any depth (also inside tables mapped with `#[confer(table)]`). In lenient mode, a misspelled key such as `prot = 9090` is ignored on load (`port` falls back to its default) and deleted by the next save. A strict type rejects it on load, but save still replaces the section without rechecking. Other sections are not changed; keep data that no scope owns in a separate section.
- A computed default is saved like any other loaded value; later loads read that value instead of recomputing it. Use an `Option` with an application-level fallback if the value should stay dynamic after saving.
- A well-typed default does not necessarily round-trip: a non-finite `f32` such as `f32::NAN` can be saved but is rejected on load.
- Concurrent `save_file` calls to the same file, even through cloned stores, never share a temporary file: each successful call installs one complete snapshot, but which call wins is unspecified. Serialize saves yourself if their order matters. Replacement and durability limitations are described in the [API reference](https://docs.rs/conferencier/latest/conferencier/struct.ConfigStore.html#method.save_file).

If people edit the configuration file by hand, prefer to only load it, and use saving mainly to generate an initial configuration file.

## File targets and permissions

`save_file` inspects the final component with `symlink_metadata` before creating a temporary file. It accepts a missing target or a regular file, but rejects final-component symlinks (including dangling links), directories, other non-regular files and every Windows reparse point, including regular-looking cloud placeholders carrying that attribute. Rejection returns `Error::Io` with `InvalidInput`, the target path and a reason; the target and any referent are left untouched when no external actor changes them. Other metadata errors are returned rather than treated as a missing target.

Links in parent directories are allowed: saving to `config-link.toml` pointing to a file is rejected, while `directory-link/config.toml` can use a linked parent. The path is not canonicalized. `load_file` can still follow a final-component symlink; to save to its referent, explicitly resolve and pass that path. Regular files with multiple hard links are accepted, but saving replaces only the supplied name. Other hard-link names and already-open handles retain the original file's contents and permissions.

On Unix, an existing target's observed owner/group/other `rwx` mode bits (`0o777`) are requested when creating the temporary file and set exactly through its open handle before writing any data. A read-only mode such as `0444` does not itself prevent replacement: saving requires suitable access to the containing directory, not just write access to the target. Metadata is read once, not atomically with publication; a later change to the target or its permissions is not revalidated.

New targets use the operating system's normal creation policy. On Windows, no access settings are copied from an existing target: the temporary file uses normal creation policy and its final access settings depend on rename behavior. A regular read-only target is accepted, but Windows may reject publication, for example with `PermissionDenied`; the library does not clear the attribute or retry.

Equal Unix mode bits do **not** guarantee equal effective access. The following are not preserved:

- **Ownership and Unix extended ACLs.** The owner and group follow creation policy, and directory ACL entries can be inherited. Saving a user's `0600` file as root can leave that user unable to read it; a named-user ACL entry can permit access to a new `0640` file that the old `0640` file denied.
- **Special mode bits:** setuid, setgid and sticky.
- **Windows security settings and attributes:** the old DACL, owner, SACL and file attributes, including read-only, are not copied. Access can become broader; read-only does not protect against reading.
- **Extended attributes, Windows alternate data streams, security labels and timestamps.** A security label assigned by system policy can differ and change access or deny it.

For a private new Unix target, pre-create an empty file once with `create_new` and mode `0600`; later saves preserve its observed mode:

```rust
# #[tokio::main]
# async fn main() -> Result<(), Box<dyn std::error::Error>> {
#[cfg(unix)]
{
    use std::os::unix::fs::OpenOptionsExt;

    let path = std::path::Path::new("config.toml");
#   let dir = tempfile::tempdir()?;
#   let path = dir.path().join(path);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    let store = conferencier::ConfigStore::from_file(&path).await?;
    store.set_integer("App", "port", 8080)?;
    store.save_file(&path).await?;
}
# Ok(())
# }
```

`create_new` fails instead of reusing an existing file; the pre-created empty file is valid empty TOML. If a file already exists, first confirm that it is the intended file, then explicitly set its permissions with `std::fs::set_permissions` and `PermissionsExt::from_mode(0o600)`. The umask can narrow the requested creation mode. On Linux, that mode also constrains the inherited POSIX ACL mask; on systems with ACLs independent of mode bits, such as macOS, review inherited ACL entries separately. This recipe is Unix-only: on Windows, private files require suitable directory permissions because creation uses inheritable directory ACEs or the token's default DACL.
