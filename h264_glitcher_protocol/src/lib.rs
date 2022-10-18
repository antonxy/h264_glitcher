#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Message {
    Videos(Vec<Video>),
    State(State),
    Event(Event),
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Video {
    pub id: usize,
    pub name: String,
    pub thumbnail_png: Vec<u8>,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct State {
    pub fps: f32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub enum Event {
    SetVideo(usize),
}