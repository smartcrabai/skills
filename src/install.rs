use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::registry::Method;

/// Copy `src` -> `dest` recursively, replacing any existing destination.
///
/// Used to populate the master store. Always a deep copy regardless of
/// `Method`, because the master is the canonical local backing store.
///
/// # Errors
///
/// Returns [`Error::Io`] on any I/O failure.
pub fn install_to_master(src: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        remove_path(dest)?;
    }
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_recursive(src, dest)
}

/// For each `agent_dir` in `agent_dirs`, place either a symlink or a deep copy
/// of `master` at `agent_dir/<basename(master)>`. The "skill name" is taken
/// from the trailing component of `master`.
///
/// # Errors
///
/// Returns [`Error::Io`] on any I/O failure, or [`Error::ConfigError`] if
/// `master` has no file name.
pub fn link_into_agents(master: &Path, agent_dirs: &[PathBuf], method: Method) -> Result<()> {
    let name = master
        .file_name()
        .ok_or_else(|| {
            Error::ConfigError(format!("master path has no name: {}", master.display()))
        })?
        .to_owned();
    for dir in agent_dirs {
        fs::create_dir_all(dir)?;
        let dest = dir.join(&name);
        if dest.exists() || dest.is_symlink() {
            remove_path(&dest)?;
        }
        match method {
            Method::Symlink => symlink(master, &dest)?,
            Method::Copy => copy_recursive(master, &dest)?,
        }
    }
    Ok(())
}

/// Remove the named entry from each agent directory.
///
/// Missing entries are silently ignored.
///
/// # Errors
///
/// Returns [`Error::Io`] on filesystem failure (other than not-found).
pub fn uninstall_from_agents(agent_dirs: &[PathBuf], name: &str) -> Result<()> {
    for dir in agent_dirs {
        let dest = dir.join(name);
        if dest.is_symlink() || dest.exists() {
            remove_path(&dest)?;
        }
    }
    Ok(())
}

/// Remove the master directory if it exists.
///
/// # Errors
///
/// Returns [`Error::Io`] on filesystem failure.
pub fn remove_master(path: &Path) -> Result<()> {
    if path.is_symlink() || path.exists() {
        remove_path(path)?;
    }
    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    let meta = fs::symlink_metadata(path)?;
    if meta.file_type().is_symlink() || meta.is_file() {
        fs::remove_file(path)?;
    } else {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn copy_recursive(src: &Path, dest: &Path) -> Result<()> {
    let src_meta = fs::symlink_metadata(src)?;
    if src_meta.is_file() {
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dest)?;
        return Ok(());
    }
    fs::create_dir_all(dest)?;
    for entry in WalkDir::new(src).min_depth(1) {
        let entry = entry.map_err(io_from_walkdir)?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| Error::Io(std::io::Error::other(format!("strip_prefix failed: {e}"))))?;
        let target = dest.join(rel);
        let ft = entry.file_type();
        if ft.is_dir() {
            fs::create_dir_all(&target)?;
        } else if ft.is_symlink() {
            let link_target = fs::read_link(entry.path())?;
            if target.exists() || target.is_symlink() {
                remove_path(&target)?;
            }
            symlink(link_target, &target)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn io_from_walkdir(e: walkdir::Error) -> Error {
    Error::Io(
        e.into_io_error()
            .unwrap_or_else(|| std::io::Error::other("walkdir error")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn copy_recursive_round_trip() {
        let src = tempdir().expect("tmp src");
        fs::create_dir_all(src.path().join("a")).expect("mkdir");
        fs::write(src.path().join("a/file.txt"), b"hi").expect("write");
        let dst = tempdir().expect("tmp dst");
        let target = dst.path().join("copy");
        install_to_master(src.path(), &target).expect("install");
        assert_eq!(fs::read(target.join("a/file.txt")).expect("read"), b"hi");
    }

    #[test]
    fn link_symlink_and_copy() {
        let master_root = tempdir().expect("tmp");
        let master = master_root.path().join("foo");
        fs::create_dir_all(&master).expect("mkdir");
        fs::write(master.join("SKILL.md"), b"#").expect("write");

        let agents_root = tempdir().expect("tmp");
        let a1 = agents_root.path().join("agent1");
        let a2 = agents_root.path().join("agent2");
        link_into_agents(&master, &[a1.clone()], Method::Symlink).expect("symlink");
        assert!(
            fs::symlink_metadata(a1.join("foo"))
                .expect("meta")
                .file_type()
                .is_symlink()
        );

        link_into_agents(&master, &[a2.clone()], Method::Copy).expect("copy");
        assert!(a2.join("foo").is_dir());
        assert!(
            !fs::symlink_metadata(a2.join("foo"))
                .expect("meta")
                .file_type()
                .is_symlink()
        );
    }
}
