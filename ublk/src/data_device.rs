use super::cli::AddDeviceArgs;
use super::ctrl_device;
use super::queue::UblkQueue;
use super::DATA_DEV_PATH;
use anyhow::anyhow;
use anyhow::Result;
use std::fs::File;
use std::sync::Arc;
use std::thread::JoinHandle;

pub(crate) struct UblkDataDevice {
    pub(crate) _fd: Arc<File>,
    pub(crate) handles: Option<Vec<JoinHandle<Result<()>>>>,
}

impl UblkDataDevice {
    pub(crate) fn new(
        args: &AddDeviceArgs,
        ctrl_dev: &ctrl_device::UblkCtrlDevice,
    ) -> Result<Self> {
        let fd = Arc::new(File::open(format!(
            "{}{}",
            DATA_DEV_PATH,
            args.number.unwrap_or(0)
        ))?);

        let depth = ctrl_dev.info.queue_depth;
        let io_buffer_size = ctrl_dev.info.max_io_buf_bytes;

        let handles = (0..ctrl_dev.info.nr_hw_queues)
            .map(|i| {
                let thread_fd = fd.clone();
                std::thread::spawn(move || {
                    UblkQueue::<crate::queue::NullTarget>::new(
                        thread_fd,
                        depth,
                        io_buffer_size.try_into()?,
                        i,
                    )?
                    .handle_queue()
                })
            })
            .collect();

        Ok(Self {
            _fd: fd,
            handles: Some(handles),
        })
    }

    pub(crate) fn block(&mut self) -> Result<()> {
        if let Some(handles) = self.handles.take() {
            for handle in handles {
                handle
                    .join()
                    .map_err(|e| anyhow!("Thread error: {:?}", e))??;
            }
        }
        Ok(())
    }
}
