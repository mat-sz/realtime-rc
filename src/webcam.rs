use crate::command;
use actix::prelude::*;
use actix::Actor;
use actix_broker::BrokerSubscribe;
use bytes::Bytes;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, RequestedFormat, RequestedFormatType};
use nokhwa::{nokhwa_initialize, query, Camera};
use openh264::formats::YUVBuffer;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tokio::sync::Notify;
use webrtc::api::media_engine::MIME_TYPE_H264;
use webrtc::media::Sample;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;
use webrtc::track::track_local::TrackLocal;

lazy_static! {
    static ref CURRENT_FRAME: Arc<Mutex<Option<Arc<nokhwa::Buffer>>>> = Arc::new(Mutex::new(None));
}

fn rgb_to_h264(frame: Arc<nokhwa::Buffer>, encoder: &mut openh264::encoder::Encoder) -> Bytes {
    let decoded = frame.decode_image::<RgbFormat>().unwrap();

    let yuv_buffer = YUVBuffer::with_rgb(
        frame.resolution().width() as usize,
        frame.resolution().height() as usize,
        decoded.as_raw(),
    );

    let h264 = encoder.encode(&yuv_buffer).unwrap();
    Bytes::from(h264.to_vec())
}

pub struct WebcamActor;

// Provide Actor implementation for our actor
impl Actor for WebcamActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("[WEBCAM] Actor is alive");
        self.subscribe_system_async::<command::NewPeerConnection>(ctx);

        thread::spawn(|| {
            nokhwa_initialize(|granted| {
                println!("Camera initialized: {}", granted);
            });
            let cameras = query(ApiBackend::Auto).unwrap();
            cameras.iter().for_each(|cam| println!("{:?}", cam));

            let first_camera = cameras.last().unwrap();

            // request the absolute highest resolution CameraFormat that can be decoded to RGB.
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
            // make the camera
            let mut camera = Camera::new(first_camera.index().clone(), requested).unwrap();
            camera.open_stream().unwrap();

            #[allow(clippy::empty_loop)] // keep it running
            loop {
                // get a frame
                match camera.frame() {
                    Ok(frame) => {
                        *CURRENT_FRAME.lock().unwrap() = Some(Arc::new(frame));
                    }
                    Err(e) => {
                        println!("error {e}");
                    }
                }

                thread::sleep(Duration::from_millis(10));
            }
        });
    }

    fn stopped(&mut self, _: &mut Context<Self>) {
        println!("[WEBCAM] Actor is stopped");
    }
}

impl Handler<command::NewPeerConnection> for WebcamActor {
    type Result = ();

    fn handle(&mut self, cmd: command::NewPeerConnection, _: &mut Context<Self>) -> Self::Result {
        let command::NewPeerConnection(pc) = cmd;
        println!("NewPeerConnection");

        actix_rt::spawn(async move {
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

            let notify_tx = Arc::new(Notify::new());
            let notify_video = notify_tx.clone();

            // Video handling:
            let video_task = actix_rt::spawn(async move {
                notify_video.notified().await;
                actix_rt::time::sleep(Duration::from_millis(2500)).await;

                let frame = {
                    let option = CURRENT_FRAME.lock();
                    option.unwrap().clone().unwrap()
                };

                println!(
                    "camera frame {} {}",
                    frame.resolution().width(),
                    frame.resolution().height()
                );

                let h264_encoder_config = openh264::encoder::EncoderConfig::new(
                    frame.resolution().width(),
                    frame.resolution().height(),
                );
                h264_encoder_config.enable_skip_frame(true);
                h264_encoder_config.max_frame_rate(30.0);
                let mut h264_encoder =
                    openh264::encoder::Encoder::with_config(h264_encoder_config).unwrap();

                loop {
                    actix_rt::time::sleep(Duration::from_millis(33)).await;

                    let frame = {
                        let option = CURRENT_FRAME.lock();
                        option.unwrap().clone().unwrap()
                    };

                    video_track
                        .write_sample(&Sample {
                            data: rgb_to_h264(frame, &mut h264_encoder),
                            duration: Duration::from_millis(33),
                            ..Default::default()
                        })
                        .await
                        .unwrap();
                }
            });

            actix_rt::spawn(async move {
                let mut rtcp_buf = vec![0u8; 1500];
                while let Ok((_, _)) = rtp_sender.read(&mut rtcp_buf).await {}
            });

            pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
                if s == RTCPeerConnectionState::Connected {
                    println!("Status connected, starting stream");
                    notify_tx.notify_waiters();
                }

                if s == RTCPeerConnectionState::Disconnected {
                    video_task.abort();
                }
                Box::pin(async {})
            }));
        });
    }
}
