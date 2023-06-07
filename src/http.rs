use actix_broker::{Broker, SystemBroker};
use actix_files::NamedFile;
use actix_web::{get, post, App, HttpResponse, HttpServer, Responder};
use log::info;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::MediaEngine;
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;

use crate::command;

#[get("/")]
async fn index() -> actix_web::Result<NamedFile> {
    Ok(NamedFile::open(Path::new("./public/index.html"))?)
}

// do_signaling exchanges all state of the local PeerConnection and is called
// every time a video is added or removed
async fn do_signaling(pc: &Arc<RTCPeerConnection>, sdp_str: String) -> impl Responder {
    let offer = match serde_json::from_str::<RTCSessionDescription>(&sdp_str) {
        Ok(s) => s,
        Err(err) => panic!("{}", err),
    };

    if let Err(err) = pc.set_remote_description(offer).await {
        panic!("{}", err);
    }

    // Create channel that is blocked until ICE Gathering is complete
    let mut gather_complete = pc.gathering_complete_promise().await;

    // Create an answer
    let answer = match pc.create_answer(None).await {
        Ok(answer) => answer,
        Err(err) => panic!("{}", err),
    };

    // Sets the LocalDescription, and starts our UDP listeners
    if let Err(err) = pc.set_local_description(answer).await {
        panic!("{}", err);
    }

    // Block until ICE Gathering is complete, disabling trickle ICE
    // we do this because we only can exchange one signaling message
    // in a production application you should exchange ICE Candidates via OnICECandidate
    let _ = gather_complete.recv().await;

    let payload = if let Some(local_desc) = pc.local_description().await {
        match serde_json::to_string(&local_desc) {
            Ok(p) => p,
            Err(err) => panic!("{}", err),
        }
    } else {
        panic!("generate local_description failed!");
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .body(payload)
}

#[post("/createPeerConnection")]
async fn create_peer_connection(req_body: String) -> impl Responder {
    // Create a MediaEngine object to configure the supported codec
    let mut m = MediaEngine::default();

    m.register_default_codecs().unwrap();

    // Create a InterceptorRegistry. This is the user configurable RTP/RTCP Pipeline.
    // This provides NACKs, RTCP Reports and other features. If you use `webrtc.NewPeerConnection`
    // this is enabled by default. If you are manually managing You MUST create a InterceptorRegistry
    // for each PeerConnection.
    let mut registry = Registry::new();

    // Use the default set of Interceptors
    registry = register_default_interceptors(registry, &mut m).unwrap();

    // Create the API object with the MediaEngine
    let api = APIBuilder::new()
        .with_media_engine(m)
        .with_interceptor_registry(registry)
        .build();

    // Prepare the configuration
    let config = RTCConfiguration {
        ice_servers: vec![RTCIceServer {
            urls: vec!["stun:stun.l.google.com:19302".to_owned()],
            ..Default::default()
        }],
        ..Default::default()
    };

    // Create a new RTCPeerConnection
    let pc = Arc::new(api.new_peer_connection(config).await.unwrap());
    Broker::<SystemBroker>::issue_async(command::NewPeerConnection(pc.clone()));

    // Command handling:
    let data_channel = pc.create_data_channel("control", None).await.unwrap();
    data_channel.on_message(Box::new(move |msg: DataChannelMessage| {
        let msg_str = String::from_utf8(msg.data.to_vec()).unwrap();

        let cmd: command::Command = serde_json::from_str(&msg_str.to_string()).unwrap();

        let message = match cmd {
            command::Command::Move(x, y) => command::Move(x, y),
        };

        Broker::<SystemBroker>::issue_async(message);

        Box::pin(async move {})
    }));

    info!("PeerConnection has been created");

    actix_rt::time::sleep(Duration::from_millis(2000)).await;
    do_signaling(&pc, req_body).await
}

pub async fn start(host: String, port: u16) {
    HttpServer::new(|| App::new().service(create_peer_connection).service(index))
        .bind((host, port))
        .unwrap()
        .run()
        .await
        .unwrap();
}
