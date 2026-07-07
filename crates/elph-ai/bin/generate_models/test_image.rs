use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use image::{ImageBuffer, Rgb, RgbImage};

pub struct TestImageOptions {
    pub output: PathBuf,
}

pub fn generate_test_image(options: TestImageOptions) -> Result<()> {
    let width = 200u32;
    let height = 200u32;
    let center_x = 100f32;
    let center_y = 100f32;
    let radius = 50f32;
    let radius_sq = radius * radius;

    let mut img: RgbImage = ImageBuffer::new(width, height);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let dx = x as f32 + 0.5 - center_x;
        let dy = y as f32 + 0.5 - center_y;
        if dx * dx + dy * dy <= radius_sq {
            *pixel = Rgb([255, 0, 0]);
        } else {
            *pixel = Rgb([255, 255, 255]);
        }
    }

    if let Some(parent) = options.output.parent() {
        fs::create_dir_all(parent).context("create test data directory")?;
    }
    img.save(&options.output)
        .with_context(|| format!("write {}", options.output.display()))?;
    println!("Generated test image at: {}", options.output.display());
    Ok(())
}
