use warp::{Filter, ws::WebSocket};
use tokio::sync::broadcast;
use futures_util::{StreamExt, SinkExt};
use serde_json::json;

// Структура для передачі повідомлень через WebSocket
#[derive(Debug, Clone)]
struct WsMessage {
    message_type: String,
    data: serde_json::Value,
}

// Створення маршруту WebSocket
pub fn routes() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    // Створюємо broadcast-канал для надсилання повідомлень усім клієнтам
    let (tx, _) = broadcast::channel(100);

    // Обробка WebSocket з'єднань
    warp::path("ws")
        .and(warp::ws())
        .and(warp::any().map(move || tx.subscribe()))
        .map(|ws: warp::ws::Ws, rx| ws.on_upgrade(move |socket| handle_ws(socket, rx)))
}

// Обробка WebSocket клієнта
async fn handle_ws(ws: WebSocket, mut rx: broadcast::Receiver<WsMessage>) {
    let (mut tx, mut rx_ws) = ws.split();

    // Читання повідомлень від клієнта (якщо потрібно)
    tokio::spawn(async move {
        while let Some(Ok(message)) = rx_ws.next().await {
            if let Ok(text) = message.to_str() {
                println!("Отримано повідомлення від клієнта: {}", text);
                // Можна додати обробку вхідних повідомлень
            }
        }
    });

    // Відправка повідомлень клієнту
    while let Ok(msg) = rx.recv().await {
        let message = json!({
            "type": msg.message_type,
            "data": msg.data
        });

        if let Err(e) = tx.send(warp::ws::Message::text(message.to_string())).await {
            eprintln!("Помилка відправки через WebSocket: {:?}", e);
            break;
        }
    }
}
