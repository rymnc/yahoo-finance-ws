use clap::{App, Arg};
use futures::sink::SinkExt;
use futures::stream::{SplitSink, StreamExt};
use serde_json::json;
use std::io::Write;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Receiver, Sender};
use tokio_native_tls::TlsStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::stream::Stream;
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::WebSocketStream;

pub type YResponse = Option<std::result::Result<Message, Error>>;
pub type Socket = SplitSink<WebSocketStream<Stream<TcpStream, TlsStream<TcpStream>>>, Message>;
pub type YWebSocket = WebSocketStream<Stream<TcpStream, TlsStream<TcpStream>>>;

#[allow(irrefutable_let_patterns)]
async fn enable_writing(mut rx_file: Receiver<String>, ticker: String, dir: &str) {
    let path = format!("./{}/", dir);
    std::fs::create_dir_all(path.clone()).unwrap();
    let mut file =
        std::fs::File::create(format!("{}/{}.dat", path, ticker)).expect("File Creation Failed");
    while let msg = rx_file.recv().await {
        match msg {
            Some(msg) => writeln!(file, "{}", &msg).expect("[ERROR] Unable to Write to File"),
            None => panic!("[ERROR] Unexpected Error"),
        }
    }
}

async fn match_and_dump(res: YResponse, tx_file: &mut Sender<String>, tx: &mut Socket) {
    match res {
        Some(res) => match res {
            Ok(msg) => tx_file.send(msg.to_string()).await.unwrap(),
            Err(e) => {
                tx_file.send(format!("[ERROR] {}", e)).await.unwrap();
                tx.close().await.unwrap();
            }
        },
        None => panic!("Connection Dropped"),
    }
}

#[allow(irrefutable_let_patterns)]
async fn receive_stream<'a>(ws_stream: YWebSocket, ticker: String, directory: String) {
    let (mut tx_file, rx_file) = tokio::sync::mpsc::channel(512);
    let temp = ticker.clone();
    tokio::spawn(async move {
        enable_writing(rx_file, temp, &directory).await;
    });
    let (mut tx, mut rx) = ws_stream.split();
    let payload = json!({ "subscribe": [ticker] });
    tx.send(Message::Text(payload.to_string())).await.unwrap();
    while let res = rx.next().await {
        match_and_dump(res, &mut tx_file, &mut tx).await;
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let matches = App::new("Yahoo Finance Data Stream")
        .version("1.0")
        .author("rymnc <aaryamannchallani7@gmail.com>")
        .about("Attaches to Yahoo Finance's websocket stream and dumps data to csv")
        .arg(
            Arg::new("tickers")
                .short('t')
                .long("tickers")
                .value_name("TICKER_1, TICKER_2, ...TICKER_N")
                .about("Comma seperated list of tickers")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::new("directory")
                .short('d')
                .long("directory")
                .value_name("DIRECTORY")
                .about("Directory to dump files")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::new("quiet")
                .short('q')
                .long("quiet")
                .about("Set Logging"),
        )
        .get_matches();
    let _directory: String = matches.value_of("directory").unwrap().to_string();
    let _quiet: bool = matches.is_present("quiet");
    let tickers: Vec<String> = matches
        .value_of("tickers")
        .unwrap()
        .split(',')
        .map(str::to_string)
        .collect();
    let mut handles = Vec::new();
    for ticker in tickers {
        let directory = _directory.clone();
        handles.push(tokio::spawn(async move {
            let (ws_stream, _) = connect_async("wss://streamer.finance.yahoo.com/")
                .await
                .expect("WebSocket Handshake Failed");
            receive_stream(ws_stream, ticker, directory).await;
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    Ok(())
}
