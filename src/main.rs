use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use binance::{websockets::WebSockets, ws_model::WebsocketEvent};
use serde::Serialize;
use warp::{Filter, ws::{Message, WebSocket}};
use futures_util::{StreamExt, SinkExt};
use tokio::sync::broadcast;
use serde_json::json;

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
    env_logger::init();
    // println!("Запуск сервера...");

    let shared_data: SharedData = Arc::new(Mutex::new(HeatmapData {
        bids: vec![],
        asks: vec![],
        spread_history: Vec::new(),
        volume_history: Vec::new(),
    }));

    let keep_running = Arc::new(AtomicBool::new(true));
    let keep_running_ws = keep_running.clone();

    let (tx, _) = broadcast::channel(100);
    let shared_data_ws = shared_data.clone();
    let tx_ws = tx.clone();

    tokio::spawn(async move {
        let mut web_socket: WebSockets<'_, WebsocketEvent> = WebSockets::new(move |event: WebsocketEvent| {
            if let WebsocketEvent::DepthOrderBook(depth) = event {
                let mut data = shared_data_ws.lock().unwrap();

                // Оновлення заявок
                data.bids = depth.bids.iter().map(|b| (b.price, b.qty)).collect();
                data.asks = depth.asks.iter().map(|a| (a.price, a.qty)).collect();

                // Розрахунок спреду
                if let (Some(best_bid), Some(best_ask)) = (data.bids.first(), data.asks.first()) {
                    let spread = best_ask.0 - best_bid.0;
                    println!("Spread: {:.2} {} {}", spread, best_bid.0, best_ask.0);
                    let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
                    data.spread_history.push((timestamp.clone(), spread));

                    // Розрахунок загального обсягу bids та asks
                    let total_bids: f64 = data.bids.iter().map(|(_, qty)| qty).sum();
                    let total_asks: f64 = data.asks.iter().map(|(_, qty)| qty).sum();
                    data.volume_history.push((timestamp, total_bids, total_asks));

                    // Обмеження довжини історії
                    if data.spread_history.len() > 1000 {
                        data.spread_history.remove(0);
                    }
                    if data.volume_history.len() > 1000 {
                        data.volume_history.remove(0);
                    }
                }

                // Надсилання оновлень
                let _ = tx_ws.send(data.clone());
            }
            Ok(())
        });

        if let Err(e) = web_socket.connect("solusdt@depth@100ms").await {
            eprintln!("Помилка підключення до WebSocket: {:?}", e);
            return;
        }

        if let Err(e) = web_socket.event_loop(&keep_running_ws).await {
            eprintln!("Помилка в циклі WebSocket: {:?}", e);
        }

        if let Err(e) = web_socket.disconnect().await {
            eprintln!("Помилка при закритті WebSocket: {:?}", e);
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

    let static_route = warp::fs::dir("./static");

    let cors = warp::cors()
    .allow_any_origin()
    .allow_header("content-type")
    .allow_methods(vec!["GET", "POST", "DELETE", "PUT"]);

    println!("HTTP сервер запущено на http://0.0.0.0:8080");
    warp::serve(data_route.or(ws_route).or(static_route).with(cors))
    .run(([0, 0, 0, 0], 8080))
    .await;

    keep_running.store(false, Ordering::SeqCst);
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
