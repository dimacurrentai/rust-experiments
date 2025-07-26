use anyhow::Result;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Mutex;

const PUBLISH_SCRIPT: &str = r#"
  local channel = ARGV[1]
  local message = ARGV[2]
  
  local existing_id = redis.call('HGET', 'msg_content_to_id', message)
  if existing_id then
    return existing_id
  end
  
  local id = redis.call('INCR', 'msg_counter')
  local key = 'msg:' .. id
  redis.call('SET', key, message)
  redis.call('HSET', 'msg_content_to_id', message, tostring(id))
  redis.call('PUBLISH', channel, message)
  return tostring(id)
"#;

static SCRIPT_SHA: LazyLock<Mutex<Option<String>>> = LazyLock::new(|| Mutex::new(None));

pub async fn create_redis_client(redis_url: &str) -> Result<redis::Client> {
  Ok(redis::Client::open(redis_url)?)
}

pub async fn publish_with_persistence(
  connection: &mut redis::aio::MultiplexedConnection, channel: &str, message: &str,
) -> Result<String> {
  loop {
    let sha = match { SCRIPT_SHA.lock().await.clone() } {
      Some(cached_sha) => cached_sha,
      None => {
        let sha = redis::cmd("SCRIPT").arg("LOAD").arg(PUBLISH_SCRIPT).query_async::<String>(connection).await?;
        *SCRIPT_SHA.lock().await = Some(sha.clone());
        sha
      }
    };

    match redis::cmd("EVALSHA").arg(&sha).arg(0).arg(channel).arg(message).query_async::<String>(connection).await {
      Ok(result) => return Ok(result),
      Err(e) => {
        if e.to_string().contains("NoScriptError") {
          *SCRIPT_SHA.lock().await = None;
          tokio::time::sleep(Duration::from_millis(500)).await;
          continue;
        }
        return Err(e.into());
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use anyhow::anyhow;
  use futures_util::StreamExt;
  use redis::AsyncCommands;
  use testcontainers::{Container, clients};
  use testcontainers_modules::redis::Redis;
  use tokio::time::timeout;

  fn get_redis_url(container: &Container<Redis>) -> String {
    let port = container.get_host_port_ipv4(6379);
    format!("redis://127.0.0.1:{}", port)
  }

  #[tokio::test]
  async fn test_redis_set_get() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url)?;
    let mut con = client.get_multiplexed_async_connection().await?;

    let _: () = con.set("test_key", "test_value").await?;
    let val: String = con.get("test_key").await?;

    assert_eq!(val, "test_value");
    println!("Redis set/get test passed! Got value: {}", val);
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_pubsub_basic() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.subscribe("test_channel").await?;

    let publish_task = tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(100)).await;
      let _ = publish_with_persistence(&mut publisher, "test_channel", "hello world").await.unwrap();
    });

    let mut stream = pubsub.on_message();
    let message = timeout(Duration::from_secs(5), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
    assert_eq!(message.get_channel_name(), "test_channel");
    assert_eq!(message.get_payload::<String>()?, "hello world");

    publish_task.await?;
    println!("Redis pub-sub basic test passed!");
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_pubsub_multiple_messages() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.subscribe("multi_channel").await?;

    let messages = vec!["message1", "message2", "message3"];
    let expected_messages = messages.clone();

    let publish_task = tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(100)).await;
      for msg in messages {
        let _ = publish_with_persistence(&mut publisher, "multi_channel", msg).await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
      }
    });

    let mut received_messages = Vec::new();
    let mut stream = pubsub.on_message();
    for _ in 0..3 {
      let message = timeout(Duration::from_secs(5), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
      received_messages.push(message.get_payload::<String>()?);
    }

    publish_task.await?;
    assert_eq!(received_messages, expected_messages);
    println!("Redis pub-sub multiple messages test passed!");
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_pubsub_multiple_channels() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.subscribe("channel1").await?;
    pubsub.subscribe("channel2").await?;

    let publish_task = tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(100)).await;
      let _ = publish_with_persistence(&mut publisher, "channel1", "message_ch1").await.unwrap();
      let _ = publish_with_persistence(&mut publisher, "channel2", "message_ch2").await.unwrap();
    });

    let mut received_messages = Vec::new();
    let mut stream = pubsub.on_message();
    for _ in 0..2 {
      let message = timeout(Duration::from_secs(5), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
      received_messages.push((message.get_channel_name().to_string(), message.get_payload::<String>()?));
    }

    publish_task.await?;
    received_messages.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(received_messages[0], ("channel1".to_string(), "message_ch1".to_string()));
    assert_eq!(received_messages[1], ("channel2".to_string(), "message_ch2".to_string()));
    println!("Redis pub-sub multiple channels test passed!");
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_pubsub_pattern_subscription() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.psubscribe("test_*").await?;

    let publish_task = tokio::spawn(async move {
      tokio::time::sleep(Duration::from_millis(100)).await;
      let _ = publish_with_persistence(&mut publisher, "test_pattern1", "pattern_message1").await.unwrap();
      let _ = publish_with_persistence(&mut publisher, "test_pattern2", "pattern_message2").await.unwrap();
      let _ = publish_with_persistence(&mut publisher, "other_channel", "should_not_receive").await.unwrap();
    });

    let mut received_messages = Vec::new();
    let mut stream = pubsub.on_message();
    for _ in 0..2 {
      let message = timeout(Duration::from_secs(5), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
      received_messages.push((message.get_channel_name().to_string(), message.get_payload::<String>()?));
    }

    publish_task.await?;
    received_messages.sort_by(|a, b| a.0.cmp(&b.0));

    assert_eq!(received_messages.len(), 2);
    assert_eq!(received_messages[0], ("test_pattern1".to_string(), "pattern_message1".to_string()));
    assert_eq!(received_messages[1], ("test_pattern2".to_string(), "pattern_message2".to_string()));
    println!("Redis pub-sub pattern subscription test passed!");
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_pubsub_unsubscribe() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.subscribe("unsub_channel").await?;

    let _ = publish_with_persistence(&mut publisher, "unsub_channel", "before_unsub").await?;
    {
      let mut stream = pubsub.on_message();
      let message = timeout(Duration::from_secs(2), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
      assert_eq!(message.get_payload::<String>()?, "before_unsub");
    }

    pubsub.unsubscribe("unsub_channel").await?;

    let _ = publish_with_persistence(&mut publisher, "unsub_channel", "after_unsub").await?;

    {
      let mut stream = pubsub.on_message();
      let result = timeout(Duration::from_millis(500), stream.next()).await;
      assert!(result.is_err(), "Should not receive message after unsubscribe");
    }

    println!("Redis pub-sub unsubscribe test passed!");
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_message_persistence() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.subscribe("persist_channel").await?;

    let message_id = publish_with_persistence(&mut publisher, "persist_channel", "test_message").await?;

    let mut stream = pubsub.on_message();
    let message = timeout(Duration::from_secs(5), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
    assert_eq!(message.get_payload::<String>()?, "test_message");

    let stored_message: String = publisher.get(format!("msg:{}", message_id)).await?;
    assert_eq!(stored_message, "test_message");

    println!("Redis message persistence test passed! Message ID: {}", message_id);
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_message_idempotency() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.subscribe("idempotent_channel").await?;

    let first_id = publish_with_persistence(&mut publisher, "idempotent_channel", "duplicate_message").await?;
    let second_id = publish_with_persistence(&mut publisher, "idempotent_channel", "duplicate_message").await?;
    let third_id = publish_with_persistence(&mut publisher, "idempotent_channel", "duplicate_message").await?;

    assert_eq!(first_id, second_id);
    assert_eq!(second_id, third_id);

    let mut stream = pubsub.on_message();
    let message = timeout(Duration::from_secs(5), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
    assert_eq!(message.get_payload::<String>()?, "duplicate_message");

    let result = timeout(Duration::from_millis(500), stream.next()).await;
    assert!(result.is_err(), "Should not receive duplicate message");

    let stored_message: String = publisher.get(format!("msg:{}", first_id)).await?;
    assert_eq!(stored_message, "duplicate_message");

    println!("Redis message idempotency test passed! All attempts returned ID: {}", first_id);
    Ok(())
  }

  #[tokio::test]
  async fn test_redis_different_messages_unique_ids() -> Result<()> {
    let docker = clients::Cli::default();
    let redis_container = docker.run(Redis::default());
    let redis_url = get_redis_url(&redis_container);

    let client = redis::Client::open(redis_url.clone())?;
    let publisher_client = redis::Client::open(redis_url)?;

    let mut pubsub = client.get_async_pubsub().await?;
    let mut publisher = publisher_client.get_multiplexed_async_connection().await?;

    pubsub.subscribe("unique_channel").await?;

    let id1 = publish_with_persistence(&mut publisher, "unique_channel", "message1").await?;
    let id2 = publish_with_persistence(&mut publisher, "unique_channel", "message2").await?;
    let id3 = publish_with_persistence(&mut publisher, "unique_channel", "message3").await?;

    assert_ne!(id1, id2);
    assert_ne!(id2, id3);
    assert_ne!(id1, id3);

    let mut received_messages = Vec::new();
    let mut stream = pubsub.on_message();
    for _ in 0..3 {
      let message = timeout(Duration::from_secs(5), stream.next()).await?.ok_or_else(|| anyhow!("timeout"))?;
      received_messages.push(message.get_payload::<String>()?);
    }

    assert_eq!(received_messages, vec!["message1", "message2", "message3"]);

    println!("Redis different messages unique IDs test passed! IDs: {}, {}, {}", id1, id2, id3);
    Ok(())
  }
}
