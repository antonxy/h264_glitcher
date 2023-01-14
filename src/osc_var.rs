use std::{net::{UdpSocket, SocketAddr}, ops::Deref, time::Duration, convert::TryInto};

use rosc::{encoder, OscPacket, OscMessage, OscType};

#[derive(Clone)]
pub struct OscVar<T> {
    pub value: T,
    pub changed_outgoing: bool,
    pub changed_incoming: bool,
    pub address: String,
}

impl<T: Default> Default for OscVar<T> {
    fn default() -> Self {
        Self { value: T::default(), changed_outgoing: true, changed_incoming: false, address: "".to_string() }
    }
}

impl<T> Deref for OscVar<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T : PartialEq + OscValue + Copy + OscValue<Target = T>> OscVar<T> {
    pub fn new<S: Into<String>>(address: S, value: T) -> Self {
        Self { value: value, changed_outgoing: true, changed_incoming: false, address: address.into() }
    }

    pub fn set(&mut self, value: T) {
        if self.value != value {
            self.value = value;
            self.changed_outgoing = true;
        }
    }

    pub fn set_changed(&mut self) {
        self.changed_outgoing = true;
    }

    pub fn set_handled(&mut self) {
        self.changed_incoming = false;
    }

    pub fn changed(&self) -> bool {
        self.changed_incoming || self.changed_outgoing
    }

    pub fn send(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
        let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
            addr: self.address.clone(),
            args: self.value.to_args(),
        })).unwrap();
        socket.send_to(&msg_buf, client_addr).unwrap();
        self.changed_outgoing = false;
    }

    pub fn send_if_changed(&mut self, socket: &UdpSocket, client_addr: &SocketAddr) {
        if self.changed_outgoing {
            self.send(socket, client_addr);
        }
    }

    pub fn handle_osc_message(&mut self, msg: &OscMessage) -> bool {
        if msg.addr == self.address {
            let value = T::try_from_args(&msg.args).unwrap();
            if self.value != value {
                self.value = value;
                self.changed_incoming = true;
            }
            true
        } else {
            false
        }
    }
}

pub trait OscValue {
    type Target;
    fn to_args(self) -> Vec<OscType>;
    fn try_from_args(args: &Vec<OscType>) -> Option<Self::Target>;
}

#[derive(PartialEq, Copy, Clone)]
pub struct LoopRange(pub Option<(f32, f32)>);
impl OscValue for LoopRange {
    type Target = LoopRange;
    fn to_args(self) -> Vec<OscType> {
        match self.0 {
            Some((from, to)) => vec![Into::into(from), Into::into(to)],
            None => vec![Into::into(0.0), Into::into(1.0)],
        }
        
    }

    fn try_from_args(args: &Vec<OscType>) -> Option<Self> {
        Some(Self(Some((args[0].clone().float()?, args[1].clone().float()?))))
    }
}

impl OscValue for Duration {
    type Target = Duration;
    fn to_args(self) -> Vec<OscType> {
        vec![self.as_secs_f32().into()]
        
    }

    fn try_from_args(args: &Vec<OscType>) -> Option<Self> {
        Some(Duration::from_secs_f32(args[0].clone().float()?))
    }
}

impl OscValue for usize {
    type Target = usize;
    fn to_args(self) -> Vec<OscType> {
        vec![(self as i32).into()]
        
    }

    fn try_from_args(args: &Vec<OscType>) -> Option<Self> {
        args[0].clone().int()?.try_into().ok()
    }
}

macro_rules! value_impl {
    ($(($name:ident, $variant:ident, $ty:ty)),*) => {
        $(
        impl OscValue for $ty {
            type Target = $ty;
            fn to_args(self) -> Vec<OscType> {
                vec![Into::into(self)]
            }
        
            fn try_from_args(args: &Vec<OscType>) -> Option<Self::Target> {
                Some(args[0].clone().$name()?)
            }
        }
        )*
    }
}
value_impl! {
    (int, Int, i32),
    (float, Float, f32),
    (string, String, String),
    (long, Long, i64),
    (double, Double, f64),
    (char, Char, char),
    (bool, Bool, bool)
}