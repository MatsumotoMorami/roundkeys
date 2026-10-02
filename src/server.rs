use crate::Shared;
use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;

const PAGE: &str = include_str!("../web/overlay.html");
pub fn start(shared: Shared, port: u16) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let listener =
                match tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await {
                    Ok(x) => x,
                    Err(e) => {
                        shared.lock().unwrap().server_status = format!("OBS 服务启动失败：{e}");
                        return;
                    }
                };
            shared.lock().unwrap().server_status = format!("http://127.0.0.1:{port}/overlay");
            let app = Router::new()
                .route("/overlay", get(|| async { Html(PAGE) }))
                .route("/", get(|| async { Html(PAGE) }))
                .route("/state", get(state))
                .route("/health", get(health))
                .route("/ws", get(ws))
                .with_state(shared.clone());
            if let Err(e) = axum::serve(listener, app).await {
                shared.lock().unwrap().server_status = format!("OBS 服务中断：{e}");
            }
        });
    });
}
async fn state(State(shared): State<Shared>) -> impl IntoResponse {
    axum::Json(shared.lock().unwrap().snapshot())
}
async fn health(State(shared): State<Shared>) -> impl IntoResponse {
    let m = shared.lock().unwrap();
    axum::Json(
        serde_json::json!({"input":m.input_status,"controllers":m.controller_status,"devices":m.devices,"server":m.server_status,"clients":m.clients,"last_input":m.last_input}),
    )
}
async fn ws(State(shared): State<Shared>, upgrade: WebSocketUpgrade) -> impl IntoResponse {
    upgrade.on_upgrade(move|socket| async move {
        shared.lock().unwrap().clients+=1;
        struct Client(Shared);impl Drop for Client { fn drop(&mut self){let mut m=self.0.lock().unwrap();m.clients=m.clients.saturating_sub(1);} }
        let _client=Client(Arc::clone(&shared));
        let (mut sender,mut receiver)=socket.split();
        let mut tick=tokio::time::interval(std::time::Duration::from_millis(16));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _=tick.tick()=>{
                    let data=serde_json::to_string(&shared.lock().unwrap().snapshot()).unwrap();
                    // Never let a slow/disconnected OBS client keep its sender task alive.
                    if !matches!(tokio::time::timeout(std::time::Duration::from_secs(2),sender.send(Message::Text(data.into()))).await,Ok(Ok(()))) {break;}
                }
                incoming=receiver.next()=>match incoming { Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break, _=>{} }
            }
        }
    })
}
