use anyhow::Result;
use clap::Parser;

mod cli;
mod ctrl_device;
mod data_device;
mod queue;

const MAX_QUEUES: u32 = 4;
const MAX_QUEUE_DEPTH: u32 = 128;
const MAX_IO_BYTES: u32 = 65536;
const CTRL_RING_DEPTH: u32 = 32;
const CTRL_DEV_PATH: &str = "/dev/ublk-control";
const DATA_DEV_PATH: &str = "/dev/ublkc";
const IO_IDLE_SECS: u64 = 20;

fn main() -> Result<()> {
    let args = cli::UblkArgs::parse();
    println!("{:?}", args);
    match args.command {
        cli::Command::Add(args) => add_device(args),
        cli::Command::Remove {} => todo!(),
        cli::Command::List {} => todo!(),
    }
}

fn add_device(args: cli::AddDeviceArgs) -> Result<()> {
    println!("{:?}", args);
    let mut ctrl_dev = ctrl_device::UblkCtrlDevice::new(&args)?;

    // TODO: Target init
    let mut data_dev = data_device::UblkDataDevice::new(&args, &ctrl_dev)?;

    ctrl_dev.set_params()?;
    ctrl_dev.start()?;
    data_dev.block()?;

    // TODO: Target deinit

    ctrl_dev.delete_device()?;

    Ok(())
}
