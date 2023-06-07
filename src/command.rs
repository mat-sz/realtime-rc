use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;
use webrtc::{
    peer_connection::RTCPeerConnection,
    track::track_local::track_local_static_sample::TrackLocalStaticSample,
};

#[derive(Clone, Serialize, Deserialize)]
pub enum Command {
    Move(f32, f32),
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct NewPeerConnection(pub Arc<RTCPeerConnection>);

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct StartVideoStream(pub Uuid, pub Arc<TrackLocalStaticSample>);

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct Move(pub f32, pub f32);
