use anyhow::Result;
use futures_util::StreamExt;
use std::env;
use testcontainers_example::create_redis_client;

#[tokio::main]
async fn main() -> Result<()> {
  let args: Vec<String> = env::args().collect();

  let channel = args.get(1).map(|s| s.as_str()).unwrap_or("default_channel");

  let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

  println!("Connecting to Redis at: {}", redis_url);
  println!("Subscribing to channel: '{}'", channel);
  println!("Waiting for messages... (Press Ctrl+C to exit)");

  let client = create_redis_client(&redis_url).await?;
  let mut pubsub = client.get_async_pubsub().await?;

  pubsub.subscribe(channel).await?;

  let mut stream = pubsub.on_message();
  while let Some(message) = stream.next().await {
    let channel_name = message.get_channel_name();
    let payload = message.get_payload::<String>()?;
    println!("[{}] {}", channel_name, payload);
  }

  Ok(())
}
