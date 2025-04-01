use axum::{routing::get, serve, Router};
use hyper::header::HeaderMap;
use redb::{Database, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};
use std::fs;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::{net::TcpListener, sync::mpsc};

mod lib {
  pub mod http;
}
use crate::lib::http;

static GLOBALS: TableDefinition<u64, u64> = TableDefinition::new("globals");

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
enum JSONResponse {
  Point { x: i32, y: i32 },
  Message { text: String },
  Counters { counter_runs: u64, counter_requests: u64 },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  fs::create_dir_all(&Path::new("./.db"))?;
  let redb = Database::create("./.db/demo.redb")?;
  run_main(&redb, inc_counter(&redb).await?).await;
  Ok(())
}

async fn inc_counter(redb: &Database) -> Result<u64, Box<dyn std::error::Error>> {
  let mut counter_runs: u64 = 0;
  let txn = redb.begin_write()?;
  {
    let mut table = txn.open_table(GLOBALS)?;
    if let Some(value) = table.get(&1)? {
      counter_runs = value.value();
    }
    counter_runs += 1;
    println!("Run counter in the DB: {}", counter_runs);
    table.insert(&1, &counter_runs)?;
  }
  txn.commit()?;
  Ok(counter_runs)
}

async fn run_main(_redb: &Database, counter_runs: u64) {
  let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

  let counter_runs = Arc::new(counter_runs);

  let app = Router::new()
    .route("/healthz", get(|| async { "OK\n" }))
    .route("/", get(|| async { "hello this is a rust http server\n" }))
    .route(
      "/quit",
      get({
        let shutdown_tx = shutdown_tx.clone();
        || async move {
          let _ = shutdown_tx.send(()).await;
          "yes i am shutting down\n"
        }
      }),
    )
    .route(
      "/json",
      get(|headers: HeaderMap| async move {
        let counter_runs = *counter_runs;
        let cnt_requests = 42; // NOT IMPLEMENTED YET
        let response = JSONResponse::Counters { counter_runs, counter_requests: cnt_requests };
        let json_string = serde_json::to_string(&response).unwrap();
        http::json_or_html(headers, &json_string).await
      }),
    );

  let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
  let listener = TcpListener::bind(addr).await.unwrap();

  println!("rust http server ready on {}", addr);

  let server = serve(listener, app);

  let mut term_signal = signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
  let mut int_signal = signal(SignalKind::interrupt()).expect("failed to register SIGINT handler");

  tokio::select! {
    _ = server.with_graceful_shutdown(async move { shutdown_rx.recv().await; }) => { println! ("done"); }
    _ = tokio::signal::ctrl_c() => { println!("terminating due to Ctrl+C"); }
    _ = term_signal.recv() => { println!("terminating due to SIGTERM"); }
    _ = int_signal.recv() => { println!("terminating due to SIGINT"); }
  }

  println!("rust http server down");
}
