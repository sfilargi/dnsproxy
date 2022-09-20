use libc;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::rc::Rc;
use std::cell::RefCell;
use bitflags::bitflags;

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
pub struct Event {
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

    Ok(())
}

enum EvResult {
    DONE,
    AGAIN,
}

struct FDWatcher<F, T> where F: AsRawFd {
    fd: F,
    object: T,
    callback: fn(&mut T, &mut EvLoop, &mut F) -> EvResult,
    callback_thunk: fn(&mut FDWatcher<F, T>, &mut EvLoop) -> EvResult,
}

impl<F, T> FDWatcher<F, T> where F: AsRawFd {
    fn cb_thunk(&mut self, ev: &mut EvLoop) -> EvResult {
	(self.callback)(&mut self.object, ev, &mut self.fd)
	
    }
    fn new(fd: F, object: T, callback: fn(&mut T, &mut EvLoop, &mut F) -> EvResult) -> FDWatcher<F, T> {
	FDWatcher{
	    fd,
	    object,
	    callback,
	    callback_thunk: Self::cb_thunk,
	}
    }
}

trait FDDispatchable {
    fn fd(&self) -> &dyn AsRawFd;
    fn dispatch(&mut self, el: &mut EvLoop) -> EvResult;
}

impl<F, T> FDDispatchable for FDWatcher<F, T> where F: AsRawFd {
    fn fd(&self) -> &dyn AsRawFd {
	&self.fd
    }
    fn dispatch(&mut self, el: &mut EvLoop) -> EvResult {
	(self.callback_thunk)(self, el)
    }
}

struct TokenFactory {
    recycled: Vec<u64>,
    next_new: u64,
}

impl TokenFactory {
    pub fn new() -> TokenFactory {
	TokenFactory{
	    recycled: Vec::new(),
	    next_new: 0,
	}
    }
    pub fn acquire(&mut self) -> u64 {
	if self.recycled.len() != 0 {
	    self.recycled.pop().expect("oops")
	} else {
	    self.next_new += 1;
	    self.next_new
	}
    }
    pub fn release(&mut self, token: u64) {
	self.recycled.push(token);
    }
}

bitflags! {
    pub struct Op: u32 {
	const READ  = 0b0001;
	const WRITE = 0b0010;
    }
}

struct EvFdWatcher {
    fd: RawFd,
    w: Rc<RefCell<dyn FDDispatchable>>,
}

struct EvLoop {
    fd: EpollFd,
    token_factory: TokenFactory,
    watchers: Vec<EvFdWatcher>,
}

impl EvLoop {
    pub fn new() -> io::Result<EvLoop> {
	Ok(EvLoop{
	    fd: epoll_create()?,
	    token_factory: TokenFactory::new(),
	    watchers: Vec::new(),
	})
    }

    pub fn watch<F: 'static, T: 'static>(&mut self, fd: F, op: Op, object: T, callback: fn(&mut T, &mut EvLoop, &mut F) -> EvResult)
    where F: AsRawFd {
	let mut events = 0;
	if op & Op::READ == Op::READ {
	    events |= libc::EPOLLIN;
	}
	if op & Op::WRITE == Op::WRITE {
	    events |= libc::EPOLLOUT;
	}
	println!("Adding... fd: {}, {}", fd.as_raw_fd(), events);
	let token = self.token_factory.acquire();
	epoll_ctl(&self.fd, libc::EPOLL_CTL_ADD, fd.as_raw_fd(), Event{events: events as u32, data: token});
	self.watchers.push(EvFdWatcher{
	    fd: fd.as_raw_fd(),
	    w: Rc::new(RefCell::new(FDWatcher::new(fd, object, callback)))
	});
    }

    fn unwatch(&mut self, token: u64) {
	let fd = self.watchers[0].fd;
	epoll_ctl(&self.fd, libc::EPOLL_CTL_DEL, fd, Event{events: 0, data: 0});
	self.watchers.pop();
	println!("Done done done!");
    }

    pub fn run(&mut self) {
	let size = 1024;
	let timeout = -1;
	let mut events: Vec<Event> = Vec::with_capacity(size);
	println!("Running....");
	loop {
	    let max_events = events.capacity();
	    events.clear();
	    let n = match libccall!(epoll_wait(
		self.fd.fd,
		events.as_mut_ptr() as *mut libc::epoll_event,
		max_events as libc::c_int,
		timeout as libc::c_int,
	    )) {
		Ok(v) => v,
		Err(e) => panic!("error during epoll wait: {}", e),
	    };
	    
	    // safe  as long as the kernel does nothing wrong - copied from mio
	    unsafe { events.set_len(n as usize) };
	    
	    println!("Events, events, events! {}", n);
	    for event in &events {
		let token = event.data;
		println!("Got event! {}", token);
		let w = self.watchers[0].w.clone();
		let res = w.borrow_mut().dispatch(self);
		match res {
		    EvResult::DONE => self.unwatch(1),
		    _ => (),
		};
	    }
	}
    }
}


#[cfg(test)]
mod tests {  
    use super::*;

    use socketpair::*;
    use std::io::{self, Read, Write};
    use std::str::from_utf8;
    use std::thread;

    fn echo(_: &mut (), ev: &mut EvLoop, fd: &mut SocketpairStream) -> EvResult {
	println!("Modest success! {}", fd.as_raw_fd());
	let mut buf: [u8; 128] = [0; 128];
	let x = fd.read(&mut buf).expect("oops");
	if x == 0 {
	    ev.unwatch(0);
	    return EvResult::AGAIN;
	}
	println!("Getting there! {}, {}", x, from_utf8(&buf).expect("oops"));
	fd.write(&buf);
	EvResult::AGAIN
    }
    
    #[test]
    fn basic() {
	let (mut a, mut b) = socketpair_stream().expect("oops");

	let _t = thread::spawn(move || {
	    let mut ev = EvLoop::new().expect("oops");
	    ev.watch(a, Op::READ, (), echo);
	    ev.run();
	});
	b.write("ping".as_bytes()).expect("oops");
	let mut buf = [0; 128];
	b.read(&mut buf).expect("oops");
	println!("Great success! {}", from_utf8(&buf).expect("oops"));
    }
}
