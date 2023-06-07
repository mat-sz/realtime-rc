use crate::motor::MotorActor;
use crate::webcam::WebcamActor;
use actix::Actor;
use colored::Colorize;
use env_logger::Env;
use gumdrop::Options;
use local_ip_address::local_ip;
use nokhwa::{nokhwa_initialize, query, utils::ApiBackend};
use tokio::signal;

mod command;
mod http;
mod motor;
mod webcam;

#[macro_use]
extern crate lazy_static;

#[derive(Options)]
struct Args {
    #[options(help = "camera index", meta = "INDEX", default = "0")]
    camera_index: u32,

    #[options(help = "list cameras", default = "false")]
    list_cameras: bool,

    #[options(no_short, help = "ip/hostname to bind to", default = "0.0.0.0")]
    host: String,

    #[options(no_short, help = "ip/hostname to bind to", default = "8080")]
    port: u16,

    #[options(help_flag, help = "view help", default = "false")]
    help: bool,
}

#[actix_rt::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse_args_default_or_exit();

    if args.list_cameras {
        nokhwa_initialize(|_| {});
        let mut cameras = query(ApiBackend::Auto).unwrap();
        cameras.sort_by(|a, b| {
            a.index()
                .as_index()
                .unwrap()
                .cmp(&b.index().as_index().unwrap())
        });

        cameras.iter().for_each(|cam| {
            println!(
                "--camera-index {} | {}, {}",
                cam.index().as_index().unwrap(),
                cam.human_name(),
                cam.description()
            )
        });

        return;
    }

    println!();
    println!("{}", "realtime-rc".green().bold());

    (WebcamActor {
        camera_index: args.camera_index,
    })
    .start();
    MotorActor.start();

    println!("Press {} to stop", "Ctrl-C".bold());
    println!();
    println!("You can view the control page in your browser.");

    if args.host.eq("0.0.0.0") {
        let my_local_ip = local_ip().unwrap();
        println!("{} http://localhost:{}", "Local:".bold(), args.port);
        println!(
            "{} http://{}:{}",
            "On your network:".bold(),
            my_local_ip,
            args.port
        );
    } else {
        println!("{} http://{}:{}/", "Visit:".bold(), args.host, args.port);
    }

    println!();

    actix_rt::spawn(async move {
        http::start(args.host, args.port).await;
    });

    signal::ctrl_c().await.unwrap();

    println!("Exiting...");
}
