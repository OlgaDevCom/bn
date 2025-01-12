use warp::Filter;
use warp::ws::{Message, WebSocket};
use std::convert::Infallible;
use futures::{FutureExt, StreamExt};
use urlencoding::decode;

#[tokio::main]
async fn main() {
    // 1. Роут для віддачі HTML-сторінки (стартова сторінка).
    let html_route = warp::path::end().map(|| {
        // Повертатимемо просту HTML-сторінку з формою вибору
        // пари, типу ринку та таймфрейму, +JS для підключення WS.
        warp::reply::html(INDEX_HTML)
    });

    // 2. Роут WebSocket: /ws/{symbol}/{market_type}/{timeframe}
    let ws_route = warp::path!("ws" / String / String / String)
        .and(warp::ws())
        .map(|symbol: String, market: String, timeframe: String, ws: warp::ws::Ws| {
            // При успішному handshaking, викликається callback on_upgrade
            ws.on_upgrade(move |socket| client_ws_connection(socket, symbol, market, timeframe))
        });

    // Об’єднуємо все в один Filter.
    let routes = html_route
        .or(ws_route);

    // Піднімаємо сервер на localhost:3030
    warp::serve(routes)
        .run(([127, 0, 0, 1], 3030))
        .await;
}

/// Обробка WebSocket-з’єднання з клієнтом.
/// symbol, market, timeframe - це параметри, що вибрав користувач.
async fn client_ws_connection(ws: WebSocket, symbol: String, market: String, timeframe: String) {
    println!("New WebSocket client connected: symbol={}, market={}, timeframe={}",
        symbol, market, timeframe
    );

    // Розділимо на Sender + Receiver
    let (mut client_ws_sender, mut client_ws_rcv) = ws.split();

    // Сформуємо URL WebSocket до Binance
    // У Binance для спота:  wss://stream.binance.com:9443/ws/...
    // Для ф'ючерсів:        wss://fstream.binance.com/ws/...
    // Формат каналу:       {symbol}@kline_{timeframe}
    // Символ у Binance повинен бути в нижньому регістрі (наприклад, btcusdt).
    let symbol_lower = symbol.to_lowercase();
    let base_url = if market == "futures" {
        "wss://fstream.binance.com/ws"
    } else {
        "wss://stream.binance.com:9443/ws"
    };
    let ws_url = format!("{}/{}@kline_{}", base_url, symbol_lower, timeframe);

    println!("Connecting to Binance WebSocket: {}", ws_url);

    // Підключимось до Binance WS:
    match tokio_tungstenite::connect_async(&ws_url).await {
        Ok((mut binance_ws_stream, _response)) => {
            println!("Connected to Binance stream OK.");

            // Створимо задачу, яка читатиме повідомлення з Binance WS
            // і надсилатиме їх клієнту.
            let mut binance_ws_sender = binance_ws_stream.split().0; // Writer
            let mut binance_ws_receiver = binance_ws_stream.split().1; // Reader

            // TASK1: з Binance -> клієнт
            let forward_to_client = async move {
                while let Some(Ok(msg)) = binance_ws_receiver.next().await {
                    // Пересилаємо повідомлення WebSocket-клієнту
                    // (Якщо потрібно парсити / фільтрувати - можна зробити це тут)
                    if let Ok(txt) = msg.to_text() {
                        // Наприклад, можна перевірити / видозмінити JSON
                        // але у цьому прикладі відправляємо як є
                        if client_ws_sender
                            .send(Message::text(txt))
                            .await
                            .is_err()
                        {
                            println!("Client disconnected while sending message.");
                            break;
                        }
                    }
                }
                println!("Binance -> client loop ended.");
            };

            // TASK2: клієнт -> Binance (на випадок, якщо потрібно щось передавати)
            // У цьому демо ми просто "зливаємо" в нікуди.
            // Якщо потрібно обробляти повідомлення від клієнта:
            let read_from_client = async move {
                while let Some(Ok(msg)) = client_ws_rcv.next().await {
                    // Наприклад, клієнт може відправити stop / ping / інші команди
                    // У цьому демо просто ігноруємо або робимо ехо
                    println!("Got a message from client: {:?}", msg);

                    // Якщо потрібно передавати на Binance (напр. ордери),
                    // треба сформувати відповідне повідомлення і
                    // викликати binance_ws_sender.send(...)
                }
                println!("Client -> Binance loop ended.");
            };

            // Запустимо обидва завдання одночасно
            futures::pin_mut!(forward_to_client, read_from_client);
            futures::select! {
                _ = forward_to_client.fuse() => (),
                _ = read_from_client.fuse() => (),
            };

        }
        Err(e) => {
            eprintln!("Failed to connect to Binance WebSocket: {}", e);
            // Можна повідомити клієнта про помилку
            let _ = client_ws_sender
                .send(Message::text(format!("Error: Could not connect to Binance: {}", e)))
                .await;
        }
    }

    println!("WebSocket session ended for {}", symbol);
}

/// Статичний HTML-шаблон (спрощено) - стартова сторінка.
static INDEX_HTML: &str = r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8"/>
    <title>Binance WebSocket Rust Demo</title>
</head>
<body>
    <h1>Binance WebSocket (Rust + Warp)</h1>
    <p>Виберіть торгову пару, тип ринку та таймфрейм:</p>
    <form id="pair-form">
        <label>Пара:
            <select name="symbol">
                <option value="BTCUSDT">BTCUSDT</option>
                <option value="ETHUSDT">ETHUSDT</option>
                <option value="BNBUSDT">BNBUSDT</option>
            </select>
        </label>
        <br/><br/>
        <label>Тип ринку:
            <select name="market">
                <option value="spot">Спот</option>
                <option value="futures">Ф'ючерси</option>
            </select>
        </label>
        <br/><br/>
        <label>Таймфрейм:
            <select name="timeframe">
                <option value="1m">1 хв</option>
                <option value="5m">5 хв</option>
                <option value="15m">15 хв</option>
                <option value="1h">1 год</option>
                <option value="4h">4 год</option>
                <option value="1d">1 день</option>
            </select>
        </label>
        <br/><br/>
        <button type="submit">Запустити WebSocket</button>
    </form>

    <hr/>
    <div id="status">Не підключено</div>
    <pre id="messages" style="border:1px solid #ccc; width: 600px; height:300px; overflow:auto;"></pre>

    <script>
    const form = document.getElementById('pair-form');
    const statusDiv = document.getElementById('status');
    const messagesPre = document.getElementById('messages');
    let ws;

    form.addEventListener('submit', function(e) {
        e.preventDefault();

        // Якщо вже є відкрите з’єднання - закриємо
        if(ws) {
            ws.close();
        }

        const symbol = form.symbol.value;     // BTCUSDT, ETHUSDT...
        const market = form.market.value;     // spot / futures
        const timeframe = form.timeframe.value; // 1m, 5m...

        // Формуємо URL WS: /ws/{symbol}/{market}/{timeframe}
        // Наприклад: ws://localhost:3030/ws/BTCUSDT/spot/1m
        const wsUrl = `ws://${location.host}/ws/${symbol}/${market}/${timeframe}`;
        ws = new WebSocket(wsUrl);

        ws.onopen = () => {
            statusDiv.innerText = "WebSocket підключено: " + wsUrl;
            messagesPre.textContent = "";
        };
        ws.onmessage = (msg) => {
            // Тут приходять сирі дані з Binance (у форматі JSON).
            // Можна їх розпарсити та оновити графік.
            messagesPre.textContent += msg.data + "\n";
        };
        ws.onerror = (err) => {
            statusDiv.innerText = "Помилка: " + err;
        };
        ws.onclose = () => {
            statusDiv.innerText = "WebSocket закрито";
        };
    });
    </script>
</body>
</html>
"#;
