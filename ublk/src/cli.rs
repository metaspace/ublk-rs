use clap::value_parser;
use clap::Args;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;

#[derive(Parser, Debug)]
#[command(author, version, about)]
pub(crate) struct UblkArgs {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Command {
    /// Add a ublk device
    Add(AddDeviceArgs),

    /// Remove a ublk device
    Remove {},

    /// List all ublk devices
    List {},
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum TargetKind {
    /// A null device type
    Null,

    /// A loop device type
    Loop,
}

#[derive(Args, Debug)]
pub(crate) struct AddDeviceArgs {
    /// Type of device
    #[arg(short, long)]
    pub(crate) kind: TargetKind,

    /// Device ID
    #[arg(short, long)]
    pub(crate) number: Option<u32>,

    /// Number of queues
    #[arg(short = 'u', long, default_value_t = 2, value_parser = value_parser!(u16).range(1..=(crate::MAX_QUEUES as i64)))]
    pub(crate) queues: u16,

    /// Depth of each queue
    #[arg(short, long, default_value_t = 128, value_parser = value_parser!(u16).range(1..=(crate::MAX_QUEUE_DEPTH as i64)))]
    pub(crate) depth: u16,

    /// Debug bitmask
    #[arg(short = 'm', long, default_value_t = 0)]
    pub(crate) debug_mask: u64,

    /// Set to produce less output
    #[arg(short, long, default_value_t = false)]
    pub(crate) quiet: bool,
}
