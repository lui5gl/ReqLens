use std::ffi::CString;
use std::io;
use std::mem::MaybeUninit;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

const ETH_P_ALL: u16 = 0x0003;

const BPF_LD: u16 = 0x00;
const BPF_LDX: u16 = 0x01;
const BPF_JMP: u16 = 0x05;
const BPF_RET: u16 = 0x06;

const BPF_H: u16 = 0x08;
const BPF_B: u16 = 0x10;

const BPF_ABS: u16 = 0x20;
const BPF_IND: u16 = 0x40;
const BPF_MSH: u16 = 0xa0;

const BPF_JEQ: u16 = 0x10;
const BPF_JSET: u16 = 0x40;

const BPF_K: u16 = 0x00;

pub struct PacketSocket {
    fd: OwnedFd,
}

impl PacketSocket {
    pub fn open(
        interface: &str,
        observed_port: u16,
        receive_timeout: Duration,
    ) -> io::Result<Self> {
        let interface_index = if interface.is_empty() || interface == "any" {
            0
        } else {
            let interface_name = CString::new(interface).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "interface name cannot contain a null byte",
                )
            })?;
            let index = unsafe { libc::if_nametoindex(interface_name.as_ptr()) };
            if index == 0 {
                return Err(io::Error::last_os_error());
            }
            index
        };

        let raw_fd = unsafe {
            libc::socket(
                libc::AF_PACKET,
                libc::SOCK_RAW,
                i32::from(ETH_P_ALL.to_be()),
            )
        };
        if raw_fd < 0 {
            return Err(io::Error::last_os_error());
        }
        let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };

        let mut address = MaybeUninit::<libc::sockaddr_ll>::zeroed();
        let address = unsafe { address.assume_init_mut() };
        address.sll_family = libc::AF_PACKET as libc::c_ushort;
        address.sll_protocol = ETH_P_ALL.to_be();
        address.sll_ifindex = interface_index as libc::c_int;

        let bind_result = unsafe {
            libc::bind(
                fd.as_raw_fd(),
                address as *const libc::sockaddr_ll as *const libc::sockaddr,
                std::mem::size_of::<libc::sockaddr_ll>() as libc::socklen_t,
            )
        };
        if bind_result < 0 {
            return Err(io::Error::last_os_error());
        }

        attach_bpf_filter(fd.as_raw_fd(), observed_port)?;

        let timeout = libc::timeval {
            tv_sec: receive_timeout.as_secs() as _,
            tv_usec: receive_timeout.subsec_micros() as _,
        };
        let timeout_result = unsafe {
            libc::setsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &timeout as *const libc::timeval as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            )
        };
        if timeout_result < 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { fd })
    }

    pub fn receive(&self, buffer: &mut [u8]) -> io::Result<Option<usize>> {
        let size = unsafe {
            libc::recv(
                self.fd.as_raw_fd(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
                0,
            )
        };
        if size > 0 {
            return Ok(Some(size as usize));
        }
        if size == 0 {
            return Ok(None);
        }

        let error = io::Error::last_os_error();
        match error.kind() {
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => Ok(None),
            _ => Err(error),
        }
    }
}

fn attach_bpf_filter(raw_fd: libc::c_int, observed_port: u16) -> io::Result<()> {
    let port = u32::from(observed_port);
    let mut instructions = [
        // 0: Load EtherType (offset 12)
        libc::sock_filter {
            code: BPF_LD | BPF_H | BPF_ABS,
            jt: 0,
            jf: 0,
            k: 12,
        },
        // 1: If IPv4 (0x0800), go to 2; else jump to 11 (VLAN check)
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 9,
            k: 0x0800,
        },
        // 2: Load IP Protocol (offset 23 = 14 + 9)
        libc::sock_filter {
            code: BPF_LD | BPF_B | BPF_ABS,
            jt: 0,
            jf: 0,
            k: 23,
        },
        // 3: If TCP (6), go to 4; else reject (jump to 22)
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 18,
            k: 6,
        },
        // 4: Load IP fragment offset (offset 20 = 14 + 6)
        libc::sock_filter {
            code: BPF_LD | BPF_H | BPF_ABS,
            jt: 0,
            jf: 0,
            k: 20,
        },
        // 5: If fragmented (not first fragment), reject (jump to 22)
        libc::sock_filter {
            code: BPF_JMP | BPF_JSET | BPF_K,
            jt: 16,
            jf: 0,
            k: 0x1fff,
        },
        // 6: Load IP header length (4 * (P[14] & 0xf)) into X
        libc::sock_filter {
            code: BPF_LDX | BPF_B | BPF_MSH,
            jt: 0,
            jf: 0,
            k: 14,
        },
        // 7: Load TCP src port (offset 14 + X + 0)
        libc::sock_filter {
            code: BPF_LD | BPF_H | BPF_IND,
            jt: 0,
            jf: 0,
            k: 14,
        },
        // 8: If src port == observed_port, accept (jump to 21); else next
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 12,
            jf: 0,
            k: port,
        },
        // 9: Load TCP dst port (offset 14 + X + 2)
        libc::sock_filter {
            code: BPF_LD | BPF_H | BPF_IND,
            jt: 0,
            jf: 0,
            k: 16,
        },
        // 10: If dst port == observed_port, accept (jump to 21); else reject (jump to 22)
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 10,
            jf: 11,
            k: port,
        },
        // 11: If VLAN (0x8100), go to 12; else reject (jump to 22)
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 10,
            k: 0x8100,
        },
        // 12: Load inner EtherType (offset 16)
        libc::sock_filter {
            code: BPF_LD | BPF_H | BPF_ABS,
            jt: 0,
            jf: 0,
            k: 16,
        },
        // 13: If IPv4 (0x0800), go to 14; else reject (jump to 22)
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 8,
            k: 0x0800,
        },
        // 14: Load IP Protocol (offset 27 = 18 + 9)
        libc::sock_filter {
            code: BPF_LD | BPF_B | BPF_ABS,
            jt: 0,
            jf: 0,
            k: 27,
        },
        // 15: If TCP (6), go to 16; else reject (jump to 22)
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 6,
            k: 6,
        },
        // 16: Load IP header length (4 * (P[18] & 0xf)) into X
        libc::sock_filter {
            code: BPF_LDX | BPF_B | BPF_MSH,
            jt: 0,
            jf: 0,
            k: 18,
        },
        // 17: Load TCP src port (offset 18 + X + 0)
        libc::sock_filter {
            code: BPF_LD | BPF_H | BPF_IND,
            jt: 0,
            jf: 0,
            k: 18,
        },
        // 18: If src port == observed_port, accept (jump to 21); else next
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 2,
            jf: 0,
            k: port,
        },
        // 19: Load TCP dst port (offset 18 + X + 2)
        libc::sock_filter {
            code: BPF_LD | BPF_H | BPF_IND,
            jt: 0,
            jf: 0,
            k: 20,
        },
        // 20: If dst port == observed_port, accept (jump to 21); else reject (jump to 22)
        libc::sock_filter {
            code: BPF_JMP | BPF_JEQ | BPF_K,
            jt: 0,
            jf: 1,
            k: port,
        },
        // 21: Accept full packet
        libc::sock_filter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: 65535,
        },
        // 22: Reject / drop in kernel
        libc::sock_filter {
            code: BPF_RET | BPF_K,
            jt: 0,
            jf: 0,
            k: 0,
        },
    ];

    let mut program = libc::sock_fprog {
        len: instructions.len() as u16,
        filter: instructions.as_mut_ptr(),
    };

    let result = unsafe {
        libc::setsockopt(
            raw_fd,
            libc::SOL_SOCKET,
            libc::SO_ATTACH_FILTER,
            &mut program as *mut libc::sock_fprog as *const libc::c_void,
            std::mem::size_of::<libc::sock_fprog>() as libc::socklen_t,
        )
    };

    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
