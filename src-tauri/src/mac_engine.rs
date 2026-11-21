use rustix::fs::{CWD, RenameFlags, renameat_with};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tauri::Manager;

use crate::util::is_engine_dir;

const ENGINE_DIR: &str = "engine/zwift-offline";

pub fn prepare(app: &tauri::AppHandle) -> Result<Option<PathBuf>, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join(ENGINE_DIR);
    let runtime = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join(ENGINE_DIR);
    prepare_engine(&bundled, &runtime)
        .map_err(|error| format!("无法准备内置引擎（{}）：{error}", runtime.display()))
}

fn prepare_engine(bundled: &Path, runtime: &Path) -> io::Result<Option<PathBuf>> {
    if is_engine_dir(runtime) {
        return Ok(Some(runtime.to_path_buf()));
    }
    if !is_engine_dir(bundled) {
        return Ok(None);
    }
    match fs::symlink_metadata(runtime) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "目标已存在但引擎文件不完整，请检查该目录或手动选择已有引擎",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let parent = runtime.parent().expect("内置引擎的运行路径应包含父目录");
    fs::create_dir_all(parent)?;
    let staging = tempfile::Builder::new()
        .prefix(".zoffline-copy-")
        .tempdir_in(parent)?;
    copy_engine_files(bundled, staging.path())?;

    // 在同一文件系统内发布完整目录，并保证并发启动也不会替换目标。
    match renameat_with(CWD, staging.path(), CWD, runtime, RenameFlags::NOREPLACE) {
        Ok(()) => Ok(Some(runtime.to_path_buf())),
        Err(rustix::io::Errno::EXIST) if is_engine_dir(runtime) => Ok(Some(runtime.to_path_buf())),
        Err(error) => Err(error.into()),
    }
}

fn copy_engine_files(source: &Path, target: &Path) -> io::Result<()> {
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let dest = target.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            fs::create_dir(&dest)?;
            copy_engine_files(&entry.path(), &dest)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &dest)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "内置引擎含非普通文件",
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_bundled_engine_and_preserves_local_changes_on_restart() {
        let fixture = tempfile::tempdir().unwrap();
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join(ENGINE_DIR);
        let runtime = fixture.path().join("Application Support").join(ENGINE_DIR);

        assert_eq!(
            prepare_engine(&bundled, &runtime).unwrap(),
            Some(runtime.clone())
        );
        for file in [
            "standalone.py",
            "zwift_offline.py",
            "requirements.txt",
            "ssl/cert-zwift-com.pem",
        ] {
            assert_eq!(
                fs::read(bundled.join(file)).unwrap(),
                fs::read(runtime.join(file)).unwrap()
            );
        }

        let ride = runtime.join("storage/ride.fit");
        fs::write(&ride, b"ride data").unwrap();
        fs::write(runtime.join("standalone.py"), b"existing engine code").unwrap();

        assert_eq!(
            prepare_engine(&bundled, &runtime).unwrap(),
            Some(runtime.clone())
        );
        assert_eq!(fs::read(&ride).unwrap(), b"ride data");
        assert_eq!(
            fs::read(runtime.join("standalone.py")).unwrap(),
            b"existing engine code"
        );
    }

    #[test]
    fn preserves_existing_storage_when_the_engine_directory_is_incomplete() {
        let fixture = tempfile::tempdir().unwrap();
        let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join(ENGINE_DIR);
        let runtime = fixture.path().join(ENGINE_DIR);
        let ride = runtime.join("storage/ride.fit");
        fs::create_dir_all(ride.parent().unwrap()).unwrap();
        fs::write(&ride, b"ride data").unwrap();

        let error = prepare_engine(&bundled, &runtime).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&ride).unwrap(), b"ride data");
        assert!(!runtime.join("standalone.py").exists());
    }

    #[test]
    fn failed_copy_leaves_no_partial_engine_and_can_retry() {
        let fixture = tempfile::tempdir().unwrap();
        let bundled = fixture.path().join("bundle");
        let runtime = fixture.path().join(ENGINE_DIR);
        fs::create_dir(&bundled).unwrap();
        fs::write(bundled.join("standalone.py"), b"").unwrap();
        fs::write(bundled.join("zwift_offline.py"), b"").unwrap();
        let link = bundled.join("unexpected-link");
        std::os::unix::fs::symlink("standalone.py", &link).unwrap();

        assert!(prepare_engine(&bundled, &runtime).is_err());
        assert!(!runtime.exists());
        assert_eq!(fs::read_dir(runtime.parent().unwrap()).unwrap().count(), 0);
        fs::remove_file(link).unwrap();
        assert_eq!(prepare_engine(&bundled, &runtime).unwrap(), Some(runtime));
    }
}
