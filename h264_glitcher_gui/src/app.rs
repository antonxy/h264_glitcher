use tracing::{event, Level};
use egui_extras::RetainedImage;
use poll_promise::Promise;
use ewebsock::{WsSender, WsReceiver};

struct Video {
    pub id: usize,
    pub name: String,
    pub texture: RetainedImage,
}

/// We derive Deserialize/Serialize so we can persist app state on shutdown.
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)] // if we add new fields, give them default values when deserializing old state
pub struct TemplateApp {
    glitch_server: String,

    #[serde(skip)]
    ws_sender: Option<WsSender>,
    #[serde(skip)]
    ws_receiver: Option<WsReceiver>,

    #[serde(skip)]
    videos: Vec<Option<Video>>, //Videos are optional since they might not have been sent by the server yet
}

impl Default for TemplateApp {
    fn default() -> Self {
        Self {
            glitch_server: "".to_owned(),
            ws_sender: None,
            ws_receiver: None,
            videos: Vec::new(),
        }
    }
}

impl TemplateApp {
    /// Called once before the first frame.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        event!(Level::INFO, "Start app");
        // This is also where you can customized the look at feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        if let Some(storage) = cc.storage {
            return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        }

        Default::default()
    }

    fn connect(&mut self, ctx: egui::Context) {
        event!(Level::INFO, "Connect to {:?}", self.glitch_server);
        let wakeup = move || ctx.request_repaint(); // wake up UI thread on new message
        let (sender, receiver) = ewebsock::connect_with_wakeup(self.glitch_server.clone(), wakeup).unwrap();
        self.ws_sender = Some(sender);
        self.ws_receiver = Some(receiver);
    }

    fn handle_ws_events(&mut self) {
        if let Some(ws_receiver) = &self.ws_receiver {
            while let Some(event) = ws_receiver.try_recv() {
                match event {
                    ewebsock::WsEvent::Message(ewebsock::WsMessage::Binary(data)) => {
                        let msg : h264_glitcher_protocol::Message = ciborium::de::from_reader(data.as_slice()).unwrap();
                        event!(Level::INFO, "Received message");
                        match msg {
                            h264_glitcher_protocol::Message::Video(video) => {
                                Self::handle_video_message(&mut self.videos, &video);
                            },
                            _ => event!(Level::INFO, "Received unhandled msg {:?}", msg),
                        }
                    },
                    _ => event!(Level::INFO, "Received unhandled {:?}", event),
                }
            }
        }
    }

    fn handle_video_message(videos: &mut Vec<Option<Video>>, video: &h264_glitcher_protocol::Video) {
        while video.id + 1 > videos.len() {
            videos.push(None);
        }
        videos[video.id] = Some(Video {
            id: video.id,
            name: video.name.clone(),
            texture: RetainedImage::from_image_bytes(video.name.clone(), &video.thumbnail_png).unwrap(),
        });
    }

}

impl eframe::App for TemplateApp {
    /// Called by the frame work to save state before shutdown.
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, self);
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut window = egui::Window::new("Settings")
                .id(egui::Id::new("demo_window_options")) // required since we change the title
                .resizable(false)
                .collapsible(true)
                .anchor(egui::Align2::LEFT_TOP, egui::Vec2::new(5., 5.));
            window.show(ctx, |ui| {
                ui.heading("Glitcher control");
                ui.horizontal(|ui| {
                    ui.label("Glitch server");
                    ui.add(egui::TextEdit::singleline(&mut self.glitch_server));
                    if ui.button("Connect").clicked() {
                        self.connect(ctx.clone());
                    }
                });
            });
            self.handle_ws_events();

            ui.horizontal_wrapped(|ui| {
                for video in self.videos.iter().filter_map(|v| v.as_ref()) {
                    let texture = &video.texture;
                    let img_size = 200.0 * texture.size_vec2() / texture.size_vec2().x;
                    if ui.add(egui::ImageButton::new(texture.texture_id(ctx), img_size)).clicked() {
                        let mut message = Vec::new();
                        ciborium::ser::into_writer(&h264_glitcher_protocol::Message::Event(h264_glitcher_protocol::Event::SetVideo(video.id)), &mut message);
                        self.ws_sender.as_mut().unwrap().send(ewebsock::WsMessage::Binary(message));
                    }
                }
            });
            egui::warn_if_debug_build(ui);
        });
    }
}
