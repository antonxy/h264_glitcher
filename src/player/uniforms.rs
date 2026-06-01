use rosc::{OscPacket, OscType};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Clone)]
pub struct Uniforms {
    pub time: f32,
    pub flash: f32,
    pub effect_color: [f32; 3],
}

impl Default for Uniforms {
    fn default() -> Self {
        Self {
            time: 0.0,
            flash: 0.0,
            effect_color: [1.0, 1.0, 1.0],
        }
    }
}

pub fn start_osc_listener(uniforms: Arc<Mutex<Uniforms>>, port: u16) {
    let addr = format!("0.0.0.0:{}", port);
    let socket = match UdpSocket::bind(&addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to bind OSC socket on {}: {}", addr, e);
            return;
        }
    };

    thread::spawn(move || {
        let mut buf = [0u8; 1024];
        loop {
            match socket.recv(&mut buf) {
                Ok(size) => {
                    if let Ok((_, packet)) = rosc::decoder::decode_udp(&buf[..size]) {
                        handle_packet(&packet, &uniforms);
                    }
                }
                Err(e) => eprintln!("OSC recv error: {}", e),
            }
        }
    });
}

fn handle_packet(packet: &OscPacket, uniforms: &Arc<Mutex<Uniforms>>) {
    match packet {
        OscPacket::Message(msg) => {
            let mut u = uniforms.lock().unwrap();
            match msg.addr.as_str() {
                "/led/flash" => {
                    if let Some(OscType::Float(v)) = msg.args.first() {
                        u.flash = *v;
                    }
                }
                "/led/color" => {
                    if msg.args.len() >= 3 {
                        if let (Some(OscType::Float(r)), Some(OscType::Float(g)), Some(OscType::Float(b))) =
                            (msg.args.get(0), msg.args.get(1), msg.args.get(2))
                        {
                            u.effect_color = [*r, *g, *b];
                        }
                    }
                }
                _ => {}
            }
        }
        OscPacket::Bundle(bundle) => {
            for p in &bundle.content {
                handle_packet(p, uniforms);
            }
        }
    }
}
