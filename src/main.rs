use actix::Actor;

use crate::motor::MotorActor;
use crate::webcam::WebcamActor;

mod command;
mod http;
mod motor;
mod webcam;

#[macro_use]
extern crate lazy_static;

#[actix_web::main]
async fn main() {
    println!("realtime-rc");

    WebcamActor.start();
    MotorActor.start();

    println!("Press ctrl-c to stop");
    println!("Starting server at: http://0.0.0.0:8080/");
    http::start().await;
}
