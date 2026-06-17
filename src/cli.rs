use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "parakses", version, about = "HFS+ reader for Windows")]
pub struct Cli {
    /// Read from a raw disk image file instead of physical drives.
    /// The file is treated as a complete disk with MBR/GPT partition table.
    #[arg(short = 'f', long = "image", global = true)]
    pub image: Option<String>,

    /// Partition index within the image (default: 0, first HFS+ partition).
    #[arg(short = 'p', long = "partition", global = true, default_value_t = 0)]
    pub partition: u32,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// List available HFS+ volumes
    Volumes,

    /// List directory contents on an HFS+ volume
    #[command(visible_aliases = ["ls"])]
    List {
        /// Volume index (from `volumes` command)
        volume: u32,
        /// Path within the volume (default: /)
        #[arg(default_value = "/")]
        path: String,
    },

    /// Print file contents to stdout
    Cat {
        /// Volume index
        volume: u32,
        /// Path to the file
        path: String,
    },

    /// Extract a file from HFS+ volume to the Windows filesystem
    #[command(visible_aliases = ["cp", "export"])]
    Extract {
        /// Volume index
        volume: u32,
        /// Source path on HFS+ volume
        src: String,
        /// Destination path on Windows filesystem
        dst: String,
    },
}
