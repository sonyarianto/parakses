pub mod partition;
pub mod windows;

pub trait VolumeDiscovery {
    type VolumeInfo;
    fn enumerate() -> anyhow::Result<Vec<Self::VolumeInfo>>;
}
