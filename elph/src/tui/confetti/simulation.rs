use iocraft::prelude::Color;

use super::physics::GRAVITY;

#[derive(Debug, Clone)]
pub struct Point {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone)]
pub struct Vector {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone)]
pub struct Physics {
    pub position: Point,
    pub velocity: Vector,
    pub gravity: f64,
    pub fps: f64,
}

impl Physics {
    pub fn new(position: Point, velocity: Vector, fps: f64) -> Self {
        Self {
            position,
            velocity,
            gravity: GRAVITY,
            fps,
        }
    }

    pub fn update(&mut self) {
        let dt = 1.0 / self.fps;
        self.velocity.y += self.gravity * dt;
        self.position.x += self.velocity.x * dt;
        self.position.y += self.velocity.y * dt;
        self.position.z += self.velocity.z * dt;
    }

    pub fn position(&self) -> Point {
        self.position.clone()
    }

    pub fn velocity(&self) -> Vector {
        self.velocity.clone()
    }
}

pub struct Particle {
    pub ch: String,
    pub color: Color,
    pub tail_char: String,
    pub physics: Physics,
    pub hidden: bool,
    pub shooting: bool,
    pub explosion_call: Option<ExplosionFn>,
}

impl Particle {
    pub fn new(ch: String, color: Color, position: Point, velocity: Vector, fps: f64) -> Self {
        Self {
            ch,
            color,
            tail_char: String::new(),
            physics: Physics::new(position, velocity, fps),
            hidden: false,
            shooting: false,
            explosion_call: None,
        }
    }
}

#[derive(Default)]
pub struct Frame {
    pub width: i32,
    pub height: i32,
}

type ExplosionFn = fn(color: Color, x: f64, y: f64, width: i32, height: i32) -> Vec<Particle>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfettiMode {
    Confetti,
    Firework,
}

pub struct System {
    pub frame: Frame,
    pub particles: Vec<Particle>,
    pub mode: ConfettiMode,
}

impl System {
    pub fn new(mode: ConfettiMode) -> Self {
        Self {
            frame: Frame::default(),
            particles: Vec::new(),
            mode,
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        if self.frame.width == width && self.frame.height == height {
            return;
        }
        self.frame.width = width;
        self.frame.height = height;
        if self.particles.is_empty() {
            self.spawn_burst();
        }
    }

    pub fn spawn_burst(&mut self) {
        match self.mode {
            ConfettiMode::Confetti => {
                let new_particles = super::rain::spawn(self.frame.width, self.frame.height);
                self.particles.extend(new_particles);
            }
            ConfettiMode::Firework => {
                let salvo = super::fireworks::spawn_salvo(self.frame.width, self.frame.height, 3);
                self.particles.extend(salvo);
            }
        }
    }

    pub fn update(&mut self) {
        let mut new_particles = Vec::new();
        let frame_width = self.frame.width;
        let frame_height = self.frame.height;

        self.particles.retain_mut(|particle| {
            let pos = particle.physics.position();

            if !particle.hidden && particle.shooting && particle.physics.velocity().y > -3.0 {
                particle.hidden = true;
                if let Some(explosion_fn) = particle.explosion_call {
                    let explosion_particles = explosion_fn(particle.color, pos.x, pos.y, frame_width, frame_height);
                    new_particles.extend(explosion_particles);
                }
            }

            let pos = particle.physics.position();
            if particle.hidden || pos.x > frame_width as f64 || pos.x < 0.0 || pos.y > frame_height as f64 {
                false
            } else {
                particle.physics.update();
                true
            }
        });

        self.particles.extend(new_particles);
    }

    pub fn visible(&self, particle: &Particle) -> bool {
        let pos = particle.physics.position();
        let x = pos.x as i32;
        let y = pos.y as i32;
        !particle.hidden && y >= 0 && y < self.frame.height - 1 && x >= 0 && x < self.frame.width - 1
    }
}

#[derive(Debug, Clone)]
pub struct RenderedCell {
    pub ch: String,
    pub color: Color,
}

impl System {
    pub fn render_plane(&self) -> Vec<Vec<RenderedCell>> {
        let width = self.frame.width.max(0) as usize;
        let height = self.frame.height.max(0) as usize;
        let mut plane = vec![
            vec![
                RenderedCell {
                    ch: " ".to_string(),
                    color: Color::Reset,
                };
                width
            ];
            height
        ];

        for particle in &self.particles {
            if !self.visible(particle) {
                continue;
            }
            let pos = particle.physics.position();
            let y = pos.y as usize;
            let x = pos.x as usize;
            if y >= plane.len() || x >= plane[y].len() {
                continue;
            }
            plane[y][x] = RenderedCell {
                ch: particle.ch.clone(),
                color: particle.color,
            };

            if particle.shooting {
                let tail_length = (-particle.physics.velocity().y) as i32;
                for offset in 1..tail_length {
                    let tail_y = (pos.y as i32 + offset) as usize;
                    if tail_y > 0 && tail_y < (self.frame.height - 1) as usize && tail_y < plane.len() {
                        plane[tail_y][x] = RenderedCell {
                            ch: particle.tail_char.clone(),
                            color: particle.color,
                        };
                    }
                }
            }
        }

        plane
    }
}
