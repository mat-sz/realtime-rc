use crate::command;
use actix::prelude::*;
use actix::Actor;
use actix_broker::BrokerSubscribe;
use bytes::Bytes;
use log::info;
use nokhwa::pixel_format::RgbFormat;
use nokhwa::utils::{ApiBackend, RequestedFormat, RequestedFormatType};
use nokhwa::{nokhwa_initialize, query, Camera};
use openh264::formats::YUVBuffer;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use tokio::task::JoinHandle;
use uuid::Uuid;
use webrtc::media::Sample;

lazy_static! {
    static ref CURRENT_FRAME: Arc<Mutex<Option<Arc<nokhwa::Buffer>>>> = Arc::new(Mutex::new(None));
}

lazy_static! {
    static ref VIDEO_TASKS: Arc<Mutex<HashMap<Uuid, JoinHandle<()>>>> =
        Arc::new(Mutex::new(HashMap::new()));
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

pub struct WebcamActor {
    pub camera_index: u32,
}

// Provide Actor implementation for our actor
impl Actor for WebcamActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        info!("Actor is alive");
        self.subscribe_system_async::<command::StartVideoStream>(ctx);
        self.subscribe_system_async::<command::StopVideoStream>(ctx);
        let camera_index = self.camera_index;

        thread::spawn(move || {
            nokhwa_initialize(|granted| {
                info!("Camera initialized: {}", granted);
            });
            let cameras = query(ApiBackend::Auto).unwrap();
            cameras
                .iter()
                .for_each(|cam| info!("Found camera: {:?}", cam));

            // request the absolute highest resolution CameraFormat that can be decoded to RGB.
            let requested = RequestedFormat::new::<RgbFormat>(RequestedFormatType::None);
            // make the camera
            let mut camera =
                Camera::new(nokhwa::utils::CameraIndex::Index(camera_index), requested).unwrap();
            camera.open_stream().unwrap();

            let frame = camera.frame().unwrap();
            *CURRENT_FRAME.lock().unwrap() = Some(Arc::new(frame));

            #[allow(clippy::empty_loop)] // keep it running
            loop {
                let count = VIDEO_TASKS.lock().unwrap().values().len();

                if count == 0 {
                    if camera.is_stream_open() {
                        info!("No more video tasks, closing stream.");
                        camera.stop_stream().unwrap();
                    }
                } else if camera.is_stream_open() {
                    let frame = camera.frame().unwrap();
                    *CURRENT_FRAME.lock().unwrap() = Some(Arc::new(frame));
                } else {
                    info!("New video task, opening stream.");
                    camera.open_stream().unwrap();
                }

                thread::sleep(Duration::from_millis(20));
            }
        });
    }

    fn stopping(&mut self, _: &mut Context<Self>) -> actix::Running {
        info!("Actor is being stopped");

        let mut tasks: std::sync::MutexGuard<HashMap<Uuid, JoinHandle<()>>> =
            VIDEO_TASKS.lock().unwrap();
        for task in tasks.values() {
            task.abort();
        }
        tasks.clear();

        actix::Running::Stop
    }
}

impl Handler<command::StartVideoStream> for WebcamActor {
    type Result = ();

    fn handle(&mut self, cmd: command::StartVideoStream, _: &mut Context<Self>) -> Self::Result {
        let command::StartVideoStream(id, video_track) = cmd;
        info!("[{id}] StartVideoStream");

        let video_task = tokio::spawn(async move {
            let frame = {
                let option = CURRENT_FRAME.lock();
                option.unwrap().clone().unwrap()
            };

            info!(
                "[{id}] Stream: camera frame {} {}",
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

            let mut interval = tokio::time::interval(Duration::from_millis(20));
            loop {
                interval.tick().await;

                let frame = {
                    let option = CURRENT_FRAME.lock();
                    option.unwrap().clone().unwrap()
                };

                let result = video_track
                    .write_sample(&Sample {
                        data: rgb_to_h264(frame, &mut h264_encoder),
                        duration: Duration::from_millis(20),
                        ..Default::default()
                    })
                    .await;

                if result.is_err() {
                    info!("[{id}] Error sending sample, exiting");
                    VIDEO_TASKS.lock().unwrap().remove(&id);
                    return;
                }
            }
        });

        {
            VIDEO_TASKS.lock().unwrap().insert(id, video_task);
        }
    }
}

impl Handler<command::StopVideoStream> for WebcamActor {
    type Result = ();

    fn handle(&mut self, cmd: command::StopVideoStream, _: &mut Context<Self>) -> Self::Result {
        let command::StopVideoStream(id) = cmd;
        info!("[{id}] StopVideoStream");

        let mut tasks = VIDEO_TASKS.lock().unwrap();
        let video_task = tasks.get(&id);

        if video_task.is_some() {
            video_task.unwrap().abort();
            tasks.remove(&id);
        }
    }
}
