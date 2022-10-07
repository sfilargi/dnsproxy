use std::net::Ipv4Addr;
    
mod message;
mod nametree;

use message::Message;
use message::Question;
use message::ResourceData;
use message::ResourceRecord;

fn genid() -> u16 {
    let mut buf = [0u8; 16];
    getrandom::getrandom(&mut buf).expect("oops");
    return ((buf[0] as u16) << 8) | (buf[1] as u16);
}

fn encode_reply(q: &Message, r: &Message) -> Result<Vec<u8>, std::io::Error> {
    let mut reply = Message::new();
    reply.id = q.id;
    reply.qr = 1; // reply
    reply.opcode = q.opcode;
    reply.aa = r.aa;
    reply.tc = r.tc;
    reply.rd = r.rd;
    reply.ra = r.ra;
    reply.ad = r.ad;
    reply.cd = r.cd;
    reply.rcode = r.rcode;
    for qs in &q.questions {
        reply.questions.push(qs.clone());
    }
    for ans in &r.answers {
        reply.answers.push(ans.clone());
    }
    return Ok(reply.into_bytes().expect("oops"));
}

fn create_response(q: &Message, a: &Ipv4Addr, ttl: u64) -> Message {
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
    r.rcode = 0;
    assert!(q.questions.len() == 1);
    for qs in &q.questions {
	r.questions.push(qs.clone());
	r.answers.push(ResourceRecord{
	    name: qs.name.clone(),
	    typ: qs.typ,
	    ttl: ttl as u32,
	    class: qs.class,
	    data: ResourceData::IPv4(a.clone()),
	});
    }
    r
}

#[derive(Debug)]
struct FwdrAnswer {
    a: Ipv4Addr,
    ttl: u64,
}

#[derive(Debug)]
struct FwdrQuestion {
    name: String,
    rsp_to: tokio::sync::mpsc::Sender<FwdrAnswer>,
}

async fn upstream_query_a(socket: &mut tokio::net::UdpSocket, name: &str) -> Result<(), std::io::Error> {
    let mut msg = Message::new();
    msg.id = genid() as u32;
    msg.qr = 0; // query
    msg.opcode = 0; // standard query
    msg.rd = 1; // recursive query
    msg.questions.push(Question{
        name: name.to_owned(),
        typ: 1, // A
        class: 1, // IN
    });
    let data = msg.into_bytes().expect("oops");
    println!("send to upstream");
    socket.send(&data).await.expect("oops");
    Ok(())
}

async fn upstream_reply_a(socket: &mut tokio::net::UdpSocket) -> Result<FwdrAnswer, std::io::Error> {
    println!("Waiting upstream reply");
    let mut buf = [0; 512];
    let amt = socket.recv(&mut buf).await.expect("oops");
    let msg = Message::from(&mut buf[..amt]).expect("oops");
    println!("Upstream reply: {:?}", msg);
    if msg.rcode != 0 {
	panic!("oops");
    }
    if let ResourceData::IPv4(a) = msg.answers[0].data {
	return Ok(FwdrAnswer{
	    a: a,
	    ttl: msg.answers[0].ttl as u64,
	});
    }
    panic!("oops");
}

async fn handle_fwd(q: FwdrQuestion) -> Result<(), std::io::Error> {
    let mut socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await.expect("oops");
    socket.connect("9.9.9.9:53").await.expect("oops");
    println!("Resolver got question");
    upstream_query_a(&mut socket, &q.name).await.expect("oops");
    println!("Resolver forwarded question");
    let answer = upstream_reply_a(&mut socket).await.expect("oops");
    q.rsp_to.send(answer).await.expect("oops");
    Ok(())
}

async fn forwarder(mut qs: tokio::sync::mpsc::Receiver<FwdrQuestion>) -> Result<(), std::io::Error> {
    loop {
	tokio::select! {
	    Some(q) = qs.recv() => {
		handle_fwd(q).await.expect("oops");
	    },
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
			 fwder: tokio::sync::mpsc::Sender<FwdrQuestion>,
			 rsp_to: tokio::sync::mpsc::Sender<(Vec<u8>, std::net::SocketAddr)>) {
    if message.questions.len() != 1 {
	panic!("Only 1 query supported!");
    }
    if message.questions[0].typ != 1 && message.questions[0].typ != 28 {
	panic!("Only type A and AAAA questions supported!");
    }
    println!("{:?}", message);
    let name = &message.questions[0].name;
    let (f_tx, mut f_rx) = tokio::sync::mpsc::channel::<FwdrAnswer>(1);
    let fq = FwdrQuestion{
	name: name.to_owned(),
	rsp_to: f_tx,
    };
    fwder.send(fq).await.expect("oops");
    if let Some(fa) = f_rx.recv().await {
	println!("Got response from forwader!");
	let answer = create_response(&message, &fa.a, fa.ttl);
	let data = encode_reply(&message, &answer).expect("oops");
	rsp_to.send((data, src)).await.expect("oops");
    }
    
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    //let mut cache = Cache::new();
    let (udp_q_tx, mut udp_q_rx) = tokio::sync::mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(128);
    let (udp_r_tx, udp_r_rx) = tokio::sync::mpsc::channel::<(Vec<u8>, std::net::SocketAddr)>(128);
    tokio::spawn(udp_server(udp_q_tx, udp_r_rx));

    let (fwd_q_tx, fwd_q_rx) = tokio::sync::mpsc::channel::<FwdrQuestion>(128);
    tokio::spawn(forwarder(fwd_q_rx));

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
