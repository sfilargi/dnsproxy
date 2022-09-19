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

fn epoll_ctl(epoll_fd: &EpollFd, op: i32, fd: RawFd, mut event: Event) -> io::Result<()> {
    libccall!(epoll_ctl(epoll_fd.fd, op, fd, &mut event as *mut _ as *mut libc::epoll_event))?;
    Ok(())
}

fn epoll_wait(epoll_fd: &EpollFd,  events: &mut Vec<Event>, timeout: i64) -> io::Result<()> {
    let max_events = events.capacity();
    libccall!(epoll_wait(epoll_fd.fd, events.as_mut_ptr() as *mut libc::epoll_event,
			max_events as libc::c_int, timeout as libc::c_int))?;
    Ok(())
}

struct FDWatcher<F, T> where F: AsRawFd {
    fd: F,
    object: T,
    callback: fn(&mut T, &mut EvLoop, &mut F),
    callback_thunk: fn(&mut FDWatcher<F, T>, &mut EvLoop),
}

impl<F, T> FDWatcher<F, T> where F: AsRawFd {
    fn cb_thunk(&mut self, ev: &mut EvLoop) {
	(self.callback)(&mut self.object, ev, &mut self.fd);
	
    }
    fn new(fd: F, object: T, callback: fn(&mut T, &mut EvLoop, &mut F)) -> FDWatcher<F, T> {
	FDWatcher{
	    fd,
	    object,
	    callback,
	    callback_thunk: Self::cb_thunk,
	}
    }
}


trait FDDispatchable {
    fn fd(&mut self) -> &mut dyn AsRawFd;
    fn dispatch(&mut self, el: &mut EvLoop);
}

impl<F, T> FDDispatchable for FDWatcher<F, T> where F: AsRawFd {
    fn fd(&mut self) -> &mut dyn AsRawFd {
	&mut self.fd
    }
    fn dispatch(&mut self, el: &mut EvLoop) {
	(self.callback_thunk)(self, el);
    }
}

struct EvLoop {
    fd: EpollFd,
    token_gen: TokenGen,
    watchers: Vec<Rc<RefCell<dyn FDDispatchable>>>,
}

impl EvLoop {
    pub fn new() -> io::Result<EvLoop> {
	Ok(EvLoop{
	    fd: epoll_create()?,
	    token_gen: TokenGen::new(),
	    watchers: Vec::new(),
	})
    }

    pub fn watch<F: 'static, T: 'static>(&mut self, fd: F, object: T, callback: fn(&mut T, &mut EvLoop, &mut F))
    where F: AsRawFd {
	epoll_ctl(&self.fd, 0, fd.as_raw_fd(), Event{events: libc::EPOLLIN as u32, data: 0});
	self.watchers.push(Rc::new(RefCell::new(FDWatcher::new(fd, object, callback))));
    }

    pub fn run(&mut self) {
    }
}
