#[cfg(test)]
mod tests {
  use anyhow::Result;
  use redis::AsyncCommands;
  use testcontainers::{Container, clients};
  use testcontainers_modules::redis::Redis;

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
}
