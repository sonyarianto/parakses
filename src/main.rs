#![allow(dead_code)]

mod blockio;
mod cli;
mod error;
mod hfs;
mod util;
mod volume;

use blockio::BlockDevice;
use clap::Parser;
use cli::{Cli, Commands};
use std::io::Write;
use volume::VolumeDiscovery;

fn open_volume(cli: &Cli, volume_index: u32) -> anyhow::Result<hfs::HfsVolume> {
    if let Some(img_path) = &cli.image {
        let path = std::path::Path::new(img_path);
        let drive: Box<dyn BlockDevice> = Box::new(blockio::filedevice::FileDevice::open(path)?);
        let vols = volume::windows::WindowsVolumeEnumerator::enumerate_from(drive.as_ref())?;
        let hp = vols.first()
            .and_then(|v| v.hfs_partitions.get(cli.partition as usize))
            .ok_or_else(|| anyhow::anyhow!("No HFS+ partition {} in image", cli.partition))?;
        let sector_size = drive.sector_size();
        let volume_offset = hp.start_lba * u64::from(sector_size);
        hfs::HfsVolume::open(drive, volume_offset)
    } else {
        let vols = volume::windows::WindowsVolumeEnumerator::enumerate()?;
        let vi = vols.get(volume_index as usize)
            .ok_or_else(|| anyhow::anyhow!("Volume index {} not found", volume_index))?;
        let hp = vi.hfs_partitions.first()
            .ok_or_else(|| anyhow::anyhow!("No HFS+ partitions on this drive"))?;
        let drive: Box<dyn BlockDevice> = Box::new(blockio::physical::PhysicalDrive::open(vi.drive_index)?);
        let sector_size = drive.sector_size();
        let volume_offset = hp.start_lba * u64::from(sector_size);
        hfs::HfsVolume::open(drive, volume_offset)
    }
}

fn print_volume_info(hfs: &hfs::HfsVolume) -> anyhow::Result<()> {
    let info = hfs.volume_info();
    println!("Volume: {}", info.volume_name);
    println!("  Signature:  {:#06x}", info.signature);
    println!("  Version:    {}", info.version);
    println!("  Type:       {}", if info.is_hfsx { "HFSX" } else { "HFS+" });
    println!("  Block size: {} bytes", info.block_size);
    println!(
        "  Capacity:   {} blocks ({} MB)",
        info.total_blocks,
        (u64::from(info.total_blocks) * u64::from(info.block_size)) / (1024 * 1024)
    );
    println!(
        "  Free:       {} blocks ({} MB)",
        info.free_blocks,
        (u64::from(info.free_blocks) * u64::from(info.block_size)) / (1024 * 1024)
    );
    println!("  Files:      {}", info.file_count);
    println!("  Folders:    {}", info.folder_count);
    println!("  Write count: {}", info.write_count);
    if info.is_journaled {
        println!("  Journal:    present{}",
            if info.journal_dirty { " (dirty, may be inconsistent)" } else { " (clean)" }
        );
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match &cli.command {
        Commands::Volumes => {
            if let Some(img_path) = &cli.image {
                let path = std::path::Path::new(img_path);
                let drive = blockio::filedevice::FileDevice::open(path)?;
                let vols = volume::windows::WindowsVolumeEnumerator::enumerate_from(&drive)?;
                if vols.is_empty() {
                    println!("No HFS+ volumes found in image.");
                } else {
                    for (i, vol) in vols.iter().enumerate() {
                        println!("[{}] Image partition set {}", i, i);
                        for (j, part) in vol.hfs_partitions.iter().enumerate() {
                            let name = part.name.as_deref().unwrap_or("(unnamed)");
                            let size_mb = (part.sector_count * 512) / (1024 * 1024);
                            println!(
                                "  Partition {}: start LBA {}, {} sectors (~{} MB) — {}",
                                j, part.start_lba, part.sector_count, size_mb, name
                            );
                        }
                    }
                }
            } else {
                let vols = volume::windows::WindowsVolumeEnumerator::enumerate()?;
                if vols.is_empty() {
                    println!("No HFS+ volumes found.");
                } else {
                    for (i, vol) in vols.iter().enumerate() {
                        println!("[{}] PhysicalDrive{}", i, vol.drive_index);
                        for (j, part) in vol.hfs_partitions.iter().enumerate() {
                            let name = part.name.as_deref().unwrap_or("(unnamed)");
                            let size_mb = (part.sector_count * 512) / (1024 * 1024);
                            println!(
                                "  Partition {}: start LBA {}, {} sectors (~{} MB) — {}",
                                j, part.start_lba, part.sector_count, size_mb, name
                            );
                        }
                    }
                }
            }
        }
        Commands::List { volume, path } => {
            let hfs = open_volume(&cli, *volume)?;
            print_volume_info(&hfs)?;

            if *path == "/" {
                let entries = hfs.list_root()?;
                println!("\nDirectory listing:");
                for entry in &entries {
                    let kind = if entry.is_directory { "DIR" } else { "   " };
                    let size = entry.size;
                    println!("  [{}] {:>10}  {}", kind, size, entry.name);
                }
                if entries.is_empty() {
                    println!("  (empty)");
                }
            } else {
                match hfs.resolve_path(path) {
                    Ok(record) => match &record {
                        hfs::catalog::CatalogRecordData::Folder(f) => {
                            println!("\n'{}' is a directory ({} items)", path, f.valence);
                            let entries = hfs.list_directory(f.folder_id)?;
                            for entry in &entries {
                                let kind = if entry.is_directory { "DIR" } else { "   " };
                                println!("  [{}] {:>10}  {}", kind, entry.size, entry.name);
                            }
                        }
                        hfs::catalog::CatalogRecordData::File(f) => {
                            println!(
                                "\n'{}' is a file ({} bytes)",
                                path, f.data_fork.logical_size
                            );
                        }
                        _ => println!("\n'{}' resolved (type: thread)", path),
                    },
                    Err(e) => println!("\nError: {}", e),
                }
            }
        }
        Commands::Cat { volume, path } => {
            let hfs = open_volume(&cli, *volume)?;
            let data = hfs.read_file(path)?;
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            handle.write_all(&data)?;
            handle.flush()?;
        }
        Commands::Extract { volume, src, dst } => {
            let hfs = open_volume(&cli, *volume)?;
            let data = hfs.read_file(src)?;
            std::fs::write(dst, &data)?;
            println!(
                "Extracted {} bytes from '{}' to '{}'",
                data.len(),
                src,
                dst
            );
        }
    }

    Ok(())
}
