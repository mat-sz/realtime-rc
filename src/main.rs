use crate::motor::MotorActor;
use crate::webcam::WebcamActor;
use actix::Actor;
use colored::Colorize;
use env_logger::Env;
use local_ip_address::local_ip;

mod command;
mod http;
mod motor;
mod webcam;

#[macro_use]
extern crate lazy_static;

#[actix_web::main]
async fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    println!();
    println!("{}", "realtime-rc".green().bold());

    WebcamActor.start();
    MotorActor.start();

    println!("Press {} to stop", "Ctrl-C".bold());
    println!();
    println!("You can view the control page in your browser.");

    let my_local_ip = local_ip().unwrap();
    println!("{} http://localhost:8080", "Local:".bold());
    println!("{} http://{}:8080", "On your network:".bold(), my_local_ip);

    println!();

    http::start().await;
}
