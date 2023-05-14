use crate::cli::AddDeviceArgs;
use anyhow::anyhow;
use anyhow::Result;
use io_uring::cqueue::Entry;
use io_uring::opcode;
use io_uring::squeue::Entry128;
use io_uring::types;
use io_uring::IoUring;
use std::fs::File;
use std::mem::size_of;
use std::os::fd::AsRawFd;
use std::rc::Rc;

use super::CTRL_DEV_PATH;
use super::CTRL_RING_DEPTH;
use super::MAX_IO_BYTES;

pub(crate) struct UblkCtrlDevice {
    pub(crate) fd: File,
    pub(crate) info: Rc<ublk_sys::ublksrv_ctrl_dev_info>,
    pub(crate) ring: IoUring<Entry128, Entry>,
}

impl UblkCtrlDevice {
    pub(crate) fn new(args: &AddDeviceArgs) -> Result<Self> {
        let fd = File::open(CTRL_DEV_PATH)?;

        let info = Rc::new(ublk_sys::ublksrv_ctrl_dev_info {
            nr_hw_queues: args.queues,
            queue_depth: args.depth,
            state: 0,
            pad0: 0,
            max_io_buf_bytes: MAX_IO_BYTES,
            dev_id: args.number.unwrap_or(0),
            ublksrv_pid: 0,
            pad1: 0,
            flags: 0,
            ublksrv_flags: 0,
            reserved1: 0,
            reserved2: 0,
            owner_uid: 0,
            owner_gid: 0,
        });

        let ring = IoUring::<Entry128, Entry>::builder()
            .dontfork()
            .build(CTRL_RING_DEPTH)?;

        let mut this = Self {
            fd,
            info: info.clone(),
            ring,
        };

        this.send_command_with_buffer::<ublk_sys::ublksrv_ctrl_dev_info>(
            ublk_sys::UBLK_CMD_ADD_DEV,
            info,
        )?;

        Ok(this)
    }

    pub(crate) fn send_command(&mut self, command_id: u32) -> Result<()> {
        let cmd = ublk_sys::ublksrv_ctrl_cmd {
            dev_id: self.info.dev_id,
            queue_id: u16::MAX,
            len: 0,
            addr: 0,
            data: [0; 1],
            dev_path_len: 0,
            pad: 0,
            reserved: 0,
        };

        self.encode_and_send(command_id, &cmd)
    }

    pub(crate) fn send_command_with_buffer<T>(
        &mut self,
        command_id: u32,
        command_data_buffer: impl Into<Rc<T>>,
    ) -> Result<()> {
        let cdr: Rc<T> = command_data_buffer.into();
        let cmd = ublk_sys::ublksrv_ctrl_cmd {
            dev_id: self.info.dev_id,
            queue_id: u16::MAX,
            len: size_of::<T>().try_into()?,
            addr: cdr.as_ref() as *const _ as u64,
            data: [0; 1],
            dev_path_len: 0,
            pad: 0,
            reserved: 0,
        };

        self.encode_and_send(command_id, &cmd)
    }

    pub(crate) fn send_command_with_data(&mut self, command_id: u32, data: u64) -> Result<()> {
        let cmd = ublk_sys::ublksrv_ctrl_cmd {
            dev_id: self.info.dev_id,
            queue_id: u16::MAX,
            len: 0,
            addr: 0,
            data: [data],
            dev_path_len: 0,
            pad: 0,
            reserved: 0,
        };

        self.encode_and_send(command_id, &cmd)
    }

    pub(crate) fn encode_and_send(
        &mut self,
        command_id: u32,
        command: &ublk_sys::ublksrv_ctrl_cmd,
    ) -> Result<()> {
        // TODO: Deduplicate this code from queue
        let slice = unsafe {
            core::slice::from_raw_parts(
                command as *const _ as *const u8,
                size_of::<ublk_sys::ublksrv_ctrl_cmd>(),
            )
        };

        let mut bytes = [0u8; 80];

        bytes[0..slice.len()].copy_from_slice(slice);

        let command = opcode::UringCmd80::new(types::Fd(self.fd.as_raw_fd()), command_id)
            .cmd(bytes)
            .build();

        unsafe {
            self.ring.submission().push(&command)?;
        }

        let count = self.ring.submit_and_wait(1)?;
        if count != 1 {
            return Err(anyhow!("Too many completions"));
        }

        let cqe = self.ring.completion().next().expect("Missing cqe");

        // TODO: Populate user_data to make sure we have the right cqe

        let result = cqe.result();

        if result != 0 {
            return Err(anyhow!("CQE Error: {}", result));
        }

        Ok(())
    }

    pub(crate) fn set_params(&mut self) -> Result<()> {
        // TODO: Contents from target
        let params = ublk_sys::ublk_params {
            len: size_of::<ublk_sys::ublk_params>().try_into()?,
            types: ublk_sys::UBLK_PARAM_TYPE_BASIC,
            basic: ublk_sys::ublk_param_basic {
                attrs: 0,
                logical_bs_shift: 9,
                physical_bs_shift: 12,
                io_opt_shift: 12,
                io_min_shift: 9,
                max_sectors: self.info.max_io_buf_bytes >> 9,
                chunk_sectors: 0,
                dev_sectors: 512, // TODO: From dev setup
                virt_boundary_mask: 0,
            },
            discard: ublk_sys::ublk_param_discard {
                discard_alignment: 0,
                discard_granularity: 0,
                max_discard_sectors: 0,
                max_write_zeroes_sectors: 0,
                max_discard_segments: 0,
                reserved0: 0,
            },
            devt: ublk_sys::ublk_param_devt {
                char_major: 0,
                char_minor: 0,
                disk_major: 0,
                disk_minor: 0,
            },
        };

        self.send_command_with_buffer::<ublk_sys::ublk_params>(
            ublk_sys::UBLK_CMD_SET_PARAMS,
            params,
        )?;
        Ok(())
    }

    pub(crate) fn start(&mut self) -> Result<()> {
        self.send_command_with_data(ublk_sys::UBLK_CMD_START_DEV, std::process::id().into())
    }

    pub(crate) fn delete_device(&mut self) -> Result<()> {
        self.send_command(ublk_sys::UBLK_CMD_DEL_DEV)
    }
}
