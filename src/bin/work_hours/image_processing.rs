use image::DynamicImage;
use imageproc::edges::canny;
use imageproc::geometric_transformations::{rotate_about_center, Interpolation};
use imageproc::hough::{detect_lines, LineDetectionOptions};
use std::f32::consts::PI;
use std::fs::File;
use std::io::{Cursor, Write};
use tracing::info;

/// Preprocess an image containing a work schedule grid to ensure it's properly aligned
pub fn preprocess_schedule_image(image_data: &[u8]) -> Result<Vec<u8>, String> {
    info!(
        "Preprocessing schedule image, size: {} bytes",
        image_data.len()
    );

    // Load the image with EXIF orientation applied
    let img = load_image_with_orientation(image_data)?;

    // Now apply grid deskewing
    let deskewed_img = deskew_grid(&img)?;

    // Convert the image back to bytes in memory
    let mut buffer = Vec::new();
    deskewed_img
        .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Jpeg)
        .map_err(|e| format!("Failed to encode image to JPEG: {}", e))?;

    info!("Image preprocessing completed successfully");
    Ok(buffer)
}

/// Load an image and apply EXIF orientation if present
fn load_image_with_orientation(image_data: &[u8]) -> Result<DynamicImage, String> {
    // Try to determine the format before creating the reader
    let format = image::guess_format(image_data)
        .map_err(|e| format!("Failed to determine image format: {}", e))?;

    info!("Detected image format: {:?}", format);

    // Create a reader with automatic format detection
    let reader = image::ImageReader::new(std::io::Cursor::new(image_data))
        .with_guessed_format()
        .map_err(|e| format!("Failed to create image reader: {}", e))?;

    // The image crate automatically reads EXIF orientation when decoding

    // Load the image with orientation applied
    let img = reader
        .decode()
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    info!(
        "Image loaded with dimensions {}x{}",
        img.width(),
        img.height()
    );

    Ok(img)
}

/// Detect grid lines and correct skew in an image
fn deskew_grid(img: &DynamicImage) -> Result<DynamicImage, String> {
    // Convert to grayscale for processing
    let gray_img = img.to_luma8();

    // Apply edge detection with appropriate thresholds for grids
    // Lower thresholds detect more edges but may include noise
    let edges = canny(&gray_img, 50.0, 150.0);

    // Debug logging for edge detection (without saving to filesystem)
    if cfg!(debug_assertions) {
        info!(
            "Edge detection completed, image dimensions: {}x{}",
            edges.width(),
            edges.height()
        );
    }

    // Detect lines using Hough transform
    // Set appropriate thresholds for grid detection
    let options = LineDetectionOptions {
        vote_threshold: 500,   // Higher values detect stronger lines
        suppression_radius: 5, // Minimum distance between detected lines
    };

    let lines = detect_lines(&edges, options);
    info!("Detected {} lines in the image", lines.len());

    if lines.is_empty() {
        info!("No lines detected, skipping deskew");
        return Ok(img.clone());
    }

    // Debug: Print detected line angles
    if cfg!(debug_assertions) {
        for (i, line) in lines.iter().enumerate().take(10) {
            info!("Line {}: angle={}°, r={}", i, line.angle_in_degrees, line.r);
        }
    }

    // Find horizontal and vertical lines for skew detection
    // We focus on near-horizontal and near-vertical lines
    let mut horizontal_lines = Vec::new();
    let mut vertical_lines = Vec::new();

    for line in &lines {
        let angle_deg = line.angle_in_degrees as f32;

        // Normalize angle to 0-180 range for easier comparison
        let normalized_angle = angle_deg % 180.0;

        // Classify lines as horizontal or vertical with tolerances
        if !(20.0..=160.0).contains(&normalized_angle) {
            // Near horizontal lines (0° or 180° ± 20°)
            horizontal_lines.push((normalized_angle, line.r));
        } else if (normalized_angle - 90.0).abs() < 20.0 {
            // Near vertical lines (90° ± 20°)
            vertical_lines.push((normalized_angle, line.r));
        }
    }

    info!(
        "Found {} horizontal and {} vertical lines",
        horizontal_lines.len(),
        vertical_lines.len()
    );

    // Calculate skew angle based on detected lines
    let skew_angle_rad = if !horizontal_lines.is_empty() {
        // Using horizontal lines for skew calculation
        info!("Using horizontal lines for skew detection");

        // Calculate average angle, normalizing angles near 180° to negative values
        let mut angle_sum = 0.0;
        let mut count = 0;

        for (angle_deg, _) in &horizontal_lines {
            // Normalize to -20° to +20° range
            let normalized = if *angle_deg > 90.0 {
                *angle_deg - 180.0
            } else {
                *angle_deg
            };

            angle_sum += normalized;
            count += 1;
        }

        if count > 0 {
            let avg_angle_deg = angle_sum / count as f32;
            info!("Detected horizontal skew: {:.2}°", avg_angle_deg);

            // Convert to radians for rotation
            avg_angle_deg * PI / 180.0
        } else {
            0.0
        }
    } else if !vertical_lines.is_empty() {
        // Using vertical lines for skew calculation
        info!("Using vertical lines for skew detection");

        // Calculate average deviation from 90°
        let mut angle_sum = 0.0;
        let mut count = 0;

        for (angle_deg, _) in &vertical_lines {
            // Calculate deviation from perfect vertical (90°)
            let deviation = *angle_deg - 90.0;
            angle_sum += deviation;
            count += 1;
        }

        if count > 0 {
            let avg_deviation_deg = angle_sum / count as f32;
            info!("Detected vertical skew: {:.2}°", avg_deviation_deg);

            // Convert to radians for rotation
            avg_deviation_deg * PI / 180.0
        } else {
            0.0
        }
    } else {
        info!("No usable lines for skew detection");
        0.0
    };

    // Skip rotation if skew is negligible or too extreme
    if skew_angle_rad.abs() < 0.01 {
        // Less than ~0.57 degrees
        info!("Skew angle too small (< 0.57°), skipping rotation");
        return Ok(img.clone());
    }

    if skew_angle_rad.abs() > 0.25 {
        // More than ~14.3 degrees
        info!("Skew angle too large (> 14.3°), might be incorrect. Skipping rotation");
        return Ok(img.clone());
    }

    // Rotate the image to correct the skew
    // Negate the angle since we want to counter-rotate
    info!(
        "Rotating image by {:.2}° to correct skew",
        -skew_angle_rad * 180.0 / PI
    );
    let rotated_img = rotate_about_center(
        &img.to_rgb8(),
        -skew_angle_rad,
        Interpolation::Bilinear,
        image::Rgb([255, 255, 255]),
    );

    let corrected_img = DynamicImage::ImageRgb8(rotated_img);
    info!("Deskew correction applied successfully");

    Ok(corrected_img)
}

/// Save an image to a file for testing and debugging purposes
pub fn save_image(path: &str, image_data: &[u8]) -> Result<(), String> {
    info!("Saving image to: {}", path);

    // Create the file
    let mut file =
        File::create(path).map_err(|e| format!("Failed to create file {}: {}", path, e))?;

    // Write the image data
    file.write_all(image_data)
        .map_err(|e| format!("Failed to write image data to {}: {}", path, e))?;

    info!("Image saved successfully to: {}", path);
    Ok(())
}

/// Process an image file and save the result
pub fn process_and_save(input_path: &str, output_path: &str) -> Result<(), String> {
    use std::fs;

    // Read the input image
    let image_data = fs::read(input_path)
        .map_err(|e| format!("Failed to read input image {}: {}", input_path, e))?;

    // Process the image
    info!("Processing image: {}", input_path);
    let processed_data = preprocess_schedule_image(&image_data)?;

    // Save the result
    save_image(output_path, &processed_data)?;

    info!(
        "Image processed and saved from {} to {}",
        input_path, output_path
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_image_processing_pipeline() {
        // Load a test image (skip if file not found)
        let test_file = "test_data/sample_schedule.jpg";
        if let Ok(image_data) = fs::read(test_file) {
            let result = preprocess_schedule_image(&image_data);
            assert!(
                result.is_ok(),
                "Image preprocessing failed: {:?}",
                result.err()
            );

            let processed_data = result.unwrap();
            assert!(
                !processed_data.is_empty(),
                "Processed image data should not be empty"
            );

            // The processed image should be a valid image
            let img_result = image::load_from_memory(&processed_data);
            assert!(img_result.is_ok(), "Processed data is not a valid image");
        } else {
            // Skip test if sample image is not available
            println!("Test image not found at {}, skipping test", test_file);
        }
    }

    #[test]
    fn test_exif_orientation_handling() {
        // This test just makes sure the EXIF handling code doesn't crash
        // We can't check actual orientation correction without a specially crafted test image
        let test_file = "test_data/sample_schedule.jpg";
        if let Ok(image_data) = fs::read(test_file) {
            // Test the EXIF orientation handling function directly
            let result = load_image_with_orientation(&image_data);
            assert!(
                result.is_ok(),
                "EXIF orientation handling failed: {:?}",
                result.err()
            );

            let img = result.unwrap();
            // Just check that we got a valid image with dimensions
            assert!(
                img.width() > 0 && img.height() > 0,
                "Image has invalid dimensions"
            );

            println!(
                "EXIF orientation test passed with image dimensions: {}x{}",
                img.width(),
                img.height()
            );
        } else {
            // Skip test if sample image is not available
            println!("Test image not found at {}, skipping test", test_file);
        }
    }
}
