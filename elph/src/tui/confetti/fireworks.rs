use std::f64::consts::PI;

use iocraft::prelude::Color;
use rand::RngExt;

use super::array;
use super::physics::SIMULATION_FPS;
use super::simulation::{Particle, Point, Vector};

const NUM_PARTICLES: usize = 72;

const COLORS: &[&str] = &[
    "#a864fd", "#29cdff", "#78ff44", "#ff718d", "#fdff6a", "#ff9f43", "#ff6bcb", "#feca57",
];
const CHARACTERS: &[&str] = &["+", "*", "•", "✦", "★", "●"];
const HEAD: &str = "▄";
const TAIL: &str = "│";

fn parse_color(hex: &str) -> Color {
    if hex.starts_with('#') && hex.len() == 7 {
        let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(255);
        let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(255);
        let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(255);
        Color::Rgb { r, g, b }
    } else {
        Color::White
    }
}

pub fn spawn_shoot(width: i32, height: i32) -> Particle {
    let mut rng = rand::rng();
    let color = parse_color(array::sample(COLORS));
    let v = rng.random_range(28..52) as f64;
    let x = rng.random::<f64>() * width.max(1) as f64;

    let position = Point {
        x,
        y: height as f64,
        z: 0.0,
    };

    let velocity = Vector { x: 0.0, y: -v, z: 0.0 };

    let mut particle = Particle::new(HEAD.to_string(), color, position, velocity, SIMULATION_FPS);
    particle.tail_char = TAIL.to_string();
    particle.shooting = true;
    particle.explosion_call = Some(spawn_explosion);
    particle
}

pub fn spawn_explosion(color: Color, x: f64, y: f64, _width: i32, _height: i32) -> Vec<Particle> {
    let mut rng = rand::rng();
    let v = rng.random_range(32..48) as f64;
    let mut particles = Vec::with_capacity(NUM_PARTICLES);

    for index in 0..NUM_PARTICLES {
        let angle = (index as f64 / NUM_PARTICLES as f64) * 2.0 * PI;
        let position = Point { x, y, z: 0.0 };
        let velocity = Vector {
            x: angle.cos() * v,
            y: angle.sin() * v / 1.6,
            z: 0.0,
        };
        let character = array::sample(CHARACTERS);
        particles.push(Particle::new(character.to_string(), color, position, velocity, SIMULATION_FPS));
    }

    particles
}

/// Launch several rockets in one burst.
pub fn spawn_salvo(width: i32, height: i32, count: usize) -> Vec<Particle> {
    (0..count).map(|_| spawn_shoot(width, height)).collect()
}
