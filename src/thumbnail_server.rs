use axum::Router;
use tower_http::services::ServeDir;


pub fn serve(path: &std::path::Path, listen_address: &str) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let app = Router::new().fallback_service(ServeDir::new(path));
        let listener = tokio::net::TcpListener::bind(listen_address).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });
}
