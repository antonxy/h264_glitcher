use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, WebSocketUpgrade, State};
use axum::extract::ws::{Message, WebSocket};
use axum::response::IntoResponse;
use rosc::OscPacket;
use tower_http::services::ServeDir;

use crate::osc_handler::OSCHandler;


async fn ws_handler(ws: WebSocketUpgrade, State(osc_handler): State<Arc<OSCHandler>>) -> impl IntoResponse {
    // Upgrade the HTTP connection and spawn the socket handler
    ws.on_upgrade(|socket| handle_socket(socket, osc_handler))
}

async fn handle_socket(mut socket: WebSocket, osc_handler: Arc<OSCHandler>) {
    // Maybe we want to collect the senders somewhere central for sending state updates:
    // use futures_util::{sink::SinkExt, stream::StreamExt};
    // let (mut sender, mut receiver) = socket.split();

    while let Some(Ok(msg)) = socket.recv().await {
        match msg {
            Message::Binary(data) => {
                // TODO duplicated in osc_listener, move into OSCHandler
                let (_, packet) = rosc::decoder::decode_udp(&data).unwrap();
                let parse_result = match packet {
                    OscPacket::Message(ref msg) => {
                        osc_handler.handle_osc_message(&msg)
                    }
                    OscPacket::Bundle(_) => {
                        eprintln!("Received bundle but they are currently not handled");
                        Err(())
                    }
                };

                if parse_result.is_err() {
                    eprintln!("Failed to parse OSC Packet: {:?}", packet);
                }
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
}

fn build_video_names_json(paths: Vec<PathBuf>) -> String {
    let mut video_names = paths.iter().map(|p| p.file_stem().unwrap().to_string_lossy()); // TODO this is duplicated in osc thread

    let mut builder = String::with_capacity(4000);
    builder.push('[');

    if let Some(name) = video_names.next() {
        builder.push('"');
        builder.push_str(&name);
        builder.push('"');

        for name in video_names {
            builder.push(',');
            builder.push('"');
            builder.push_str(&name);
            builder.push('"');
        }
    }

    builder.push(']');
    builder
}

async fn thumbnail_handler(Path(video_idx): Path<i32>) -> String {
    // TODO return video thumbnail
   video_idx.to_string()
}

pub fn serve(path: &std::path::Path, listen_address: &str, videos: Vec<PathBuf>, osc_handler: OSCHandler) {
    println!("Opening webserver at {}", listen_address);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let video_names_json : &'static str = build_video_names_json(videos).leak();
    rt.block_on(async {
        let app = Router::new()
            .route("/ws", axum::routing::get(ws_handler)).with_state(Arc::new(osc_handler))
            .route("/videos", axum::routing::get(video_names_json))
            .route("/thumbnail/{idx}", axum::routing::get(thumbnail_handler))
            .fallback_service(ServeDir::new("ui"));
        let listener = tokio::net::TcpListener::bind(listen_address).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
