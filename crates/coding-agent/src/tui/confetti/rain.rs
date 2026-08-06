use iocraft::prelude::Color;
use rand::RngExt;

use super::array;
use super::physics::SIMULATION_FPS;
use super::simulation::{Particle, Point, Vector};

const NUM_PARTICLES: usize = 140;

const COLORS: &[&str] = &[
    "#a864fd", "#29cdff", "#78ff44", "#ff718d", "#fdff6a", "#ff9f43", "#ff6bcb", "#54a0ff", "#feca57", "#48dbfb",
];
const CHARACTERS: &[&str] = &["█", "▓", "▒", "✦", "✧", "★", "●", "♦", "◆", "▄", "▀", "░"];

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

pub fn spawn(width: i32, _height: i32) -> Vec<Particle> {
    let mut rng = rand::rng();
    let mut particles = Vec::with_capacity(NUM_PARTICLES);

    for _ in 0..NUM_PARTICLES {
        let color = parse_color(array::sample(COLORS));
        let character = array::sample(CHARACTERS);
        let origin_x = rng.random::<f64>() * width.max(1) as f64;

        let position = Point {
            x: origin_x,
            y: -rng.random::<f64>() * 4.0,
            z: 0.0,
        };

        let velocity = Vector {
            x: (rng.random::<f64>() - 0.5) * 180.0,
            y: rng.random::<f64>() * 90.0 + 40.0,
            z: 0.0,
        };

        particles.push(Particle::new(character.to_string(), color, position, velocity, SIMULATION_FPS));
    }

    particles
}
