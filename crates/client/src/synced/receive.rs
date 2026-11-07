//! Receiving a file from the master: written beside its place, checked against the hash the master gave, and then renamed.

use super::files::{give_to_owner_of, hash_in_background, local_stat, make_parent};
use super::Attached;
use super::Master;
use super::Settled;
use super::PART_PREFIX;
use crate::sync::Report;
use crate::sync::SyncError;
use anyhow::Result;
use fafnirx_common::pb;
use std::path::Path;
use tokio::io::AsyncWriteExt;

impl Attached {
    /// Receives the current content of a file into place. It is written beside its place, checked against the hash the
    /// master gave, and only then renamed, so a half received file is never taken for the file.
    /// A local file edited while this runs (it can take minutes) is settled as a conflict, not replaced. Returns whether the
    /// master's version was put in place.
    pub(super) async fn download(&mut self, path: &str, target: &Path, change: &pb::Change, report: &mut Report) -> Result<bool, SyncError> {
        make_parent(&self.synced.path, target)?;
        let before = local_stat(target)?;
        let tmp = target.with_file_name(format!("{PART_PREFIX}{}", hex::encode(&change.root_hash[..change.root_hash.len().min(6)])));
        if let Err(e) = self.receive_checked(path, &tmp, change).await {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
        if let Some(now) = local_stat(target)? {
            if Some(now) != before {
                if let Settled::Leave = self.settle(path, target, Master::Live(change), report)? {
                    let _ = std::fs::remove_file(&tmp);
                    return Ok(false);
                }
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // The mode of the other computer, but never one the owner cannot read and write.
            if change.mode & 0o777 != 0 {
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(change.mode & 0o777 | 0o600));
            }
        }
        give_to_owner_of(&self.synced.path, &tmp);
        tokio::fs::rename(&tmp, target).await.map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            let why = if e.kind() == std::io::ErrorKind::PermissionDenied && cfg!(target_os = "macos") && crate::service::disk_access::is_protected(target) {
                format!(" (macOS: {})", crate::service::disk_access::STEPS)
            } else {
                String::new()
            };
            SyncError::File(format!("cannot replace the file: {e}{why}"))
        })?;
        Ok(true)
    }

    /// Writes the file to `tmp` and checks it is what the master listed.
    pub(super) async fn receive_checked(&mut self, path: &str, tmp: &Path, change: &pb::Change) -> Result<(), SyncError> {
        let mut stream = self
            .engine
            .client
            .download_folder(pb::DownloadFolderRequest { folder: self.synced.key().to_string(), path: path.to_string() })
            .await?
            .into_inner();
        let mut file = tokio::fs::File::create(tmp).await?;
        while let Some(part) = stream.message().await? {
            file.write_all(&part.data).await?;
        }
        file.flush().await?;
        let got = hash_in_background(tmp).await?;
        if got.root[..] != change.root_hash[..] || got.size != change.size {
            return Err(SyncError::File("what was received is not what the server has (it changed meanwhile)".into()));
        }
        Ok(())
    }
}
