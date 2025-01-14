// use kraken_async_rs::test_support::set_up_logging;
// use kraken_async_rs::wss::{KrakenWSSClient, Message as KrakenMessage, OhlcSubscription, WssMessage};
// use tokio::time::Duration;
// use tokio_stream::StreamExt;
// use tracing::{info, warn};

// #[tokio::main]
// async fn main() {
//     set_up_logging("wss_ohlc.log");

//     let mut client = KrakenWSSClient::new_with_tracing(
//         "wss://ws.kraken.com",
//         "wss://ws-auth.kraken.com",
//         true,
//         true,
//     );
//     let mut kraken_stream = client.connect::<WssMessage>().await.unwrap();

//     let ohlc_params = OhlcSubscription::new(vec!["XBT/USD".into()], 1);
//     let subscription = KrakenMessage::new_subscription(ohlc_params, 0);

//     let result = kraken_stream.send(&subscription).await;
//     assert!(result.is_ok(), "Failed to subscribe: {:?}", result);

//     while let Ok(Some(message)) = tokio::time::timeout(Duration::from_secs(10), kraken_stream.next()).await {
//         match message {
//             Ok(WssMessage::Channel(channel)) => {
//                 info!("Received channel message: {:?}", channel);
//             }
//             Err(e) => {
//                 warn!("Error receiving message: {:?}", e);
//             }
//             _ => {}
//         }
//     }
// }
