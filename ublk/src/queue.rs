use anyhow::anyhow;
use anyhow::Result;
use core::ptr::NonNull;
use io_uring::cqueue::Entry;
use io_uring::opcode;
use io_uring::squeue::Entry128;
use io_uring::types;
use io_uring::IoUring;
use std::fs::File;
use std::mem::size_of;
use std::os::fd::AsRawFd;
use std::sync::Arc;

pub(crate) struct UblkQueue<T: Target> {
    pub(crate) ublk_data_fd: Arc<File>,
    pub(crate) queue_index: u16,
    pub(crate) queue_depth: u16,
    pub(crate) io_command_buffer: memmap2::Mmap,
    pub(crate) io_data_buffer: NonNull<u8>,
    pub(crate) ring: IoUring<Entry128, Entry>,
    _backend: core::marker::PhantomData<T>,
}

impl<T: Target> UblkQueue<T> {
    pub(crate) fn new(
        fd: Arc<File>,
        queue_depth: u16,
        buffer_size: usize,
        queue_index: u16,
    ) -> Result<Self> {
        println!("Starting queue {}", queue_index);
        // Init queues
        //  - Map io descriptor memory
        let offset = ublk_sys::UBLKSRV_CMD_BUF_OFFSET
            + queue_index as u32
                * (ublk_sys::UBLK_MAX_QUEUE_DEPTH * size_of::<ublk_sys::ublksrv_io_desc>() as u32);

        println!(
            "Mapping command descriptors at offset: {}: {}",
            offset, queue_index
        );
        let length: usize = queue_depth as usize * size_of::<ublk_sys::ublksrv_io_desc>();

        let io_command_buffer = unsafe {
            memmap2::MmapOptions::new()
                .populate()
                .offset(offset.into())
                .len(length)
                .map(fd.as_raw_fd())?
        };
        //  - Allocate IO buffer memory
        let layout = core::alloc::Layout::from_size_align(buffer_size, 4096)?;
        let io_data_buffer = NonNull::new(unsafe { std::alloc::alloc(layout) })
            .ok_or(anyhow!("Allocation error"))?;
        //  - Setup ring
        let ring = IoUring::<Entry128, Entry>::builder()
            .setup_coop_taskrun()
            .build(queue_depth.into())?;

        // TODO: - Regiser ring fd

        // TODO: Add loop file to registered fds
        ring.submitter().register_files(&[fd.as_raw_fd()])?;

        Ok(Self {
            ublk_data_fd: fd,
            queue_index,
            queue_depth,
            io_command_buffer,
            io_data_buffer,
            ring,
            _backend: core::marker::PhantomData,
        })
    }

    pub(crate) fn handle_queue(&mut self) -> Result<()> {
        // Submit fetch commands
        for i in 0u16..self.queue_depth {
            let io_cmd = ublk_sys::ublksrv_io_cmd {
                q_id: self.queue_index,
                tag: i,
                result: 0,
                addr: self.io_data_buffer.as_ptr() as _,
            };

            self.encode_and_send(ublk_sys::UBLK_IO_FETCH_REQ, &io_cmd, 0, false)?;
        }

        loop {
            self.handle_io()?;
        }

        // TODO: Deinit queues
    }

    pub(crate) fn encode_and_send(
        &mut self,
        command_op: u32,
        command: &ublk_sys::ublksrv_io_cmd,
        target_data: u16,
        is_target_io: bool,
    ) -> Result<()> {
        assert!(command_op >> 8 == 0);

        let slice = unsafe {
            core::slice::from_raw_parts(
                command as *const _ as *const u8,
                size_of::<ublk_sys::ublksrv_io_cmd>(),
            )
        };

        let mut bytes = [0u8; 80];

        bytes[0..slice.len()].copy_from_slice(slice);

        let user_data: u64 =
            command.tag as u64 | (command_op as u64) << 16 | (target_data as u64) << 24;

        // We use the same ring for target IO. Set bit 63 of user_data to be
        // able to identify target IO
        let user_data = if is_target_io {
            user_data | 1u64 << 63
        } else {
            user_data
        };

        let command = opcode::UringCmd80::new(types::Fd(self.ublk_data_fd.as_raw_fd()), command_op)
            .cmd(bytes)
            .build()
            .user_data(user_data);

        unsafe {
            self.ring.submission().push(&command)?;
        }

        self.ring.submit()?;

        Ok(())
    }

    pub(crate) fn handle_io(&mut self) -> Result<()> {
        // TODO: Check if queue is stopping

        // let timespec = io_uring::types::Timespec::new().sec(IO_IDLE_SECS);
        // let args = io_uring::types::SubmitArgs::new().timespec(&timespec);
        // let _count = self.ring.submitter().submit_with_args(1, &args)?;
        self.ring.submitter().submit_and_wait(1)?;

        let completions: Vec<_> = self.ring.completion().collect();
        for cqe in completions {
            let tag: u16 = (cqe.user_data() & 0xffff).try_into()?;
            let command_op: u8 = ((cqe.user_data() >> 8) & 0xff).try_into()?;
            // TODO: Check for queue stopping and abort result

            if cqe.result() as u32 == ublk_sys::UBLK_IO_RES_OK {
                assert!(tag < self.queue_depth);

                // TODO: get the iod

                // TODO: Handle target IO
                T::handle_io()?;

                // Complete OK
                let io_cmd = ublk_sys::ublksrv_io_cmd {
                    q_id: self.queue_index,
                    tag,
                    result: (self.get_io_descriptor(tag)?.nr_sectors << 9) as i32,
                    addr: self.io_data_buffer.as_ptr() as _,
                };

                self.encode_and_send(ublk_sys::UBLK_IO_COMMIT_AND_FETCH_REQ, &io_cmd, 0, false)?;
            }
        }

        Ok(())
    }

    pub(crate) fn get_io_descriptor(&mut self, tag: u16) -> Result<&'_ ublk_sys::ublksrv_io_desc> {
        assert!(tag < self.queue_depth);
        let ptr: *mut ublk_sys::ublksrv_io_desc = self.io_command_buffer.as_ptr() as _;
        let r = unsafe { ptr.add(tag as usize).as_ref().unwrap() };
        Ok(r)
    }
}

pub(crate) trait Target {
    fn handle_io() -> Result<()>;
}

pub(crate) struct NullTarget;
impl Target for NullTarget {
    fn handle_io() -> Result<()> {
        Ok(())
    }
}
