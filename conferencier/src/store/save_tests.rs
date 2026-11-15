use super::*;
use std::ffi::OsString;
use std::io::Read;

fn entries(path: &Path) -> std::io::Result<Vec<OsString>> {
    let mut entries = std::fs::read_dir(path)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    Ok(entries)
}

fn io_error_at(error: Error, path: &Path, kind: ErrorKind) -> std::io::Error {
    match error {
        Error::Io {
            path: error_path,
            source,
        } => {
            assert_eq!(error_path.as_deref(), Some(path));
            assert_eq!(source.kind(), kind);
            source
        }
        other => panic!("unexpected error: {other}"),
    }
}

#[test]
fn metadata_failure_stops_before_temporary_creation() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "original")?;
    for source in [
        std::io::Error::new(ErrorKind::PermissionDenied, "metadata denied"),
        std::io::Error::from_raw_os_error(123),
    ] {
        let kind = source.kind();
        let code = source.raw_os_error();
        let message = source.to_string();
        let error = write_atomic_blocking_with(
            &path,
            b"replacement",
            Err(source),
            |_, _| panic!("metadata failure must stop preparation"),
            |_, _| panic!("metadata failure must stop writing"),
            |_, _| panic!("metadata failure must stop publication"),
        )
        .unwrap_err();
        let source = io_error_at(error, &path, kind);
        assert_eq!(source.raw_os_error(), code);
        assert_eq!(source.to_string(), message);
        assert_eq!(std::fs::read(&path)?, b"original");
        assert_eq!(entries(dir.path())?, ["config.toml"]);
    }
    Ok(())
}

#[tokio::test]
async fn save_file_rejects_directory_without_creating_temporary_file() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("directory");
    std::fs::create_dir(&path)?;
    let store = ConfigStore::new();
    store.set_integer("App", "value", 42)?;
    let source = io_error_at(
        store.save_file(&path).await.unwrap_err(),
        &path,
        ErrorKind::InvalidInput,
    );
    assert!(source.to_string().contains("directory"));
    assert!(path.is_dir());
    assert!(entries(&path)?.is_empty());
    assert_eq!(entries(dir.path())?, ["directory"]);
    Ok(())
}

#[test]
fn temporary_collisions_retry_without_opening_or_removing_existing_entries() -> Result<()> {
    for collisions in [1, 16] {
        let dir = tempfile::tempdir()?;
        for index in 1..=collisions {
            std::fs::write(
                dir.path().join(format!("{index}.tmp")),
                format!("stale {index}"),
            )?;
        }
        let mut attempts = 0;
        let result = create_temporary(None, || {
            attempts += 1;
            dir.path().join(format!("{attempts}.tmp"))
        });
        if collisions == 16 {
            let source = io_error_at(
                result.unwrap_err(),
                &dir.path().join("16.tmp"),
                ErrorKind::AlreadyExists,
            );
            assert!(source.raw_os_error().is_some());
            assert_eq!(attempts, 16);
        } else {
            let (path, file) = result?;
            assert_eq!(attempts, 2);
            assert_eq!(path, dir.path().join("2.tmp"));
            assert_eq!(file.metadata()?.len(), 0);
            drop(file);
            std::fs::remove_file(path)?;
        }
        for index in 1..=collisions {
            assert_eq!(
                std::fs::read_to_string(dir.path().join(format!("{index}.tmp")))?,
                format!("stale {index}")
            );
        }
        assert_eq!(entries(dir.path())?.len(), collisions);
    }
    Ok(())
}

#[test]
fn missing_target_stays_absent_during_preparation_and_partial_writes() -> Result<()> {
    for fail_write in [false, true] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        let control_path = dir.path().join("control");
        let control = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&control_path)?;
        #[cfg(unix)]
        let expected_mode = {
            use std::os::unix::fs::PermissionsExt;
            control.metadata()?.permissions().mode() & 0o777
        };
        drop(control);
        let mut renamed = false;
        let result = write_atomic_blocking_with(
            &path,
            b"complete snapshot",
            std::fs::symlink_metadata(&path),
            |file, metadata| {
                assert!(metadata.is_none());
                assert_eq!(
                    std::fs::symlink_metadata(&path).unwrap_err().kind(),
                    ErrorKind::NotFound
                );
                assert_eq!(file.metadata()?.len(), 0);
                prepare_temporary(file, metadata)
            },
            |file, contents| {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(file.metadata()?.permissions().mode() & 0o777, expected_mode);
                }
                file.write_all(&contents[..1])?;
                assert_eq!(file.metadata()?.len(), 1);
                assert_eq!(
                    std::fs::symlink_metadata(&path).unwrap_err().kind(),
                    ErrorKind::NotFound
                );
                if fail_write {
                    return Err(std::io::Error::new(ErrorKind::WriteZero, "write failed"));
                }
                file.write_all(&contents[1..])
            },
            |from, to| {
                renamed = true;
                assert_eq!(std::fs::read(from)?, b"complete snapshot");
                assert_eq!(
                    std::fs::symlink_metadata(to).unwrap_err().kind(),
                    ErrorKind::NotFound
                );
                std::fs::rename(from, to)
            },
        );
        if fail_write {
            match result.unwrap_err() {
                Error::Io {
                    path: Some(temporary_path),
                    source,
                } => {
                    assert_eq!(source.kind(), ErrorKind::WriteZero);
                    assert_eq!(source.to_string(), "write failed");
                    assert_eq!(temporary_path.parent(), path.parent());
                    assert!(!temporary_path.exists());
                }
                other => panic!("unexpected error: {other}"),
            }
            assert!(!renamed);
            assert!(!path.exists());
            assert_eq!(entries(dir.path())?, ["control"]);
        } else {
            result?;
            assert!(renamed);
            assert_eq!(std::fs::read(&path)?, b"complete snapshot");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&path)?.permissions().mode() & 0o777,
                    expected_mode
                );
            }
            assert_eq!(entries(dir.path())?, ["config.toml", "control"]);
        }
    }
    Ok(())
}

#[test]
fn cleanup_after_publication_error_handles_missing_file_and_reports_failure() -> Result<()> {
    for leave_directory in [false, true] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "original")?;
        let error = write_atomic_blocking_with(
            &path,
            b"replacement",
            std::fs::symlink_metadata(&path),
            prepare_temporary,
            Write::write_all,
            |from, _| {
                std::fs::remove_file(from)?;
                if leave_directory {
                    std::fs::create_dir(from)?;
                }
                Err(std::io::Error::from_raw_os_error(123))
            },
        )
        .unwrap_err();
        let save_error = if leave_directory {
            match error {
                Error::SaveFileCleanup {
                    save_error,
                    temporary_path,
                    cleanup_error,
                } => {
                    assert_eq!(temporary_path.parent(), path.parent());
                    assert!(temporary_path.is_dir());
                    assert!(cleanup_error.raw_os_error().is_some());
                    std::fs::remove_dir(temporary_path)?;
                    *save_error
                }
                other => panic!("unexpected error: {other}"),
            }
        } else {
            error
        };
        let source = io_error_at(
            save_error,
            &path,
            std::io::Error::from_raw_os_error(123).kind(),
        );
        assert_eq!(source.raw_os_error(), Some(123));
        assert_eq!(std::fs::read(&path)?, b"original");
        assert_eq!(entries(dir.path())?, ["config.toml"]);
    }
    Ok(())
}

#[tokio::test]
async fn dropping_waiter_does_not_cancel_started_blocking_save() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    let save_path = path.clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let waiter = tokio::task::spawn_blocking(move || {
        let result = write_atomic_blocking_with(
            &save_path,
            b"complete snapshot",
            std::fs::symlink_metadata(&save_path),
            |file, metadata| {
                prepare_temporary(file, metadata)?;
                started_tx.send(()).expect("started receiver dropped");
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(10))
                    .expect("save was not released");
                Ok(())
            },
            Write::write_all,
            |from, to| std::fs::rename(from, to),
        );
        finished_tx.send(result).expect("finished receiver dropped");
    });
    started_rx.await.expect("blocking save did not start");
    assert!(!path.exists());
    drop(waiter);
    release_tx.send(()).expect("blocking save stopped");
    finished_rx.await.expect("blocking save did not finish")?;
    assert_eq!(std::fs::read(&path)?, b"complete snapshot");
    assert_eq!(entries(dir.path())?, ["config.toml"]);
    Ok(())
}

#[tokio::test]
async fn save_file_replaces_only_one_hard_link_and_preserves_open_handles() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    let alias = dir.path().join("alias.toml");
    std::fs::write(&path, "original")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))?;
    }
    std::fs::hard_link(&path, &alias)?;
    let mut old_handle = std::fs::File::open(&path)?;
    let store = ConfigStore::new();
    store.set_integer("App", "value", 42)?;
    store.save_file(&path).await?;
    assert_eq!(std::fs::read_to_string(&path)?, store.save_str()?);
    assert_eq!(std::fs::read(&alias)?, b"original");
    let mut old_contents = String::new();
    old_handle.read_to_string(&mut old_contents)?;
    assert_eq!(old_contents, "original");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&path)?.permissions().mode() & 0o777,
            0o640
        );
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        assert_eq!(
            std::fs::metadata(&alias)?.permissions().mode() & 0o777,
            0o640
        );
        assert_eq!(old_handle.metadata()?.permissions().mode() & 0o777, 0o640);
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn save_file_rejects_final_symlinks_including_dangling_links() -> Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    for dangling in [false, true] {
        let dir = tempfile::tempdir()?;
        let target = dir.path().join("target.toml");
        let link = dir.path().join("link.toml");
        if !dangling {
            std::fs::write(&target, "original")?;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600))?;
        }
        symlink(&target, &link)?;
        let before = entries(dir.path())?;
        let store = ConfigStore::new();
        store.set_integer("App", "value", 42)?;
        let source = io_error_at(
            store.save_file(&link).await.unwrap_err(),
            &link,
            ErrorKind::InvalidInput,
        );
        assert!(source.to_string().contains("symlink"));
        assert_eq!(std::fs::read_link(&link)?, target);
        if dangling {
            assert!(!target.exists());
        } else {
            assert_eq!(std::fs::read(&target)?, b"original");
            assert_eq!(
                std::fs::metadata(&target)?.permissions().mode() & 0o777,
                0o600
            );
        }
        assert_eq!(entries(dir.path())?, before);
    }
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn save_file_rejects_socket_without_creating_temporary_file() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&path)?;
    let error = ConfigStore::new().save_file(&path).await.unwrap_err();
    let source = io_error_at(error, &path, ErrorKind::InvalidInput);
    assert!(source.to_string().contains("socket"));
    assert_eq!(entries(dir.path())?, ["socket"]);
    Ok(())
}

#[cfg(unix)]
#[tokio::test]
async fn save_file_through_parent_symlink_preserves_observed_mode() -> Result<()> {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let dir = tempfile::tempdir()?;
    let real = dir.path().join("real");
    let link = dir.path().join("link");
    std::fs::create_dir(&real)?;
    let path = real.join("config.toml");
    std::fs::write(&path, "original")?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640))?;
    symlink(&real, &link)?;
    let store = ConfigStore::new();
    store.set_integer("App", "value", 42)?;
    store.save_file(link.join("config.toml")).await?;
    assert_eq!(std::fs::read_to_string(&path)?, store.save_str()?);
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o640
    );
    assert_eq!(std::fs::read_link(&link)?, real);
    assert_eq!(entries(&real)?, ["config.toml"]);
    Ok(())
}

#[cfg(unix)]
#[test]
fn existing_unix_modes_are_established_before_writing_through_open_handle() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    // BSD chmod rejects the sticky bit on regular files for unprivileged users.
    for mode in [0o600, 0o640, 0o751, 0o444, 0o400, 0o000, 0o4640] {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "original")?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))?;
        let mut wrote = false;
        write_atomic_blocking_with(
            &path,
            b"replacement",
            std::fs::symlink_metadata(&path),
            prepare_temporary,
            |file, contents| {
                assert_eq!(file.metadata()?.len(), 0);
                assert_eq!(file.metadata()?.permissions().mode() & 0o7777, mode & 0o777);
                wrote = true;
                file.write_all(contents)
            },
            |from, to| std::fs::rename(from, to),
        )?;
        let final_mode = std::fs::metadata(&path)?.permissions().mode() & 0o7777;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        assert!(wrote);
        assert_eq!(final_mode, mode & 0o777);
        assert_eq!(std::fs::read(&path)?, b"replacement");
        assert_eq!(entries(dir.path())?, ["config.toml"]);
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn permission_preparation_failure_stops_writing_and_cleans_empty_temporary() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "original")?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let error = write_atomic_blocking_with(
        &path,
        b"replacement",
        std::fs::symlink_metadata(&path),
        |file, metadata| {
            assert!(metadata.is_some());
            assert_eq!(file.metadata()?.len(), 0);
            assert_eq!(file.metadata()?.permissions().mode() & 0o177, 0);
            Err(std::io::Error::from_raw_os_error(123))
        },
        |_, _| panic!("permission failure must stop writing"),
        |_, _| panic!("permission failure must stop publication"),
    )
    .unwrap_err();
    match error {
        Error::Io {
            path: Some(temporary_path),
            source,
        } => {
            assert_eq!(source.raw_os_error(), Some(123));
            assert_eq!(temporary_path.parent(), path.parent());
            assert!(!temporary_path.exists());
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(std::fs::read(&path)?, b"original");
    assert_eq!(
        std::fs::metadata(&path)?.permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(entries(dir.path())?, ["config.toml"]);
    Ok(())
}

#[cfg(windows)]
#[test]
fn simulated_windows_reparse_attribute_rejects_regular_looking_metadata() -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let file = tempfile::NamedTempFile::new()?;
    let metadata = file.as_file().metadata()?;
    assert!(metadata.is_file());
    let error = validate_windows_attributes(file.path(), metadata.file_attributes() | 0x00000400)
        .unwrap_err();
    let source = io_error_at(error, file.path(), ErrorKind::InvalidInput);
    assert!(source.to_string().contains("reparse point"));
    Ok(())
}

#[cfg(windows)]
#[tokio::test]
async fn save_file_rejects_junction_but_allows_junction_in_parent_path() -> Result<()> {
    use std::os::windows::fs::MetadataExt;
    let dir = tempfile::tempdir()?;
    let real = dir.path().join("real");
    let junction = dir.path().join("junction");
    std::fs::create_dir(&real)?;
    let path = real.join("config.toml");
    std::fs::write(&path, "original")?;
    let output = std::process::Command::new(std::env::var_os("ComSpec").expect("ComSpec is set"))
        .args(["/C", "mklink", "/J"])
        .arg(&junction)
        .arg(&real)
        .output()?;
    assert!(
        output.status.success(),
        "could not create junction: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let before = entries(dir.path())?;
    let store = ConfigStore::new();
    store.set_integer("App", "value", 42)?;
    let error = store.save_file(&junction).await.unwrap_err();
    let source = io_error_at(error, &junction, ErrorKind::InvalidInput);
    assert!(source.to_string().contains("reparse point"));
    assert_eq!(std::fs::read(&path)?, b"original");
    assert_eq!(entries(dir.path())?, before);
    store.save_file(junction.join("config.toml")).await?;
    assert_eq!(std::fs::read_to_string(&path)?, store.save_str()?);
    assert_ne!(
        std::fs::symlink_metadata(&junction)?.file_attributes() & 0x00000400,
        0
    );
    assert_eq!(entries(&real)?, ["config.toml"]);
    std::fs::remove_dir(junction)?;
    Ok(())
}

#[cfg(windows)]
#[tokio::test]
async fn save_file_accepts_read_only_regular_target_without_clearing_attribute() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "original")?;
    let original_permissions = std::fs::metadata(&path)?.permissions();
    let mut permissions = original_permissions.clone();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions)?;
    let metadata = validate_target_metadata(&path, std::fs::symlink_metadata(&path))?;
    assert!(metadata.is_some());
    let store = ConfigStore::new();
    store.set_integer("App", "value", 42)?;
    let result = store.save_file(&path).await;
    let contents = std::fs::read_to_string(&path)?;
    let was_readonly = std::fs::metadata(&path)?.permissions().readonly();
    std::fs::set_permissions(&path, original_permissions)?;
    match result {
        Ok(()) => assert_eq!(contents, store.save_str()?),
        Err(error) => {
            match error {
                Error::Io {
                    path: error_path,
                    source,
                } => {
                    assert_eq!(error_path.as_deref(), Some(path.as_path()));
                    assert!(source.raw_os_error().is_some());
                }
                other => panic!("unexpected error: {other}"),
            }
            assert_eq!(contents, "original");
            assert!(was_readonly);
        }
    }
    assert_eq!(entries(dir.path())?, ["config.toml"]);
    Ok(())
}
