# RFC-0009: Reliable file saving with `ConfigStore::save_file`

Date: 2026-11-14
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.6
Related: [RFC-0010](./RFC-0010_save_file_permissions.md)

## Document Scope

This RFC reviews the complete [`ConfigStore::save_file`](../conferencier/src/store.rs)
protocol: capturing a snapshot, preparing a temporary file, publishing it,
handling failures and cancellation, and documenting its limits. It originated
in the [replacement fallback debt](./BACKLOG.md#technical-debt), but is not
limited to that fallback.

The proposed direction is to prepare a complete file in the target directory
and publish it by renaming it over the target, without a separate deletion.
This is a draft, not a claim that the current implementation meets every
proposed guarantee. Permission policy is specified separately in
[RFC-0010](./RFC-0010_save_file_permissions.md); the two drafts must be
reviewed together.

## Motivation and current behavior

The store serializes one consistent in-memory snapshot under a read lock. It
then releases the lock, creates a uniquely named temporary sibling with
`create_new`, writes the snapshot, closes the file, and renames it to the
target on Tokio's blocking pool.

This separates preparation from publication, but several details need a
clearer contract:

- If rename reports `AlreadyExists`, the current fallback removes the target
  and retries. A failed retry can lose the original, even without concurrent
  saves. When this branch occurs on supported filesystems remains unverified.
- The temporary file uses default permissions and can be more accessible than
  an existing target; replacement can also change the target's permissions.
- Cleanup after a write or rename failure is attempted, but cleanup errors are
  ignored. A crash can also leave a temporary file behind.
- Neither the file nor its containing directory is synchronized to storage.
  `File`'s destructor ignores close errors.
- Dropping the waiting future does not necessarily stop the blocking task.
  The caller may stop waiting before a save completes.

## Goals and boundaries

Goals:

- Define observable behavior for successful saves, errors and cancellation.
- Do not delete or truncate the existing target as a separate prerequisite to
  publishing its replacement.
- Preserve complete snapshot publication on filesystems whose replacement
  operation supports it, without promising more than those operations provide.
- Define temporary-file lifecycle and its interaction with RFC-0010.
- Verify behavior on the project's Windows, Linux and macOS CI platforms.

Proposed non-goals for 0.0.6:

- Ordering or coordinating writers, including across processes.
- Crash durability or recovery after power loss; no file or directory sync
  guarantee is proposed here. This boundary is agreed in R1 below.
- Universal atomicity, metadata preservation, or rollback after every error
  on every filesystem.
- New saving methods or configuration, automatic parent-directory creation,
  backups or a journal. R3 adds one public error variant.
- Defending against an adversary who can mutate entries in the target
  directory while a save is in progress.

## Proposed protocol

1. **Snapshot.** Serialize the in-memory document before file I/O, as today.
   The store lock is not held during file I/O. Changes made later are not in
   this snapshot; unsaved scope changes are not included.
2. **Prepare.** On Unix, read the target metadata as specified in RFC-0010
   before creating the temporary file. Create a new temporary sibling without
   opening an existing file, establish its access policy as agreed in
   RFC-0010, and write all snapshot bytes. The target is not deliberately
   modified during this phase.
3. **Publish.** Close the temporary handle as needed, then call
   `std::fs::rename` to publish the temporary file whether the target is
   missing or already exists. Do not select behavior through a prior
   `exists()` check, which can race with changes to the target. Do not add a
   fallback that removes the target before retrying.
4. **Finish.** Return success after publication succeeds. On a preparation or
   publication error, attempt to remove the temporary file by path and return
   the original or combined error according to R3. The method does not modify
   the in-memory store.

The temporary file is in the target directory so publication normally stays
on the same filesystem. Its name must remain independent of the target
filename, preserving support for long valid target names. Exclusive creation
prevents accidentally opening an existing temporary file; it does not by
itself protect a named temporary file against interference by an actor who can
change entries in the directory.

## Proposed observable contract

The following assumes no external actor changes the target or its parent
directory during the operation, except where concurrent saves are explicitly
discussed.

| Event | Proposed behavior and limits |
| --- | --- |
| Serialization fails | Return `Error::Serialize` before creating a temporary file or touching the target. |
| Permission preparation fails | Do not write snapshot data or modify the target. Attempt cleanup if a temporary file was created; return the error as specified in R3 and RFC-0010 R3. |
| Temporary creation or writing fails before publication | The library has not modified the target. Attempt cleanup if a temporary file was created; return the error as specified in R3. |
| Publication succeeds | The complete serialized snapshot was installed by the replacement operation. A later writer may replace it again before the caller observes it. |
| Publication returns an error | Do not explicitly remove the old target and retry. The final state is subject to the OS and filesystem; an error does not universally prove that publication did not happen. Attempt temporary-file cleanup and return the error as specified in R3. |
| Cleanup fails other than with `NotFound` | Return `Error::SaveFileCleanup` with both errors and the temporary path; the temporary file may remain. |
| Cleanup succeeds or returns `NotFound` | Return the original save error unchanged; `NotFound` does not establish whether publication occurred. |
| Waiting future is dropped or times out | The caller has no result. A dispatched blocking task can still run and publish the file; cancellation is not a rollback. |

The combined error is returned only while the caller is still waiting for the
result. Dropping the future or terminating the process can leave no caller to
receive an error; R3 does not introduce logging or another notification channel.

On filesystems that provide atomic replacement of a regular file, a reader
opening the target during publication sees either the old or the new file,
not an intermediate pathname gap created by the library. Existing open
handles can still refer to the old file. This is a visibility property, not a
guarantee that data or directory changes survive a crash.

`Ok(())` means publication completed, not that it is durable after power
loss. Since `File`'s destructor ignores close errors, `write_all` followed by
close also does not report every possible delayed storage error. This proposal
does not add `sync_all` or claim that all writes have reached stable storage.

An error before publication is different from an error returned by the
publication operation itself. POSIX specifies that the destination is
unaffected by rename failures other than `EIO`; on NFS a server can perform a
rename and then report failure after a retry. Consequently the RFC must not
promise that every failed publication leaves the original contents at the
target path.

## Paths, concurrency and cancellation

- Each save prepares its own temporary file. Successful concurrent saves can
  install different complete snapshots; their order is unspecified. Applications
  requiring ordering must coordinate their own saves. Permission decisions use
  the metadata observed during preparation, not necessarily the target's state
  at publication; see RFC-0010.
- These guarantees do not protect against another process editing, deleting or
  replacing the target, or changing the containing directory.
- The library publishes to the path exactly as supplied, without canonicalizing
  it to select a referent. On Unix, `load_file` through a final-component
  symlink follows it, but renaming over that path replaces the symlink itself
  with the new file; the referent remains unchanged. The metadata lookup for
  permissions does follow the link, as specified in RFC-0010. Other hard-link
  names and already-open handles can continue to refer to the old file. Callers
  that want to replace a symlink's referent must resolve and pass that path
  themselves. Windows reparse points and other non-regular targets have no
  additional guarantee beyond the behavior of the underlying platform operation.
- Dropping a Tokio `JoinHandle` detaches its task, and a started
  `spawn_blocking` task cannot be aborted. Thus a dropped `save_file` future
  is not evidence that no write happened. Runtime shutdown and queued tasks
  also require the limitations described by Tokio.

The intended security boundary is a directory whose entries are not being
manipulated by an untrusted actor during the save. A predictable temporary name
and path-based operations are not a defense against such an actor. File and
temporary-file access permissions within that boundary are addressed by
RFC-0010.

## Platform evidence and open verification

[`std::fs::rename`](https://doc.rust-lang.org/1.85.0/std/fs/fn.rename.html)
documents replacement of an existing destination and uses platform-specific
operations. POSIX specifies atomic pathname replacement for its `rename`
operation; this does not establish a universal filesystem or durability
guarantee.

A direct local probe on Windows/NTFS, using Rust 1.85 and the installed stable
toolchain, replaced an existing file successfully. With the destination held
without sharing, the probe returned `PermissionDenied` and the old contents
remained. This is evidence for those cases only: it does not prove that the
current `AlreadyExists` fallback is unreachable on every filesystem.

As an implementation acceptance condition, verify the chosen operation and
its error and visibility behavior through the project's test suite on Windows,
Linux and macOS. Try the implementation locally on Windows and review the
relevant GitHub Actions results before RFC-0009 is marked implemented. If CI
reveals a platform incompatibility, revisit this decision rather than
restoring the destructive fallback. Network or unusual filesystems may have
weaker or ambiguous failure semantics; do not infer their behavior from local
or CI tests on standard filesystems.

## Durability and alternatives

- **In-place overwrite:** preserves the file's identity but can leave partial
  contents after a write error or interruption. A cooperative lock does not
  make that write crash-safe.
- **Temporary file and rename:** the proposed baseline. It avoids modifying
  the target while preparing the snapshot, but has the platform, permissions
  and error limitations above.
- **A dedicated crate:** for example, [`tempfile::persist`](https://docs.rs/tempfile/3.23.0/tempfile/struct.NamedTempFile.html#method.persist)
  manages temporary files and documents atomic replacement. It adds a runtime
  dependency and its default Unix mode differs from the current behavior.
  Any candidate must also be assessed against RFC-0010 and the MSRV.
- **Windows `ReplaceFileW`:** can preserve a DACL on success, but does not
  protect the temporary file during writing. Microsoft documents failure
  cases in which the replaced file no longer has its original name or, without
  a backup, no longer exists; it is not automatically a stronger failure
  guarantee.
- **Backup or journal:** can support recovery, but requires additional
  files, recovery rules and error handling; it is not proposed for 0.0.6.
- **Synchronize file and directory:** strengthens crash durability on
  platforms where the required operations are supported, but has performance,
  portability and post-publication error implications. It is not required for
  the agreed 0.0.6 protection scope (R1).

## Compatibility and documentation

The proposed `save_file` signature and serialized format are unchanged.
Replacing an existing file may return an error where the old implementation
would attempt the destructive fallback. R3 adds a public `Error` variant:
the enum is non-exhaustive, but callers that handle only `Error::Io` must also
consider the combined-error case. Atomicity, cancellation, cleanup,
permissions, the documented Unix symlink behavior and durability must be
described accurately in the `save_file` and `Error` rustdoc and aligned root
and crate READMEs. Record user-visible changes in the changelog.

The permission policy is specified separately in RFC-0010. Implementation of
this protocol depends on resolving that policy, because it can affect the
preparation steps and their failure behavior. Neither RFC promises writer
coordination if another writer changes the target during a save.

## Testing

- Retain successful saves to new and existing targets, long target names and
  concurrent snapshot tests.
- Verify that `save_file` replaces an existing target with `std::fs::rename`
  on local Windows, then review the focused test results in GitHub Actions for
  Windows, Linux and macOS before marking the RFC implemented.
- Deterministically simulate a publication error such as `AlreadyExists`:
  assert the error is surfaced and the existing target is not removed by the
  library. Use a narrow test seam if needed rather than relying on a race.
- Cover failure before publication and cleanup attempts. Verify that successful
  cleanup and `NotFound` return the original error unchanged.
- Deterministically simulate both a save failure and a cleanup failure. Check
  that the combined error preserves the original error and its source, the
  temporary path and the cleanup error, and that its display reports both
  failures. Do not assert unconditional cleanup after a crash or failed
  cleanup operation.
- Test cancellation with controlled synchronization: pause a started blocking
  task, drop the waiter, then release the task and verify it can still publish.
- Document and test Unix final-component symlink behavior and its interaction
  with permissions under RFC-0010, including dangling links. No publication
  to the referent or cross-platform reparse-point guarantee is required.
- Run focused tests on Windows, Linux and macOS; document behavior that cannot
  be established by those tests.

## Resolved questions

**R1 — Protection scope for 0.0.6.** Prepare the complete new snapshot before
publication, without deliberately modifying the target during preparation.
Do not separately delete or truncate the target before replacing it. This
protects the existing file from the library's destructive fallback during
ordinary saving.

Crash durability is not a goal for 0.0.6: this decision does not require file
or directory synchronization and does not promise recovery after a crash or
power loss. Successful publication is not proof that the data has reached
stable storage. Nor does an error from the publication operation universally
prove that the old contents remain at the target path.

This keeps the protection appropriate for ordinary configuration saving
without committing the project to platform-specific durability and recovery
work. The replacement mechanism, cleanup, special paths and permissions are
addressed separately.

**R2 — Replacement operation.** Use `std::fs::rename` to publish the prepared
temporary file both when the target is absent and when it already exists.
Rely on the documented standard-library behavior rather than adding a
platform-specific API or dependency in advance. Do not check whether the
target exists before calling rename, and do not restore the remove-and-retry
fallback.

The implementation must be tested locally on Windows and its GitHub Actions
test results on Windows, Linux and macOS reviewed before this RFC is marked
implemented. If those results reveal an incompatibility, revisit R2 and
document the evidence; a CI failure is not a reason to silently delete the
target before retrying.

**R3 — Reporting cleanup failures.** If saving fails after creating a temporary
file, attempt to remove it. If removal succeeds or returns `NotFound`, return
the original save error unchanged. `NotFound` means there is no file to remove
at that path; it does not prove whether publication occurred.

If removal fails for another reason, return a new public
`Error::SaveFileCleanup` variant containing:

- `save_error: Box<Error>`: the original failure, retaining its path and
  underlying error; it is the variant's `std::error::Error::source`.
- `temporary_path: PathBuf`: the path the library attempted to clean up.
- `cleanup_error: std::io::Error`: the removal failure, available as a
  structured field.

The variant's display includes the original failure, temporary path and cleanup
failure. The original error remains the primary cause; the cleanup error is
inspectable separately rather than discarded or converted into text only. This
preserves the original error's kind and OS code and requires no logging
dependency. A failed cleanup does not guarantee that the temporary file still
exists when the caller inspects it.

**R4 — Path and symlink behavior.** `save_file` publishes to the path exactly
as provided; it does not canonicalize it to select the referent. On Unix,
`load_file` through a final-component symlink follows the link, while
publishing with rename replaces the link itself with the new file and leaves
its referent unchanged. The Unix permission metadata lookup follows the link,
as described in RFC-0010. Other hard-link names can continue to refer to the
old file. This behavior will be documented; 0.0.6 does not add publication to
the referent or preserve the link. A caller that intends to replace the
referent must resolve and pass its path. Behavior for Windows reparse points
and other non-regular targets remains unspecified beyond the underlying
platform operation.

## References

- [POSIX `rename`](https://pubs.opengroup.org/onlinepubs/9799919799/functions/rename.html)
  and [Linux `rename(2)` NFS caveat](https://man7.org/linux/man-pages/man2/rename.2.html).
- [Linux `fsync(2)`](https://man7.org/linux/man-pages/man2/fsync.2.html):
  synchronizing a file does not necessarily synchronize its directory entry.
- [Tokio `spawn_blocking`](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)
  and [`JoinHandle`](https://docs.rs/tokio/latest/tokio/task/struct.JoinHandle.html).
- [Microsoft `ReplaceFileW`](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-replacefilew):
  metadata handling and documented failure cases.
