use std::sync::{Arc, Mutex};

use crate::command;
use actix::prelude::*;
use actix_broker::BrokerSubscribe;
use adafruit_motorkit::{dc::DcMotor, init_pwm, Motor};
use embedded_hal::blocking::i2c::Read;
use linux_embedded_hal::I2cdev;

lazy_static! {
    static ref MOTOR_TYPE: Arc<Mutex<MotorType>> = Arc::new(Mutex::new(MotorType::None));
}

#[derive(PartialEq)]
pub enum MotorType {
    Adafruit,
    Sparkfun,
    None,
}

pub struct MotorActor;

// Provide Actor implementation for our actor
impl Actor for MotorActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Context<Self>) {
        println!("[MOTOR] Actor is alive");
        self.subscribe_system_async::<command::Move>(ctx);

        match I2cdev::new("/dev/i2c-1") {
            Ok(mut i2c) => {
                let mut motor_type = MotorType::None;

                let mut buf = [0 as u8];
                for i in [93, 96] {
                    match i2c.read(i, &mut buf) {
                        Ok(()) => {
                            if i == 93 {
                                println!("Found sparkfun motors");
                                motor_type = MotorType::Sparkfun;
                            } else if i == 96 {
                                println!("Found adafruit motors");
                                motor_type = MotorType::Adafruit;
                            }
                        }
                        Err(_) => {}
                    }
                }

                *MOTOR_TYPE.lock().unwrap() = motor_type;
            }
            Err(_) => println!("Unable to find i2c"),
        }
    }

    fn stopped(&mut self, _: &mut Context<Self>) {
        let motor_type = { MOTOR_TYPE.lock().unwrap() };

        if motor_type.eq(&MotorType::None) {
            let mut pwm = init_pwm(None).unwrap();
            let mut left_motor = DcMotor::try_new(&mut pwm, Motor::Motor1).unwrap();
            let mut right_motor = DcMotor::try_new(&mut pwm, Motor::Motor2).unwrap();
            left_motor.stop(&mut pwm).unwrap();
            right_motor.stop(&mut pwm).unwrap();
        }

        println!("[MOTOR] Actor is stopped");
    }
}

impl Handler<command::Move> for MotorActor {
    type Result = ();

    fn handle(&mut self, cmd: command::Move, _: &mut Context<Self>) -> Self::Result {
        let command::Move(x, y) = cmd;

        let multiplier = if y < 0.0 { -1.0 } else { 1.0 };
        let speed = (x.powf(2.0) + y.powf(2.0)).sqrt() * multiplier;
        let left_speed = if x > 0.0 { speed - x * 2.0 } else { speed };
        let right_speed = if x < 0.0 { speed + x * 2.0 } else { speed };

        let motor_type = { MOTOR_TYPE.lock().unwrap() };
        if motor_type.eq(&MotorType::None) {
            return;
        }

        let mut pwm = init_pwm(None).unwrap();
        let mut left_motor = DcMotor::try_new(&mut pwm, Motor::Motor1).unwrap();
        let mut right_motor = DcMotor::try_new(&mut pwm, Motor::Motor2).unwrap();
        left_motor.set_throttle(&mut pwm, left_speed).unwrap();
        right_motor.set_throttle(&mut pwm, right_speed).unwrap();
    }
}
