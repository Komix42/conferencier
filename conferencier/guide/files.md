Conferencier is primarily meant for reading configuration. Saving works, but it rewrites the configuration rather than editing it:

- `ConfigStore::save_str` and `ConfigStore::save_file` regenerate the whole document from the stored values. Comments are lost, keys may be reordered, inline tables become `[section.table]` headers, and values are written in canonical notation (for example `0x1F90` becomes `8080` and `'text'` becomes `"text"`).
- A scope's `save` owns its section, including all tables nested in it: it removes every key and sub-table that is not mapped to a non-ignored field, at any depth (also inside tables mapped with `#[confer(table)]`). In lenient mode, a misspelled key such as `prot = 9090` is ignored on load (`port` falls back to its default) and deleted by the next save. A strict type rejects it on load, but save still replaces the section without rechecking. Other sections are not changed; keep data that no scope owns in a separate section.
- A computed default is saved like any other loaded value; later loads read that value instead of recomputing it. Use an `Option` with an application-level fallback if the value should stay dynamic after saving.
- A well-typed default does not necessarily round-trip; see [value conversion](crate::guide::derive#value-conversion) for the `f32` limitation.

If people edit the configuration file by hand, prefer to only load it, and use saving mainly to generate an initial configuration file.

## Replacing the file

[`ConfigStore::save_file`](crate::ConfigStore::save_file) never edits the target in place. Each call writes the serialized document to its own [temporary file](#temporary-files-and-reserved-names) in the target's directory, which then replaces the target with a single rename:

- On filesystems that provide atomic replacement, readers opening the target see either the old or the complete new file.
- Concurrent calls to the same file, even through cloned stores, never share a temporary file: each successful call installs one complete snapshot, but which call wins is unspecified. Serialize saves yourself if their order matters.
- A failed rename is not retried, and the target is never removed to make room for a retry. A rename error does not guarantee that the target is unchanged; its final state depends on the operating system and filesystem, and on NFS a server can perform the rename and still report a failure. On Windows, sharing conflicts can make the rename fail, for example with `PermissionDenied`. On Linux, replacing a single-file bind mount can fail with `EBUSY`.
- When a write or the rename fails, the temporary file is removed. If that removal also fails, `Error::SaveFileCleanup` carries both errors and the temporary path.
- Success means that writing and the rename completed, not that the data reached stable storage. Neither the temporary file nor the directory is synchronized, so a delayed write error (for example on a full disk with a filesystem that defers allocation, or on NFS) can go unreported, and a crash or power loss after a successful save can leave the target missing, empty or incomplete.
- Directory watchers can observe the temporary file being created and renamed.

These guarantees assume that no other process edits, deletes or replaces the target or changes its directory during a save. Predictable temporary names and path-based operations are no defense against an untrusted actor who can modify that directory.

### Cancellation and relative paths

The file operations of a save run as one task on Tokio's blocking thread pool, and a started blocking task cannot be aborted. Dropping the `save_file` future, for example after a timeout, therefore does not cancel the save: the task can still replace the target, and nobody receives its result or error. A lock that serializes saves must stay held until `save_file` returns; a lock released when the waiting future is dropped does not establish the order of saves.

A relative path is resolved against the process's current directory, which must not change from the first poll of `save_file` until its blocking task finishes, even after the future is dropped. A fully qualified absolute path avoids this dependency; on Windows, a drive-relative path such as `C:config.toml` is not fully qualified.

### Temporary files and reserved names

A temporary file is named `.conferencier.<process id>.<counter>.tmp`. The name does not depend on the target filename, so targets with long names, up to the filesystem's limit, can be saved. The `.conferencier.` prefix and `.tmp` suffix are stable; the format of the numeric parts is not. On Windows, the leading dot does not make the file hidden. A temporary file is always newly created: if its name already exists, the next counter value is tried without opening or removing the existing entry. After 16 attempts, saving returns `Error::Io` with `AlreadyExists` and the last temporary path.

Because filesystems can resolve different names to the same file, the library cannot reliably tell whether a temporary name refers to the target. The following final path components are therefore reserved and must not be used as `save_file` targets:

- a name beginning with `.conferencier.` in any letter case, as compared by the filesystem;
- on Windows, a name containing `~` with the extension `.tmp` in any letter case, which can be the 8.3 short name of a temporary file (for example `CONFER~1.TMP`);
- on Windows, either of the above followed by trailing dots or spaces, or by a `:` stream suffix such as `::$DATA`.

These names are not checked. Saving to a missing target with a reserved name may create the temporary file at the target path: partial contents can become visible there before the rename, and cleanup after a failed save can remove the file. An existing target with a reserved name is not overwritten while the snapshot is prepared, but is still unsupported, because users and tools may treat it as a leftover temporary file.

A temporary file can remain after a crash, after a failed cleanup, or while the task of a dropped `save_file` future is still running. A file named `.conferencier.*.tmp` can be removed when no save into that directory is in progress.

## File targets and permissions

`save_file` inspects the final component with `symlink_metadata` before creating a temporary file. It accepts a missing target or a regular file, but rejects final-component symlinks (including dangling links), directories, other non-regular files and every Windows reparse point, including regular-looking cloud placeholders carrying that attribute. Rejection returns `Error::Io` with `InvalidInput`, the target path and a reason; the target and any referent are left untouched when no external actor changes them. Other metadata errors are returned rather than treated as a missing target.

Links in parent directories, including Windows junctions, are allowed: saving to `config-link.toml` pointing to a file is rejected, while `directory-link/config.toml` can use a linked parent. The path is not canonicalized. `load_file` can still follow a final-component symlink; to save to its referent, explicitly resolve and pass that path. Regular files with multiple hard links are accepted, but saving replaces only the supplied name. Other hard-link names and already-open handles retain the original file's contents and permissions.

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
