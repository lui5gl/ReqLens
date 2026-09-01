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
    pub fn open(interface: &str, receive_timeout: Duration) -> io::Result<Self> {
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

        set_receive_timeout(fd.as_raw_fd(), receive_timeout)?;

        Ok(Self { fd })
    }

    pub fn receive(&self, buffer: &mut [u8]) -> io::Result<Option<usize>> {
        let size = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                0,
            )
        };
        if size < 0 {
            let error = io::Error::last_os_error();
            return match error.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Ok(None),
                _ => Err(error),
            };
        }
        Ok(Some(size as usize))
    }
}

fn set_receive_timeout(fd: libc::c_int, timeout: Duration) -> io::Result<()> {
    let timeout = libc::timeval {
        tv_sec: timeout.as_secs().try_into().unwrap_or(libc::time_t::MAX),
        tv_usec: i64::from(timeout.subsec_micros()),
    };
    let result = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_RCVTIMEO,
            (&timeout as *const libc::timeval).cast(),
            mem::size_of::<libc::timeval>() as libc::socklen_t,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
