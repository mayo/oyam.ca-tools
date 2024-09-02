pub mod fsdriver;
pub mod s3driver;

use crate::SyncManifest;

pub trait Driver {
    async fn build_manifest(&self) -> SyncManifest;
}