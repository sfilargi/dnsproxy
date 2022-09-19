use libc;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use std::cell::RefCell;

use crate::tokengen::TokenGen;

macro_rules! libccall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

#[repr(C)]
#[cfg_attr(target_arch = "x86_64", repr(packed))]
#[derive(Clone, Copy)]
struct Event {
    pub events: u32,
    pub data: u64,
}

struct EpollFd {
    fd: RawFd
}

fn epoll_create() -> io::Result<EpollFd> {
    let fd = libccall!(epoll_create1(libc::EPOLL_CLOEXEC))?;
    Ok(EpollFd{fd})
}

fn epoll_ctl(epoll_fd: EpollFd, op: i32, fd: RawFd, mut event: Event) -> io::Result<()> {
    libccall!(epoll_ctl(epoll_fd.fd, op, fd, &mut event as *mut _ as *mut libc::epoll_event))?;
    Ok(())
}

fn epoll_wait(epoll_fd: EpollFd,  events: &mut Vec<Event>, timeout: i64) -> io::Result<()> {
    let max_events = events.capacity();
    libccall!(epoll_wait(epoll_fd.fd, events.as_mut_ptr() as *mut libc::epoll_event,
			max_events as libc::c_int, timeout as libc::c_int))?;
    Ok(())
}

struct Watcher<T> {
    object: T,
    callback: fn(&mut T, &mut EvLoop),
}

trait Dispatchable {
    fn dispatch(&mut self, el: &mut EvLoop);
}

impl<T> Dispatchable for Watcher<T> {
    fn dispatch(&mut self, el: &mut EvLoop) {
	(self.callback)(&mut self.object, el);
    }
}

trait DispatchableFd: Dispatchable + AsRawFd {} 
type Ev = dyn DispatchableFd ;

struct EvLoop {
    fd: EpollFd,
    token_gen: TokenGen,
    watchers: Vec<Rc<RefCell<dyn Dispatchable>>>,
}

impl EvLoop {
    fn new() -> io::Result<EvLoop> {
	Ok(EvLoop{
	    fd: epoll_create()?,
	    token_gen: TokenGen::new(),
	    watchers: Vec::new(),
	})
    }
}
