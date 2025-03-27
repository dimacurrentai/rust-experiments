use shared::http;

#[tokio::main]
async fn main() {
  http::run_server().await;
}
