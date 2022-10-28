use hyper::server::conn::AddrStream;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, Method};
use std::collections::HashMap;
use std::io::{Error, ErrorKind, BufRead};
use std::{convert::Infallible, net::SocketAddr};
use tokio::time::Duration;
use tokio::time::timeout;

mod dns;
mod message;
mod nametree;

use message::Message;
use dns::ResourceRecord;

fn genid() -> u16 {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("oops");
    return ((buf[0] as u16) << 8) | (buf[1] as u16);
}

fn create_response(q: &Message, rcode: u32, ans: &Vec<ResourceRecord>,
		   ns: &Vec<ResourceRecord>, ads: &Vec<ResourceRecord>) -> Message {
    let mut r = Message::new();
    r.id = q.id;
    r.qr = 1;
    r.opcode = q.opcode;
    r.aa = 1; // ?
    r.tc = 0;
    r.rd = q.rd;
    r.ra = 1;
    r.ad = 0;
    r.cd = 0;
    r.rcode = rcode;
    assert!(q.questions.len() == 1);
    r.questions.push(q.questions[0].clone());
    for a in ans {
	match a.rtype {
	    dns::RecordType::UNKNOWN(_) => (),
	    _ =>  r.answers.push(a.clone()),
	}
    }
    for n in ns {
	match n.rtype {
	    dns::RecordType::UNKNOWN(_) => (),
	    _ =>  r.nameservers.push(n.clone()),
	}
    }
    for a in ads {
	match a.rtype {
	    dns::RecordType::UNKNOWN(_) => (),
	    _ =>  r.additional.push(a.clone()),
	}
    }
    r
}

#[derive(Debug)]
struct FwdrAnswer {
    rcode: u32,
    answers: Vec<ResourceRecord>,
    nameservers: Vec<ResourceRecord>,
    additional: Vec<ResourceRecord>,
}

#[derive(Debug)]
struct Question {
    name: String,
    rtype: dns::RecordType,
    rsp_to: tokio::sync::mpsc::Sender<FwdrAnswer>,
}

async fn upstream_query_a(socket: &mut tokio::net::UdpSocket, name: &str, qtype: dns::RecordType) -> Result<(), std::io::Error> {
    let mut msg = Message::new();
    msg.id = genid() as u32;
    msg.qr = 0; // query
    msg.opcode = 0; // standard query
    msg.rd = 1; // recursive query
    msg.questions.push(message::Question{
        name: name.to_owned(),
        qtype: qtype,
        class: dns::RecordClass::IN, // IN
    });
    let data = msg.into_bytes().expect("oops");
    socket.send(&data).await.expect("oops");
    Ok(())
}

async fn upstream_reply_a(socket: &mut tokio::net::UdpSocket) -> Result<FwdrAnswer, std::io::Error> {
    let mut buf = [0; 512];
    let amt = match timeout(Duration::from_secs(2), socket.recv(&mut buf)).await {
	Err(_) => {
	    eprintln!("Upstream timeout");
	    return Err(Error::new(ErrorKind::Other, "BitCursor overflow"));
	},
	Ok(amt) => amt?,
    };
    let msg = Message::from(&mut buf[..amt]).expect("oops");
    println!("Upstream answer: {:?}", msg);
    Ok(FwdrAnswer{rcode: msg.rcode, answers: msg.answers,
		  nameservers: msg.nameservers, additional: msg.additional})
}

async fn handle_fwd(q: Question) -> Result<(), std::io::Error> {
    let mut socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await.expect("oops");
    socket.connect("9.9.9.9:53").await.expect("oops");
    println!("Upstream query: {:?}, {:?}", q.name, q.rtype);
    upstream_query_a(&mut socket, &q.name, q.rtype).await.expect("oops");
    let answer = match upstream_reply_a(&mut socket).await {
	Err(_) => {
	    if let Err(_) = q.rsp_to.send(FwdrAnswer{
		rcode: 5,
		answers: Vec::new(),
		nameservers: Vec::new(),
		additional: Vec::new(),
	    }).await {
		eprintln!("handle_fwd: Failed to send answer");
	    }
	    return Ok(());
	},
	Ok(a) => a,
    };
    if let Err(err) = q.rsp_to.send(answer).await {
	eprintln!("handle_fwd: Failed to send answer: {:?}", err);
    }
    Ok(())
}

async fn forwarder(mut qs: tokio::sync::mpsc::Receiver<Question>) -> Result<(), std::io::Error> {
    loop {
	tokio::select! {
	    Some(q) = qs.recv() => {
		tokio::spawn(async move {
		    handle_fwd(q).await.expect("oops");
		});
	    }
	}
    }
}

async fn udp_server(questions: tokio::sync::mpsc::Sender<(Vec<u8>, std::net::SocketAddr)>,
		    mut answers: tokio::sync::mpsc::Receiver<(Vec<u8>, std::net::SocketAddr)>)
		    -> Result<(), std::io::Error> {
    let server = tokio::net::UdpSocket::bind("0.0.0.0:3553").await.expect("oops");
    loop {
	let mut buf = [0; 512];
	tokio::select! {
	    Ok((amt, source)) = server.recv_from(&mut buf) => {
		questions.send((buf[0..amt].to_vec(), source)).await.expect("oops");
	    },
	    Some((data, source)) = answers.recv() => {
		server.send_to(&data.to_vec(), source).await.expect("oops");
	    },
	    else => {
		panic!("oops");
	    }
	}
    }
}

async fn handle_question(src: std::net::SocketAddr, message: Message,
			 fwder: tokio::sync::mpsc::Sender<Question>,
			 rsp_to: tokio::sync::mpsc::Sender<(Vec<u8>, std::net::SocketAddr)>) {
    if message.questions.len() != 1 {
	panic!("Only 1 query supported!");
    }
    println!("UDP Question: {:?}", message);
    let name = &message.questions[0].name;
    let (f_tx, mut f_rx) = tokio::sync::mpsc::channel::<FwdrAnswer>(1);
    let fq = Question{
	name: name.to_owned(),
	rtype: message.questions[0].qtype,
	rsp_to: f_tx,
    };
    fwder.send(fq).await.expect("oops");
    if let Some(fa) = f_rx.recv().await {
	let mut answer = create_response(&message, fa.rcode, &fa.answers,
					 &fa.nameservers, &fa.additional);
	println!("UDP Answer: {:?}", answer);
	let data = answer.into_bytes().expect("oops");
	rsp_to.send((data, src)).await.expect("oops");
    }    
}

async fn handle_doh_question(req: Request<Body>, fwder: tokio::sync::mpsc::Sender<Question>) -> Result<Response<Body>, Infallible> {

    let params: HashMap<String, String> = req.uri().query().map(|v| {
	url::form_urlencoded::parse(v.as_bytes()).into_owned().collect()
    }).expect("oops");
    if matches!(req.method(), &Method::POST) {
	panic!("oops");
    }
    let mut payload = match base64_url::decode(&params["dns"].to_owned()) {
	Ok(payload) => payload,
	_ => {
	    eprintln!("oops");
	    return Ok(Response::builder().status(500).body(Body::from("oops")).expect("oops"));
	},
    };
    let message = Message::from(&mut payload).expect("oops");
    println!("DoH Question: {:?}", message);
    if let dns::RecordType::UNKNOWN(_) = message.questions[0].qtype {
	let mut answer = create_response(&message, 4, &Vec::new(), &Vec::new(),
					 &Vec::new());
	println!("Not supported - DoH Answer: {:?}", answer);
	let data = answer.into_bytes().expect("oops");
	return Ok(Response::new(Body::from(data)));
    }

    let name = &message.questions[0].name;
    let (f_tx, mut f_rx) = tokio::sync::mpsc::channel::<FwdrAnswer>(1);
    let fq = Question{
	name: name.to_owned(),
	rtype: message.questions[0].qtype,
	rsp_to: f_tx,
    };
    fwder.send(fq).await.expect("oops");
    if let Some(fa) = f_rx.recv().await {
	let mut answer = create_response(&message, fa.rcode, &fa.answers,
					 &fa.nameservers, &fa.additional);
	println!("DoH Answer: {:?}", answer);
	let data = answer.into_bytes().expect("oops");
	return Ok(Response::new(Body::from(data)));
    }
    eprintln!("DoH failed waiting for answer. Why?");
    Ok(Response::builder().status(500).body(Body::from("oops")).expect("oops"))
}

fn run_doh(fwder: tokio::sync::mpsc::Sender<Question>) {
    let addr = SocketAddr::from(([127, 0, 0, 1], 4443));
    
    let make_svc = make_service_fn(move |_conn: &AddrStream| {
	let fwder = fwder.clone();
        let service = service_fn(move |req| {
	    handle_doh_question(req, fwder.clone())
	});
	async move {Ok::<_, Infallible>(service)}
    });
    
    let server = Server::bind(&addr).serve(make_svc);
    
    tokio::spawn(async move {
	if let Err(e) = server.await {
            eprintln!("server error: {}", e);
	}
    });


}

fn read_line(l: &str) {
    // Remove comments
    let parts: Vec<&str> = l.split("#").collect();
    if parts[0].len() == 0 {
	return;
    }
    // Split Addr/Name
    let parts: Vec<&str> = parts[0].split(" ").collect();
    let parts: Vec<&str> = parts.into_iter().filter(|w| w.len() != 0).collect();
    if parts.len() < 2 {
	return;
    }
    match parts[0].parse::<std::net::IpAddr>() {
	Ok(addr) => println!("{:?}, {:?}", addr, parts[1]),
	Err(e) => println!("{:?}, {:?}", parts[0], e),
    }
}

fn read_hosts() {
    let file = std::fs::File::open("hosts").expect("oops");
    let lines = std::io::BufReader::new(file).lines();

    for l in lines {
	if let Ok(txt) = l {
	    read_line(&txt);
	}
    }
}
    
#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    read_hosts();
    
    //let mut cache = Cache::new();
    let (udp_q_tx, mut udp_q_rx) = tokio::sync::mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(128);
    let (udp_r_tx, udp_r_rx) = tokio::sync::mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(128);
    tokio::spawn(udp_server(udp_q_tx, udp_r_rx));

    let (fwd_q_tx, fwd_q_rx) = tokio::sync::mpsc::channel::<Question>(128);
    tokio::spawn(forwarder(fwd_q_rx));

    run_doh(fwd_q_tx.clone());

    loop {
	tokio::select!{
	    Some((mut qdata, src)) = udp_q_rx.recv() => {
		handle_question(src, Message::from(&mut qdata).expect("oops"),
				fwd_q_tx.clone(), udp_r_tx.clone()).await;
	    }
	    else => {
		panic!("oops");
	    }
	}
    }
}
