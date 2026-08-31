use std::ffi::CString;
use std::io;
use std::mem;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

const ETH_P_IP: u16 = 0x0800;

pub struct PacketSocket {
    fd: OwnedFd,
}

impl PacketSocket {
    pub fn open(interface: &str, port: u16) -> io::Result<Self> {
        let fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_DGRAM | libc::SOCK_CLOEXEC,
                i32::from(ETH_P_IP.to_be()),
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
            sll_protocol: ETH_P_IP.to_be(),
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

        attach_port_filter(fd.as_raw_fd(), port)?;
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

fn stmt(code: u16, jt: u8, jf: u8, k: u32) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

fn attach_port_filter(fd: libc::c_int, port: u16) -> io::Result<()> {
    // AF_PACKET/SOCK_DGRAM strips the link header, so offsets start at IPv4.
    // Accept TCP packets whose source or destination port matches `port`.
    let mut instructions = port_filter(port);
    let program = libc::sock_fprog {
        len: instructions.len() as u16,
        filter: instructions.as_mut_ptr(),
    };
    let result = unsafe {
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_ATTACH_FILTER,
            (&program as *const libc::sock_fprog).cast(),
            mem::size_of::<libc::sock_fprog>() as libc::socklen_t,
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn port_filter(port: u16) -> [libc::sock_filter; 9] {
    [
        stmt((libc::BPF_LD | libc::BPF_B | libc::BPF_ABS) as u16, 0, 0, 9),
        stmt(
            (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            0,
            // Jump directly to RET 0 when the IPv4 protocol is not TCP.
            6,
            libc::IPPROTO_TCP as u32,
        ),
        stmt(
            (libc::BPF_LDX | libc::BPF_B | libc::BPF_MSH) as u16,
            0,
            0,
            0,
        ),
        stmt((libc::BPF_LD | libc::BPF_H | libc::BPF_IND) as u16, 0, 0, 0),
        stmt(
            (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            2,
            0,
            u32::from(port),
        ),
        stmt((libc::BPF_LD | libc::BPF_H | libc::BPF_IND) as u16, 0, 0, 2),
        stmt(
            (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16,
            0,
            1,
            u32::from(port),
        ),
        stmt((libc::BPF_RET | libc::BPF_K) as u16, 0, 0, u32::MAX),
        stmt((libc::BPF_RET | libc::BPF_K) as u16, 0, 0, 0),
    ]
}

#[cfg(test)]
mod tests {
    use super::port_filter;

    #[test]
    fn non_tcp_branch_jumps_to_kernel_reject() {
        let filter = port_filter(80);
        let protocol_check_index = 1_usize;
        let reject_index = protocol_check_index + 1 + usize::from(filter[1].jf);

        assert_eq!(reject_index, 8);
        assert_eq!(
            filter[reject_index].code,
            (libc::BPF_RET | libc::BPF_K) as u16
        );
        assert_eq!(filter[reject_index].k, 0);
    }
}
