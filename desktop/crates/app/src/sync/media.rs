use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use protocol::model::{MediaFile, MediaKind};
use protocol::sync::RecordKind;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

use super::client::Convex;
use crate::library::MEDIA_FOLDER;
use crate::Db;

pub const MAX_BYTES: u64 = 100 * 1024 * 1024;
const CHUNK: usize = 256 * 1024;
const BYTES_PER_SECOND: u64 = 3 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Offer {
    Exists,
    TooLarge,
    Upload { url: String },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Stored {
    storage_id: String,
}

pub struct Transfers {
    pub convex: Convex,
    pub token: String,
    pub db: Db,
    pub dir: PathBuf,
    pub busy: Arc<AtomicBool>,
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub struct Queue {
    pub uploads: usize,
    pub downloads: usize,
}

impl Transfers {
    pub fn queue(&self) -> Result<Queue> {
        let db = self.db.lock()?;
        Ok(Queue { uploads: db.media_uploads()?.len(), downloads: db.media_downloads()?.len() })
    }

    pub async fn step(&self) -> Result<bool> {
        if self.busy.load(Ordering::Relaxed) {
            return Ok(false);
        }
        let (download, uploads) = {
            let db = self.db.lock()?;
            let uploads: Vec<MediaFile> = db.media_uploads()?.into_iter().filter(|m| db.base(RecordKind::Media, &m.id).ok().flatten().is_some() && !db.is_dirty(RecordKind::Media, &m.id).unwrap_or(true)).collect();
            (db.media_downloads()?.into_iter().next(), uploads)
        };
        if let Some((media, url)) = download {
            self.download(&media, &url).await?;
            return Ok(true);
        }
        let Some(media) = uploads.into_iter().next() else { return Ok(false) };
        self.upload(&media).await?;
        Ok(true)
    }

    async fn upload(&self, media: &MediaFile) -> Result<()> {
        let path = PathBuf::from(media.path.clone().ok_or_else(|| anyhow!("{} has no file here", media.title))?);
        let size = tokio::fs::metadata(&path).await.with_context(|| format!("read {}", path.display()))?.len();
        if size > MAX_BYTES {
            return self.done(&media.id);
        }
        let Some(hash) = media.hash.clone() else {
            let hash = tokio::task::spawn_blocking(move || hash_file(&path)).await??;
            return self.db.lock()?.set_media_hash(&media.id, &hash);
        };
        let reply: Offer = self.convex.mutation("sync:mediaUpload", json!({ "token": self.token, "mediaId": media.id, "hash": hash, "size": size })).await?;
        let url = match reply {
            Offer::Exists | Offer::TooLarge => return self.done(&media.id),
            Offer::Upload { url } => url,
        };
        let body = reqwest::Body::wrap_stream(throttled(path, self.busy.clone()));
        let stored: Stored = self.convex.http().post(url).header("Content-Type", content_type(media)).body(body).send().await?.error_for_status()?.json().await?;
        self.convex.mutation::<bool>("sync:attachMedia", json!({ "token": self.token, "mediaId": media.id, "hash": hash, "storageId": stored.storage_id, "size": size })).await?;
        log::info!("sync: uploaded {} ({} KB)", media.title, size / 1024);
        self.done(&media.id)
    }

    fn done(&self, media_id: &str) -> Result<()> {
        self.db.lock()?.set_media_remote(media_id, None, true)
    }

    async fn download(&self, media: &MediaFile, url: &str) -> Result<()> {
        let folder = self.dir.join(MEDIA_FOLDER);
        tokio::fs::create_dir_all(&folder).await?;
        let response = self.convex.http().get(url).send().await?.error_for_status()?;
        if response.content_length().is_some_and(|n| n > MAX_BYTES) {
            return Err(anyhow!("{} is larger than 100 MB", media.title));
        }
        let target = free_name(&folder, &media.title, extension(media, response.headers().get("content-type").and_then(|v| v.to_str().ok())));
        let partial = target.with_extension("partial");
        let mut file = tokio::fs::File::create(&partial).await?;
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            while self.busy.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            file.write_all(&chunk?).await?;
        }
        file.flush().await?;
        drop(file);
        tokio::fs::rename(&partial, &target).await?;
        self.db.lock()?.set_media_path(&media.id, &target.to_string_lossy())?;
        log::info!("sync: downloaded {}", media.title);
        Ok(())
    }
}

fn throttled(path: PathBuf, busy: Arc<AtomicBool>) -> impl futures_util::Stream<Item = std::io::Result<Vec<u8>>> {
    futures_util::stream::unfold(None::<tokio::fs::File>, move |file| {
        let (path, busy) = (path.clone(), busy.clone());
        async move {
            let mut file = match file {
                Some(file) => file,
                None => match tokio::fs::File::open(&path).await {
                    Ok(file) => file,
                    Err(e) => return Some((Err(e), None)),
                },
            };
            while busy.load(Ordering::Relaxed) {
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            let mut buffer = vec![0u8; CHUNK];
            match tokio::io::AsyncReadExt::read(&mut file, &mut buffer).await {
                Ok(0) => None,
                Ok(n) => {
                    buffer.truncate(n);
                    tokio::time::sleep(Duration::from_millis(n as u64 * 1000 / BYTES_PER_SECOND)).await;
                    Some((Ok(buffer), Some(file)))
                }
                Err(e) => Some((Err(e), None)),
            }
        }
    })
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn content_type(media: &MediaFile) -> &'static str {
    let ext = media.path.as_deref().and_then(|p| Path::new(p).extension()).and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn extension(media: &MediaFile, content_type: Option<&str>) -> &'static str {
    match content_type.unwrap_or("").split(';').next().unwrap_or("") {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "video/quicktime" => "mov",
        "video/webm" => "webm",
        "video/x-matroska" => "mkv",
        "audio/mpeg" => "mp3",
        "audio/wav" => "wav",
        "audio/mp4" => "m4a",
        "application/pdf" => "pdf",
        _ => match media.kind {
            MediaKind::Image | MediaKind::Presentation => "png",
            MediaKind::Video => "mp4",
            MediaKind::Audio => "mp3",
        },
    }
}

fn free_name(folder: &Path, title: &str, extension: &str) -> PathBuf {
    let stem: String = title.chars().map(|c| if c.is_alphanumeric() || " ._-()".contains(c) { c } else { '_' }).collect();
    let stem = stem.trim().trim_start_matches('.');
    let stem = stem.strip_suffix(&format!(".{extension}")).unwrap_or(stem);
    let stem = if stem.is_empty() { "media" } else { stem };
    (1..).map(|n| folder.join(if n == 1 { format!("{stem}.{extension}") } else { format!("{stem} ({n}).{extension}") })).find(|p| !p.exists()).expect("a free name")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_get_a_free_safe_name_with_one_extension() {
        let dir = std::env::temp_dir().join(format!("palto-sync-{}", protocol::model::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(free_name(&dir, "Count/down.mp4", "mp4"), dir.join("Count_down.mp4"));
        std::fs::write(dir.join("Count_down.mp4"), b"x").unwrap();
        assert_eq!(free_name(&dir, "Count/down.mp4", "mp4"), dir.join("Count_down (2).mp4"));
        std::fs::write(dir.join("a.bin"), b"abc").unwrap();
        assert_eq!(hash_file(&dir.join("a.bin")).unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
