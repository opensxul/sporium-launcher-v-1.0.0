use super::{
    fs::{extract, java_path, read_limited, write_atomic},
    model::JavaRuntime,
    network::{Download, Hash, Network},
};
use crate::{
    error::CoreError,
    instances::filesystem::{Paths, no_links},
};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn command(executable: &Path) -> Command {
    let mut command = Command::new(executable);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    for key in [
        "JAVA_TOOL_OPTIONS",
        "JDK_JAVA_OPTIONS",
        "_JAVA_OPTIONS",
        "CLASSPATH",
    ] {
        command.env_remove(key);
    }
    command
}

pub fn inspect(executable: &Path, managed: bool) -> Result<JavaRuntime, CoreError> {
    if !executable.is_absolute()
        || !executable
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("java.exe"))
    {
        return Err(CoreError::JavaUnavailable);
    }
    no_links(executable)?;
    if !executable.is_file() {
        return Err(CoreError::JavaUnavailable);
    }
    let output = tempfile::tempfile()?;
    let mut child = command(executable)
        .args(["-XshowSettings:properties", "-version"])
        .stdout(Stdio::null())
        .stderr(output.try_clone()?)
        .spawn()
        .map_err(|_| CoreError::JavaUnavailable)?;
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                return Err(CoreError::JavaUnavailable);
            }
            break;
        }
        if start.elapsed() > Duration::from_secs(8) || output.metadata()?.len() > 100_000 {
            let _ = child.kill();
            let _ = child.wait();
            return Err(CoreError::JavaUnavailable);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    use std::io::{Seek, SeekFrom};
    let mut output = output;
    output.seek(SeekFrom::Start(0))?;
    let mut text = String::new();
    output.take(100_001).read_to_string(&mut text)?;
    let property = |key: &str| {
        text.lines().find_map(|line| {
            line.trim()
                .strip_prefix(key)
                .map(str::trim)
                .map(str::to_owned)
        })
    };
    let version = property("java.version =").ok_or(CoreError::JavaUnavailable)?;
    let architecture = property("os.arch =").ok_or(CoreError::JavaUnavailable)?;
    if !["amd64", "x86_64"].contains(&architecture.as_str()) {
        return Err(CoreError::JavaUnavailable);
    }
    let major = version
        .strip_prefix("1.")
        .unwrap_or(&version)
        .split(['.', '-', '_'])
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or(CoreError::JavaUnavailable)?;
    Ok(JavaRuntime {
        executable: java_path(&executable.canonicalize()?),
        major,
        version,
        architecture,
        managed,
    })
}

fn candidates(paths: &Paths) -> Vec<(PathBuf, bool)> {
    let mut result = Vec::new();
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        result.push((PathBuf::from(home).join("bin/java.exe"), false));
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path).take(128) {
            if directory.is_absolute() {
                result.push((directory.join("java.exe"), false));
            }
        }
    }
    for key in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(key) {
            for vendor in ["Eclipse Adoptium", "Java", "Microsoft", "Zulu"] {
                let root = PathBuf::from(&root).join(vendor);
                if no_links(&root).is_err() {
                    continue;
                }
                if let Ok(entries) = fs::read_dir(root) {
                    for entry in entries.flatten().take(64) {
                        result.push((entry.path().join("bin/java.exe"), false));
                    }
                }
            }
        }
    }
    if let Ok(entries) = fs::read_dir(paths.root().join("shared/runtimes")) {
        for entry in entries.flatten().take(64) {
            let record = entry.path().join("runtime.json");
            if let Ok(bytes) = read_limited(paths, &record, 16000)
                && let Ok(runtime) = serde_json::from_slice::<JavaRuntime>(&bytes)
            {
                let executable = PathBuf::from(runtime.executable);
                if executable
                    .canonicalize()
                    .is_ok_and(|path| path.starts_with(paths.root().join("shared/runtimes")))
                {
                    result.push((executable, true));
                }
            }
        }
    }
    result
}

pub fn discover(paths: &Paths) -> Vec<JavaRuntime> {
    let mut seen = std::collections::HashSet::new();
    candidates(paths)
        .into_iter()
        .filter(|(path, _)| path.is_file())
        .filter_map(|(path, managed)| {
            let canonical = path.canonicalize().ok()?;
            if !seen.insert(canonical.clone()) {
                return None;
            }
            inspect(&canonical, managed).ok()
        })
        .collect()
}

pub fn resolve(
    paths: &Paths,
    network: &Network,
    major: u32,
    custom: Option<&str>,
    cancelled: &impl Fn() -> bool,
    progress: &impl Fn(u64),
) -> Result<JavaRuntime, CoreError> {
    if !cfg!(all(windows, target_arch = "x86_64")) || !(8..=99).contains(&major) {
        return Err(CoreError::JavaUnavailable);
    }
    if let Some(custom) = custom {
        let runtime = inspect(Path::new(custom), false)?;
        return if runtime.major == major {
            Ok(runtime)
        } else {
            Err(CoreError::JavaUnavailable)
        };
    }
    if let Some(runtime) = discover(paths)
        .into_iter()
        .find(|runtime| runtime.major == major)
    {
        return Ok(runtime);
    }
    let response: Value = serde_json::from_slice(&network.bytes(&format!("https://api.adoptium.net/v3/assets/latest/{major}/hotspot?architecture=x64&image_type=jre&os=windows&vendor=eclipse"), 2_000_000)?)?;
    let item = response
        .as_array()
        .and_then(|items| items.first())
        .ok_or(CoreError::JavaUnavailable)?;
    if item.pointer("/version/major").and_then(Value::as_u64) != Some(major as u64) {
        return Err(CoreError::Integrity);
    }
    let package = item
        .pointer("/binary/package")
        .ok_or(CoreError::JavaUnavailable)?;
    let checksum = super::metadata::string(package, "checksum")?;
    if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Integrity);
    }
    let url = super::metadata::string(package, "link")?;
    if !url.starts_with(&format!(
        "https://github.com/adoptium/temurin{major}-binaries/releases/download/"
    )) || !url.ends_with(".zip")
    {
        return Err(CoreError::Integrity);
    }
    let destination = paths
        .root()
        .join("shared/runtimes")
        .join(format!("temurin-{major}-{}", &checksum[..16]));
    if destination.exists() {
        // Publishing the verified directory and recording it are separate atomic writes.
        // Recover a runtime if the launcher stopped between those writes.
        let runtime = inspect(&destination.join("bin/java.exe"), true)?;
        if runtime.major != major {
            return Err(CoreError::JavaUnavailable);
        }
        write_atomic(
            paths,
            &destination.join("runtime.json"),
            &serde_json::to_vec(&runtime)?,
        )?;
        return Ok(runtime);
    }
    let archive = paths
        .root()
        .join("shared/cache/java")
        .join(format!("{checksum}.zip"));
    let size = package
        .get("size")
        .and_then(Value::as_u64)
        .filter(|size| *size < 500_000_000)
        .ok_or(CoreError::Integrity)?;
    network.download(
        paths,
        &Download {
            url: url.into(),
            path: archive.clone(),
            hash: Hash::Sha256(checksum.into()),
            size,
        },
        cancelled,
        progress,
    )?;
    let stage = paths
        .root()
        .join("shared/cache")
        .join(format!("java-stage-{}", uuid::Uuid::new_v4()));
    let result = (|| {
        extract(paths, &archive, &stage, &[], cancelled)?;
        let mut roots = fs::read_dir(&stage)?.collect::<Result<Vec<_>, _>>()?;
        if roots.len() != 1 {
            return Err(CoreError::UnsafeArchive);
        }
        let root = roots.remove(0).path();
        let java = root.join("bin/java.exe");
        let verified = inspect(&java, true)?;
        if verified.major != major {
            return Err(CoreError::JavaUnavailable);
        }
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        paths.rename(&root, &destination)?;
        let runtime = inspect(&destination.join("bin/java.exe"), true)?;
        write_atomic(
            paths,
            &destination.join("runtime.json"),
            &serde_json::to_vec(&runtime)?,
        )?;
        Ok(runtime)
    })();
    let _ = paths.remove(&stage);
    result
}
