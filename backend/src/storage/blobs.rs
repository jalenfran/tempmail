use crate::error::AppResult;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::AsyncWriteExt;

pub struct BlobStore {
    pub base: PathBuf,
}

impl BlobStore {
    pub fn new(base: impl Into<PathBuf>) -> Self {
        Self { base: base.into() }
    }

    pub async fn write_eml(&self, id: &str, bytes: &[u8]) -> AppResult<PathBuf> {
        let path = self.base.join("eml").join(format!("{id}.eml"));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let mut f = fs::File::create(&path).await?;
        f.write_all(bytes).await?;
        Ok(path)
    }

    pub async fn write_attachment(
        &self,
        message_id: &str,
        attachment_id: &str,
        bytes: &[u8],
    ) -> AppResult<PathBuf> {
        let dir = self.base.join("attachments").join(message_id);
        fs::create_dir_all(&dir).await?;
        let path = dir.join(attachment_id);
        let mut f = fs::File::create(&path).await?;
        f.write_all(bytes).await?;
        Ok(path)
    }

    pub async fn delete_message_blobs(&self, message_id: &str, eml_path: &Path) -> AppResult<()> {
        let _ = fs::remove_file(eml_path).await;
        let dir = self.base.join("attachments").join(message_id);
        let _ = fs::remove_dir_all(&dir).await;
        Ok(())
    }
}
