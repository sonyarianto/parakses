use crate::blockio::physical::PhysicalDrive;
use crate::blockio::BlockDevice;
use crate::volume::partition::{self, PartitionTable};
use crate::volume::VolumeDiscovery;

#[derive(Debug)]
pub struct WindowsVolume {
    pub drive_index: u32,
    pub hfs_partitions: Vec<HfsPartitionInfo>,
}

#[derive(Debug, Clone)]
pub struct HfsPartitionInfo {
    pub start_lba: u64,
    pub sector_count: u64,
    pub name: Option<String>,
}

pub struct WindowsVolumeEnumerator;

impl VolumeDiscovery for WindowsVolumeEnumerator {
    type VolumeInfo = WindowsVolume;

    fn enumerate() -> anyhow::Result<Vec<Self::VolumeInfo>> {
        let mut volumes = Vec::new();

        for drive_index in 0..32 {
            let drive = match PhysicalDrive::open(drive_index) {
                Ok(d) => d,
                Err(_) => continue,
            };

            log::info!(
                "Drive {}: sector_size={}, total_sectors={}",
                drive_index,
                drive.sector_size(),
                drive.total_sectors()
            );

            let partition_table = match partition::detect_partition_table(&drive, 0) {
                Ok(Some(pt)) => pt,
                Ok(None) => {
                    log::debug!("Drive {}: no partition table", drive_index);
                    continue;
                }
                Err(e) => {
                    log::warn!(
                        "Drive {}: error reading partition table: {}",
                        drive_index,
                        e
                    );
                    continue;
                }
            };

            let hfs_partitions = find_hfs_partitions(&partition_table);

            if !hfs_partitions.is_empty() {
                volumes.push(WindowsVolume {
                    drive_index,
                    hfs_partitions,
                });
            }
        }

        Ok(volumes)
    }
}

impl WindowsVolumeEnumerator {
    /// Enumerate HFS+ partitions from an arbitrary block device (e.g. a disk image file).
    /// Returns a single WindowsVolume entry with drive_index=0.
    pub fn enumerate_from(device: &dyn BlockDevice) -> anyhow::Result<Vec<WindowsVolume>> {
        let partition_table = match partition::detect_partition_table(device, 0) {
            Ok(Some(pt)) => pt,
            Ok(None) => {
                // No partition table found — try treating the entire device as a raw HFS+ volume.
                // Check if byte 1024 has a valid Volume Header signature.
                let sector = device.read_sector(0)?;
                if sector.len() < 1024 + 2 {
                    return Ok(Vec::new());
                }
                let sig = u16::from_be_bytes([sector[1024], sector[1025]]);
                if sig == 0x482B || sig == 0x4858 {
                    return Ok(vec![WindowsVolume {
                        drive_index: 0,
                        hfs_partitions: vec![HfsPartitionInfo {
                            start_lba: 0,
                            sector_count: device.total_sectors(),
                            name: None,
                        }],
                    }]);
                }
                return Ok(Vec::new());
            }
            Err(e) => {
                log::warn!("Error reading partition table from image: {}", e);
                return Ok(Vec::new());
            }
        };

        let hfs_partitions = find_hfs_partitions(&partition_table);
        if hfs_partitions.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![WindowsVolume {
                drive_index: 0,
                hfs_partitions,
            }])
        }
    }
}

fn find_hfs_partitions(table: &PartitionTable) -> Vec<HfsPartitionInfo> {
    match table {
        PartitionTable::Mbr(entries) => entries
            .iter()
            .filter(|e| partition::is_hfs_mbr(e.partition_type))
            .map(|e| HfsPartitionInfo {
                start_lba: e.start_lba,
                sector_count: e.sector_count,
                name: None,
            })
            .collect(),
        PartitionTable::Gpt { entries, .. } => entries
            .iter()
            .filter(|e| partition::is_hfs_gpt(&e.partition_type_guid))
            .map(|e| {
                let name = if e.name.is_empty() {
                    None
                } else {
                    Some(e.name.clone())
                };
                HfsPartitionInfo {
                    start_lba: e.start_lba,
                    sector_count: e.end_lba - e.start_lba + 1,
                    name,
                }
            })
            .collect(),
    }
}
