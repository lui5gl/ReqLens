#[cfg(target_os = "linux")]
#[allow(non_snake_case)]
#[allow(clippy::missing_safety_doc)]
pub mod linux_syscall_compat {
    use libc::{
        F_GETFL, F_SETFD, F_SETFL, FD_CLOEXEC, O_CLOEXEC, O_NONBLOCK, SOCK_CLOEXEC, SOCK_NONBLOCK,
        c_int, c_long, fcntl, sockaddr, socklen_t, syscall,
    };

    const EPOLL_CLOEXEC: c_int = 0x80000;
    const SYS_EPOLL_CREATE: c_long = 213;
    const SYS_ACCEPT: c_long = 43;
    const SYS_PIPE: c_long = 22;
    const SYS_DUP2: c_long = 33;

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn epoll_create1(flags: c_int) -> c_int {
        unsafe {
            let fd = syscall(SYS_EPOLL_CREATE, 1024 as c_int) as c_int;
            if fd >= 0 && (flags & EPOLL_CLOEXEC != 0) {
                fcntl(fd, F_SETFD, FD_CLOEXEC);
            }
            fd
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn accept4(
        sockfd: c_int,
        addr: *mut sockaddr,
        addrlen: *mut socklen_t,
        flags: c_int,
    ) -> c_int {
        unsafe {
            let fd = syscall(SYS_ACCEPT, sockfd, addr, addrlen) as c_int;
            if fd >= 0 {
                if flags & SOCK_CLOEXEC != 0 {
                    fcntl(fd, F_SETFD, FD_CLOEXEC);
                }
                if flags & SOCK_NONBLOCK != 0 {
                    let curr = fcntl(fd, F_GETFL);
                    if curr >= 0 {
                        fcntl(fd, F_SETFL, curr | O_NONBLOCK);
                    }
                }
            }
            fd
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn pipe2(pipefd: *mut c_int, flags: c_int) -> c_int {
        unsafe {
            let res = syscall(SYS_PIPE, pipefd) as c_int;
            if res >= 0 && !pipefd.is_null() {
                let p0 = *pipefd;
                let p1 = *pipefd.add(1);
                if flags & O_CLOEXEC != 0 {
                    fcntl(p0, F_SETFD, FD_CLOEXEC);
                    fcntl(p1, F_SETFD, FD_CLOEXEC);
                }
                if flags & O_NONBLOCK != 0 {
                    let curr0 = fcntl(p0, F_GETFL);
                    if curr0 >= 0 {
                        fcntl(p0, F_SETFL, curr0 | O_NONBLOCK);
                    }
                    let curr1 = fcntl(p1, F_GETFL);
                    if curr1 >= 0 {
                        fcntl(p1, F_SETFL, curr1 | O_NONBLOCK);
                    }
                }
            }
            res
        }
    }

    #[unsafe(no_mangle)]
    pub unsafe extern "C" fn dup3(oldfd: c_int, newfd: c_int, flags: c_int) -> c_int {
        if oldfd == newfd {
            return -1;
        }
        unsafe {
            let res = syscall(SYS_DUP2, oldfd, newfd) as c_int;
            if res >= 0 && (flags & O_CLOEXEC != 0) {
                fcntl(res, F_SETFD, FD_CLOEXEC);
            }
            res
        }
    }
}
