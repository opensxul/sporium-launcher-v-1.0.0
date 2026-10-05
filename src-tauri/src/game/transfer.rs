//! Resumable immutable artifact transfers. A .part is never an installed artifact.
use super::network::{Download, Hash, Network, hex, verify};
use crate::{error::CoreError, instances::filesystem::Paths};
use sha1::Digest;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub cached: bool,
    pub repaired: bool,
}
pub enum Event {
    Progress { received: u64 },
    Retry,
}

pub fn partial_path(paths: &Paths, item: &Download) -> Result<PathBuf, CoreError> {
    let (kind, hash, length) = match &item.hash {
        Hash::Sha1(h) => ("sha1", h, 40),
        Hash::Sha256(h) => ("sha256", h, 64),
        Hash::Sha512(h) => ("sha512", h, 128),
    };
    if hash.len() != length || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(CoreError::Integrity);
    }
    let key = format!("{kind}:{hash}:{}:{}", item.size, item.path.display());
    paths.checked(
        &paths
            .root()
            .join("shared/cache/parts")
            .join(format!("{}.part", hex(&sha1::Sha1::digest(key.as_bytes())))),
    )
}
fn backoff(delay: Duration, cancelled: &impl Fn() -> bool) -> Result<(), CoreError> {
    let start = std::time::Instant::now();
    while start.elapsed() < delay {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Ok(())
}

impl Network {
    pub fn download(
        &self,
        paths: &Paths,
        item: &Download,
        cancelled: &impl Fn() -> bool,
        progress: &impl Fn(u64),
    ) -> Result<Outcome, CoreError> {
        self.download_observed(paths, item, cancelled, progress, &|_| {})
    }
    pub fn download_observed(
        &self,
        paths: &Paths,
        item: &Download,
        cancelled: &impl Fn() -> bool,
        progress: &impl Fn(u64),
        event: &impl Fn(Event),
    ) -> Result<Outcome, CoreError> {
        if cancelled() {
            return Err(CoreError::Cancelled);
        }
        paths.checked(&item.path)?;
        let partial = partial_path(paths, item)?;
        if verify(&item.path, &item.hash, item.size)? {
            return Ok(Outcome {
                cached: true,
                repaired: false,
            });
        }
        let repaired = item.path.exists();
        paths.mkdir(partial.parent().ok_or(CoreError::UnsafePath)?)?;
        let _lease = paths.named_lock(&partial.with_extension("lock"))?;
        for attempt in 0..3 {
            if cancelled() {
                return Err(CoreError::Cancelled);
            }
            paths.checked(&partial)?;
            let length = partial.metadata().map(|m| m.len()).unwrap_or(0);
            if length > item.size
                || length == item.size && !verify(&partial, &item.hash, item.size)?
            {
                File::create(&partial)?.sync_all()?;
            }
            let result = (|| {
                let offset = partial.metadata().map(|m| m.len()).unwrap_or(0);
                event(Event::Progress { received: offset });
                if offset != item.size {
                    let mut response = self.transfer_response(&item.url, offset)?;
                    let status = response.status().as_u16();
                    if status == 416 {
                        File::create(&partial)?.sync_all()?;
                        return Err(CoreError::Integrity);
                    }
                    let append = status == 206;
                    if append {
                        let expected = format!(
                            "bytes {}-{}/{}",
                            offset,
                            item.size.saturating_sub(1),
                            item.size
                        );
                        if response
                            .headers()
                            .get("content-range")
                            .and_then(|h| h.to_str().ok())
                            != Some(expected.as_str())
                        {
                            return Err(CoreError::Integrity);
                        }
                    } else if status != 200 {
                        return Err(CoreError::Network);
                    }
                    let start = if append { offset } else { 0 };
                    if response
                        .content_length()
                        .is_some_and(|n| n != item.size - start)
                    {
                        return Err(CoreError::Integrity);
                    }
                    let mut file = OpenOptions::new()
                        .create(true)
                        .write(true)
                        .append(append)
                        .truncate(!append)
                        .open(&partial)?;
                    let mut received = start;
                    event(Event::Progress { received });
                    let mut buffer = [0; 65536];
                    loop {
                        if cancelled() {
                            file.sync_all()?;
                            return Err(CoreError::Cancelled);
                        }
                        let count = response.read(&mut buffer).map_err(|_| CoreError::Network)?;
                        if count == 0 {
                            break;
                        }
                        received += count as u64;
                        if received > item.size {
                            return Err(CoreError::Integrity);
                        }
                        file.write_all(&buffer[..count])?;
                        progress(count as u64);
                        event(Event::Progress { received });
                    }
                    file.sync_all()?;
                    if received != item.size {
                        return Err(CoreError::Network);
                    }
                }
                if !verify(&partial, &item.hash, item.size)? {
                    File::create(&partial)?.sync_all()?;
                    return Err(CoreError::Integrity);
                }
                if cancelled() {
                    return Err(CoreError::Cancelled);
                }
                let parent = item.path.parent().ok_or(CoreError::UnsafePath)?;
                paths.mkdir(parent)?;
                let mut output = tempfile::NamedTempFile::new_in(parent)?;
                std::io::copy(&mut File::open(&partial)?, &mut output)?;
                output.as_file().sync_all()?;
                paths.checked(&item.path)?;
                output
                    .persist(&item.path)
                    .map_err(|e| CoreError::Io(e.error))?;
                paths.remove(&partial)?;
                Ok(Outcome {
                    cached: false,
                    repaired,
                })
            })();
            match result {
                Err(CoreError::Network | CoreError::Integrity) if attempt < 2 => {
                    event(Event::Retry);
                    backoff(Duration::from_millis(250 * (1 << attempt)), cancelled)?;
                }
                other => return other,
            }
        }
        Err(CoreError::Network)
    }
}
