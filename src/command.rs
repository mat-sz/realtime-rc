use actix::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use webrtc::peer_connection::RTCPeerConnection;

#[derive(Clone, Serialize, Deserialize)]
pub enum Command {
    Move(f32, f32),
}

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct NewPeerConnection(pub Arc<RTCPeerConnection>);

#[derive(Clone, Message)]
#[rtype(result = "()")]
pub struct Move(pub f32, pub f32);
