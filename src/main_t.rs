use kraken_async_rs::wss::{KrakenWSSClient, Message as KrakenMessage, WssMessage, BookSubscription};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use warp::{Filter, ws::{Message, WebSocket}};
use tracing::{info, warn};

#[derive(Serialize, Debug, Clone)]
struct HeatmapData {
    bids: Vec<(f64, f64)>,          // (ціна, обсяг)
    asks: Vec<(f64, f64)>,          // (ціна, обсяг)
    spread_history: Vec<(String, f64)>, // (час, спред)
    volume_history: Vec<(String, f64, f64)>, // (час, загальний обсяг bids, загальний обсяг asks)
}

type SharedData = Arc<Mutex<HeatmapData>>;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let shared_data: SharedData = Arc::new(Mutex::new(HeatmapData {
        bids: vec![],
        asks: vec![],
        spread_history: Vec::new(),
        volume_history: Vec::new(),
    }));

    let (tx, _) = broadcast::channel(100);
    let shared_data_ws = shared_data.clone();

    tokio::spawn(async move {
        let mut client = KrakenWSSClient::new();
        let mut kraken_stream = client.connect::<WssMessage>().await.unwrap();

        let book_params = BookSubscription::new(vec!["XBT/USD".to_string()]);
        let subscription = KrakenMessage::new_subscription(book_params, 0);

        if let Err(e) = kraken_stream.send(&subscription).await {
            eprintln!("Failed to subscribe: {:?}", e);
            return;
        }

        while let Some(Ok(message)) = kraken_stream.next().await {
            if let WssMessage::Channel(channel_message) = message {
                match channel_message {
                    kraken_async_rs::wss::ChannelMessage::Orderbook(orderbook) => {
                        let mut data = shared_data_ws.lock().unwrap();
                        data.bids = orderbook.data.bids.iter().map(|b| (b.price, b.qty)).collect();
                        data.asks = orderbook.data.asks.iter().map(|a| (a.price, a.qty)).collect();

                        if let (Some(best_bid), Some(best_ask)) = (data.bids.first(), data.asks.first()) {
                            let spread = best_ask.0 - best_bid.0;
                            let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
                            data.spread_history.push((timestamp.clone(), spread));

                            let total_bids: f64 = data.bids.iter().map(|(_, qty)| qty).sum();
                            let total_asks: f64 = data.asks.iter().map(|(_, qty)| qty).sum();
                            data.volume_history.push((timestamp, total_bids, total_asks));

                            if data.spread_history.len() > 1000 {
                                data.spread_history.remove(0);
                            }
                            if data.volume_history.len() > 1000 {
                                data.volume_history.remove(0);
                            }
                        }

                        let _ = tx.send(data.clone());
                    }
                    _ => {}
                }
            }
        }
    });

    let data_route = warp::path("data")
        .and(warp::get())
        .and(warp::any().map(move || shared_data.clone()))
        .map(|shared_data: SharedData| {
            let data = shared_data.lock().unwrap();
            warp::reply::json(&*data)
        });

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(warp::any().map(move || tx.subscribe()))
        .map(|ws: warp::ws::Ws, rx| {
            ws.on_upgrade(move |socket| handle_ws(socket, rx))
        });

    let cors = warp::cors()
        .allow_any_origin()
        .allow_header("content-type")
        .allow_methods(vec!["GET", "POST", "DELETE", "PUT"]);

    println!("HTTP сервер запущено на http://0.0.0.0:8080");
    warp::serve(data_route.or(ws_route).with(cors))
        .run(([0, 0, 0, 0], 8080))
        .await;
}

async fn handle_ws(ws: WebSocket, mut rx: broadcast::Receiver<HeatmapData>) {
    let (mut tx, _rx) = ws.split();

    while let Ok(data) = rx.recv().await {
        let message = json!({
            "bids": data.bids,
            "asks": data.asks,
            "spread_history": data.spread_history,
            "volume_history": data.volume_history
        });

        if let Err(e) = tx.send(Message::text(message.to_string())).await {
            eprintln!("Помилка відправки через WebSocket: {:?}", e);
            break;
        }
    }
}
