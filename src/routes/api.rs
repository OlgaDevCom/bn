use warp::{Filter, ws::Message, ws::WebSocket};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use tokio::sync::broadcast;
use futures_util::{StreamExt, SinkExt};

// Тип для зберігання даних
#[derive(Serialize, Deserialize, Clone, Debug)]
struct Item {
    id: u64,
    name: String,
    value: String,
}

// Спільний стан для зберігання даних
type Db = Arc<Mutex<HashMap<u64, Item>>>;

pub fn routes() -> impl Filter<Extract = impl warp::Reply, Error = warp::Rejection> + Clone {
    let db = Arc::new(Mutex::new(HashMap::new()));
    let (tx, _) = broadcast::channel::<Item>(100);

    // Маршрут для API
    let get_item = warp::path!("api" / "items" / u64)
        .and(warp::get())
        .and(with_db(db.clone()))
        .map(|id: u64, db: Db| {
            let db = db.lock().unwrap();
            if let Some(item) = db.get(&id) {
                warp::reply::json(item)
            } else {
                warp::reply::json(&json!({"error": "Item not found"}))
            }
        });

    let create_item = warp::path!("api" / "items")
        .and(warp::post())
        .and(warp::body::json())
        .and(with_db(db.clone()))
        .and(with_broadcast(tx.clone()))
        .map(|new_item: Item, db: Db, tx: broadcast::Sender<Item>| {
            let mut db = db.lock().unwrap();
            db.insert(new_item.id, new_item.clone());
            let _ = tx.send(new_item.clone());
            warp::reply::json(&new_item)
        });

    let update_item = warp::path!("api" / "items" / u64)
        .and(warp::put())
        .and(warp::body::json())
        .and(with_db(db.clone()))
        .map(|id: u64, updated_item: Item, db: Db| {
            let mut db = db.lock().unwrap();
            if db.contains_key(&id) {
                db.insert(id, updated_item.clone());
                warp::reply::json(&updated_item)
            } else {
                warp::reply::json(&json!({"error": "Item not found"}))
            }
        });

    let delete_item = warp::path!("api" / "items" / u64)
        .and(warp::delete())
        .and(with_db(db.clone()))
        .map(|id: u64, db: Db| {
            let mut db = db.lock().unwrap();
            if db.remove(&id).is_some() {
                warp::reply::json(&json!({"message": "Item deleted"}))
            } else {
                warp::reply::json(&json!({"error": "Item not found"}))
            }
        });

    let ws_route = warp::path("ws")
        .and(warp::ws())
        .and(with_broadcast(tx.clone()))
        .map(|ws: warp::ws::Ws, tx: broadcast::Sender<Item>| {
            ws.on_upgrade(move |socket| handle_ws(socket, tx.subscribe()))
        });

    get_item
        .or(create_item)
        .or(update_item)
        .or(delete_item)
        .or(ws_route)
}

// Функція для передачі стану через маршрути
fn with_db(db: Db) -> impl Filter<Extract = (Db,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || db.clone())
}

// Функція для передачі каналу broadcast через маршрути
fn with_broadcast(
    tx: broadcast::Sender<Item>,
) -> impl Filter<Extract = (broadcast::Sender<Item>,), Error = std::convert::Infallible> + Clone {
    warp::any().map(move || tx.clone())
}

// Обробка WebSocket клієнта
async fn handle_ws(ws: WebSocket, mut rx: broadcast::Receiver<Item>) {
    let (mut tx, _) = ws.split();

    while let Ok(item) = rx.recv().await {
        let message = json!({
            "id": item.id,
            "name": item.name,
            "value": item.value
        });

        if let Err(e) = tx.send(Message::text(message.to_string())).await {
            eprintln!("Помилка відправки через WebSocket: {:?}", e);
            break;
        }
    }
}
