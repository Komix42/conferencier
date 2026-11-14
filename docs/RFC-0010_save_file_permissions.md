# RFC-0010: File permissions in `ConfigStore::save_file`

Date: 2026-11-14
Status: Draft
Author: Komix42 <komix[at]ignore.cz>
Target release: 0.0.6
Related: [RFC-0009](./RFC-0009_save_file_replacement.md)

## Document Scope

This RFC proposes the access-control policy for the temporary sibling and
resulting file written by [`ConfigStore::save_file`](../conferencier/src/store.rs).
It covers replacing an existing target and creating a new one. It does not
specify preservation of all filesystem metadata.

Today the temporary file is created with the operating system's default file
permissions and then renamed over the target. Consequently, an existing
target's permissions are replaced along with its contents. The temporary file
also contains the serialized configuration before it is renamed.

## Motivation

A save can silently widen access to configuration data. For example, replacing
a Unix file with mode `0600` under umask `022` can install a file readable by
others. On Windows, the new file can inherit access from its parent directory
instead of retaining a more restrictive target DACL. In either case, the
temporary file's access policy matters while its contents are being written,
not only after replacement.

The Unix behavior follows from the current create-and-rename code path; the
Windows behavior has been reproduced during review. The exact policy and
guarantees should be verified per platform before implementation.

## Goals and non-goals

Goals:

- Define separate permission behavior for a new target and an existing target.
- On Unix, use the existing target's observed permission mode bits for the
  temporary and resulting file, as agreed in R1 and R2.
- Document precisely which access settings are and are not preserved, so that
  users do not expect broader guarantees.
- Test the supported permission guarantees.

Non-goals:

- Guaranteeing unchanged effective access to the target or temporary file:
  ownership and ACLs can make access broader or narrower despite equal bits.
- Preserving ownership, extended ACLs, special mode bits, the Windows security
  descriptor or other metadata, as listed in R1.
- Preventing access that is already allowed by the intended final permissions.
- Changing the public API.
- Adding dependencies or operating-system APIs beyond the Rust standard
  library for access control.

## Proposed policy

For a **new target**, create the temporary file using the operating system's
normal file-creation policy in the target directory, then publish it by rename.
That policy may include a Unix umask or default ACL, or a Windows inherited
or token-default DACL. The resulting access settings are subject to the
platform and filesystem's creation and rename behavior.

For an **existing target**, the preserved access settings are limited to those
agreed in R1: Unix permission mode bits observed during preparation. Before
writing data, set those bits on the temporary file. This does not guarantee
equivalent effective access: ownership and ACLs can differ during staging and
after publication. On Windows, no access settings are copied; the temporary
file uses normal creation policy and is published by rename. Failures to read
or apply the required permissions are handled as specified in R3.

## Alternatives considered

- **Document that callers must restrict the parent directory.** Simple, but
  does not preserve a restrictive existing target when its directory allows
  broader access, and leaves temporary-file exposure dependent on directory
  policy.
- **Use the temporary file's default permissions for every save.** This is the
  current behavior and can silently broaden access compared with the replaced
  target.
- **Apply the target's mode after writing the temporary file.** Simple, but
  the temporary file can be more accessible than the target while it contains
  serialized data. Rejected in R2.
- **Create the temporary file as owner-only (`0600`), then apply the final
  mode before publication.** This can limit access to partially written
  contents when the target's mode permits group or other readers. However,
  `0600` can be broader than a more restrictive target's mode, and mode bits
  alone do not control all ACL-based access. For a new target, deriving the
  normal final permissions is also nontrivial. R2 uses the observed target
  mode instead and permits users allowed by that mode to see the temporary
  file while it is being written, subject to ownership and ACL limitations.

## Compatibility

The `save_file` signature and serialized contents remain unchanged. Replacing
an existing Unix file should use its observed permission mode bits instead of
process defaults, subject to the concurrency limits below. Access settings
outside R1 keep their current behavior and must be documented. Some filesystems
or permission configurations may not support the proposed policy; their
behavior must be explicit, not a success-shaped fallback.

## Testing

- On Linux and macOS, verify that replacing an existing target preserves its
  `rwx` mode bits, including when a normal new file would be more permissive.
- On Unix, check that a new target's mode follows normal file creation in the
  same environment, and verify the temporary file's mode before writing, as
  required by R2.
- On Linux and macOS, verify that a final-component symlink to an existing
  regular file is replaced at the link path using the referent's observed mode,
  leaving the referent unchanged. Verify that a dangling link follows the
  `NotFound` policy, uses the normal new-file mode and is replaced on a
  successful save without creating the missing referent.
- Deterministically simulate metadata-read and permission-setting failures.
  Verify that no data is written, the target remains unchanged, and the error
  and any cleanup attempt follow R3 and RFC-0009. Check that the I/O error
  retains the relevant target or temporary path and underlying cause. Use a
  narrow test seam if needed rather than relying on runner permissions or timing.
- On Windows, run the applicable cross-platform saving and error tests; do not
  claim they verify DACL inheritance, preservation or other settings excluded
  by R1. The default-DACL behavior relies on platform documentation, not these
  CI tests.
- Do not change the test runner's process-global umask. If a specific umask
  is needed, isolate it in a subprocess.

## Documentation deliverables

- Update the `save_file` rustdoc and README saving guidance with the exact
  supported permission guarantees and every limitation listed in R1.
- Update the changelog if the user-visible permission behavior changes.

## Resolved questions

**R1 — Scope of preserved permissions.** When replacing an existing regular
file on Unix, use the read, write and execute permission bits for owner, group
and others (`0o777`) observed when reading its metadata. A new target uses the
operating system's normal creation policy. On Windows, no access settings,
including the DACL, are copied from the replaced file. The project does not add
Windows APIs or dependencies for this in 0.0.6.

This preserves basic Unix mode bits, not effective access. Different ownership
or ACLs can allow additional users to read the temporary file during writing
or the published file, or can deny previous users access. The following
limitations must be documented in the `save_file` rustdoc and aligned READMEs:

- **Ownership.** The new file's owner and group are assigned by the system's
  creation policy and can differ from the original's. With unchanged mode
  bits, a different owner or group can change who has access. For example,
  saving a user's `0600` file as `root` can leave a root-owned `0600` file
  the original user cannot read, absent other applicable privileges or ACLs.
- **Unix extended ACLs** of the replaced file are not copied. The new file
  can inherit ACL entries from the directory. For example, an inherited ACL
  entry may allow a named user to read a new `0640` file even if that user
  could not read the previous `0640` file.
- **Special mode bits** (`setuid`, `setgid`, sticky).
- **Windows access control.** The temporary file receives the security settings
  assigned by Windows at creation; the replacement's final settings depend on
  the platform's rename behavior. The old DACL is not copied, so access can
  become broader. Owner, SACL and file attributes are not preserved either.
  This includes the read-only attribute, even though Rust's standard library
  can read and set it. Read-only is not a DACL or protection against reading.
- **Other metadata**, such as timestamps.
- **New targets.** Normal creation and rename rules determine their settings.
  Callers that require private configuration files must arrange suitable
  access controls themselves, for example through directory permissions.
- **Unix symlinks and hard links.** Under RFC-0009 R4, a final-component
  symlink is replaced by a regular file on successful publication. If it points
  to an existing regular file, its observed mode bits are used for the new
  file; the referent is unchanged. For a dangling symlink, `metadata` returns
  `NotFound`, so normal new-file creation rules apply; publication replaces the
  link, not its missing referent. Other hard-link names and open handles keep
  the old file with its old permissions. Windows reparse points remain outside
  this guarantee.

Broader preservation, including Windows DACLs, is deferred until there is a
concrete need and would be specified by a later RFC.

**R2 — Preparing the temporary file.** On Unix, establish the temporary file's
mode before any serialized data is written:

1. Read the target's metadata, following a final-component symlink as in R1,
   before creating the temporary file.
2. If the target exists, create the temporary file exclusively with the
   observed permission bits (`mode & 0o777`). OS creation rules can restrict
   the requested mode; on Linux a default directory ACL may apply instead of
   the umask. Then set exactly those bits on the still-empty file through its
   open handle.
3. If the target does not exist (`NotFound`), create the temporary file as
   today, with the normal creation permissions of a new file.
4. Write the snapshot and publish it according to RFC-0009.

The temporary file has the observed mode bits before it contains data, and the
final mode is in place before publication, so no permission change is needed
after rename. Equality of mode bits does not ensure equivalent effective access.
This uses only the standard library (`OpenOptionsExt::mode`,
`File::set_permissions`).

The metadata is read once, before the temporary file is created. If the target
or its permissions change during the save, the earlier observation is used;
this includes concurrent saves, which RFC-0009 does not order. For example, two
processes can both observe a missing target and use different creation
policies; the later publication can replace a more restrictive file created
by the other process. On Windows, the temporary file is created as today;
effective access after rename is subject to platform behavior.

**R3 — Permission failures.** Any failure to read the target's metadata, other
than `NotFound`, or to set the temporary file's permissions fails the save
before serialized data is written:

- If reading the target's metadata fails, its mode is unknown, so the save
  stops before creating the temporary file. Continuing with default
  permissions could broaden access.
- If setting the mode on the still-empty temporary file fails, the file was
  created requesting the observed mode bits, and no data has been written.
  The save still fails because the exact mode required by R1 was not
  established; it makes no claim that effective access was no broader.

In both cases the target is not modified. If a temporary file was created,
attempt cleanup and return the error as specified in RFC-0009 R3. Callers
cannot opt into a fallback that proceeds with different permissions.

**R4 — CI verification.** Test library-controlled guarantees on applicable
platforms. Run the Unix mode-preservation, symlink and staging tests on both
Linux and macOS, exercise permission failures deterministically, and run
applicable cross-platform saving tests on Windows. Review the relevant GitHub
Actions results for all three platforms before marking this RFC implemented.

The Windows tests do not inspect DACLs: normal creation behavior is based on
the operating system's documented rules, and DACL behavior after rename is
platform-dependent rather than established by those tests. Passing tests do
not establish preservation of Windows DACLs, Unix extended ACLs or ownership,
or prove behavior on every filesystem and permission configuration. Those
limitations remain part of the documentation required by R1.

## References

- [Rust `metadata`](https://doc.rust-lang.org/1.85.0/std/fs/fn.metadata.html)
  and [`Permissions`](https://doc.rust-lang.org/1.85.0/std/fs/struct.Permissions.html).
- [Linux `umask(2)`](https://man7.org/linux/man-pages/man2/umask.2.html)
  and [`acl(5)`](https://man7.org/linux/man-pages/man5/acl.5.html).
- [Windows DACL for a new object](https://learn.microsoft.com/en-us/windows/win32/secauthz/dacl-for-a-new-object).
