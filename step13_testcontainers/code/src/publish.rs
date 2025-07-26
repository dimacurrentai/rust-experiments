use anyhow::{Result, anyhow};
use std::env;
use testcontainers_example::{create_redis_client, publish_with_persistence};

#[tokio::main]
async fn main() -> Result<()> {
  let args: Vec<String> = env::args().collect();

  if args.len() < 2 {
    eprintln!("Usage: {} <message> [channel]", args[0]);
    eprintln!("Environment variables:");
    eprintln!("  REDIS_URL - Redis connection URL (default: redis://127.0.0.1:6379)");
    std::process::exit(1);
  }

  let message = &args[1];
  let channel = args.get(2).map(|s| s.as_str()).unwrap_or("default_channel");

  let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());

  println!("Connecting to Redis at: {}", redis_url);
  println!("Publishing message: '{}' to channel: '{}'", message, channel);

  let client = create_redis_client(&redis_url).await?;
  let mut connection = client.get_multiplexed_async_connection().await?;

  match publish_with_persistence(&mut connection, channel, message).await {
    Ok(message_id) => {
      println!("Message published successfully! Message ID: {}", message_id);
      Ok(())
    }
    Err(e) => {
      eprintln!("Failed to publish message: {}", e);
      Err(anyhow!("Publication failed: {}", e))
    }
  }
}
