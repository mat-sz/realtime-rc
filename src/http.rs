use actix_broker::{Broker, SystemBroker};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use log::info;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MediaEngine, MIME_TYPE_H264};
use webrtc::api::APIBuilder;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::ice_transport::ice_server::RTCIceServer;
use webrtc::interceptor::registry::Registry;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

use crate::command;

static INDEX_FILE: &'static str = include_str!("../public/index.html");

async fn index() -> Result<Response<Body>, hyper::Error> {
    Ok(Response::new(Body::from(INDEX_FILE)))
}

// do_signaling exchanges all state of the local PeerConnection and is called
// every time a video is added or removed
async fn do_signaling(
    pc: &Arc<RTCPeerConnection>,
    r: Request<Body>,
) -> Result<Response<Body>, hyper::Error> {
    let sdp_str = match std::str::from_utf8(&hyper::body::to_bytes(r.into_body()).await?) {
        Ok(s) => s.to_owned(),
        Err(err) => panic!("{}", err),
    };

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

    let mut response = match Response::builder()
        .header("content-type", "application/json")
        .body(Body::from(payload))
    {
        Ok(res) => res,
        Err(err) => panic!("{}", err),
    };

    *response.status_mut() = StatusCode::OK;
    Ok(response)
}

async fn create_peer_connection(r: Request<Body>) -> Result<Response<Body>, hyper::Error> {
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

    let id = Uuid::new_v4();

    // Create a new RTCPeerConnection
    let pc = Arc::new(api.new_peer_connection(config).await.unwrap());

    info!("[{id}] New connection");

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

    // Video track:
    let video_track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: MIME_TYPE_H264.to_owned(),
            ..Default::default()
        },
        "video".to_owned(),
        "webrtc-rs".to_owned(),
    ));

    // Add this newly created track to the PeerConnection
    let rtp_sender = pc
        .add_track(Arc::clone(&video_track) as Arc<dyn TrackLocal + Send + Sync>)
        .await
        .unwrap();

    tokio::spawn(async move {
        let mut rtcp_buf = vec![0u8; 1500];
        while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
    });

    pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        if s == RTCPeerConnectionState::Connected {
            info!("[{id}] Connected, starting stream");
            Broker::<SystemBroker>::issue_async(command::StartVideoStream(id, video_track.clone()));
        }

        if s == RTCPeerConnectionState::Disconnected {
            info!("[{id}] Disconnected, stopping stream");
            Broker::<SystemBroker>::issue_async(command::StopVideoStream(id));
        }
        Box::pin(async {})
    }));

    do_signaling(&pc, r).await
}

async fn remote_handler(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/") | (&Method::GET, "/index.html") => index().await,

        (&Method::POST, "/createPeerConnection") => create_peer_connection(req).await,

        // Return the 404 Not Found for other routes.
        _ => {
            let mut not_found = Response::default();
            *not_found.status_mut() = StatusCode::NOT_FOUND;
            Ok(not_found)
        }
    }
}

pub async fn start(host: String, port: u16) {
    let addr = SocketAddr::new(IpAddr::from_str(&host).unwrap(), port);
    let service = make_service_fn(|_| async { Ok::<_, hyper::Error>(service_fn(remote_handler)) });
    let server = Server::bind(&addr).serve(service);

    if let Err(e) = server.await {
        eprintln!("server error: {e}");
    }
}
