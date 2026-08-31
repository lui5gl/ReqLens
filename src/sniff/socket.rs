use std::ffi::CString;
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

const ETH_P_ALL: u16 = 0x0003;

pub struct PacketSocket {
    fd: OwnedFd,
}

impl PacketSocket {
    pub fn open(interface: &str) -> io::Result<Self> {
        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW | libc::SOCK_CLOEXEC,
                i32::from(ETH_P_ALL.to_be()),
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };

        let interface = CString::new(interface)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "interface contains NUL"))?;
        let interface_index = if interface.to_bytes() == b"any" {
            0
        } else {
            let index = unsafe { libc::if_nametoindex(interface.as_ptr()) };
            if index == 0 {
                return Err(io::Error::last_os_error());
            }
            index as i32
        };

        let address = libc::sockaddr_ll {
            sll_family: libc::AF_PACKET as u16,
            sll_protocol: ETH_P_ALL.to_be(),
            sll_ifindex: interface_index,
            sll_hatype: 0,
            sll_pkttype: 0,
            sll_halen: 0,
            sll_addr: [0; 8],
        };
        let result = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                (&address as *const libc::sockaddr_ll).cast(),
                mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { fd })
    }

    pub fn receive(&self, buffer: &mut [u8], timeout: Duration) -> io::Result<Option<usize>> {
        let mut descriptor = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe {
            libc::poll(
                &mut descriptor,
                1,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if ready < 0 {
            return Err(io::Error::last_os_error());
        }
        if ready == 0 {
            return Ok(None);
        }
        let size = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
            )
        };
        if size < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Some(size as usize))
    }
}
