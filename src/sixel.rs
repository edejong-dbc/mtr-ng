use std::collections::HashMap;

/// Sixel graphics support for enhanced sparklines
#[derive(Debug, Clone)]
pub struct SixelRenderer {
    /// Whether terminal supports Sixel graphics
    pub enabled: bool,
}

impl SixelRenderer {
    /// Create a new Sixel renderer with terminal capability detection
    pub fn new(force_enable: bool) -> Self {
        let enabled = force_enable || Self::detect_sixel_support();
        Self { enabled }
    }

    /// Detect if the current terminal supports Sixel graphics
    pub fn detect_sixel_support() -> bool {
        // Temporarily disable auto-detection to force fallback mode
        // TODO: Re-enable when Sixel implementation is working correctly
        false

        // Check environment variables for Sixel support
        // if let Ok(term) = std::env::var("TERM") {
        //     // Known Sixel-capable terminals
        //     term.contains("xterm") ||
        //     term.contains("sixel") ||
        //     term.contains("mlterm") ||
        //     term.contains("foot") ||
        //     term.contains("wezterm")
        // } else {
        //     false
        // }
    }

    /// Generate sparkline using Sixel graphics
    pub fn generate_sparkline(&self, data: &[f64], _width: u32, _height: u32) -> String {
        // For now, always use fallback until Sixel implementation is perfected
        // TODO: Re-enable Sixel when terminal compatibility issues are resolved
        self.fallback_sparkline(data)

        // if !self.enabled || data.is_empty() {
        //     return self.fallback_sparkline(data);
        // }
        //
        // match self.create_sixel_sparkline(data, width, height) {
        //     Ok(sixel) => {
        //         // For debugging: if the sixel is very short, fall back
        //         if sixel.len() < 10 {
        //             self.fallback_sparkline(data)
        //         } else {
        //             sixel
        //         }
        //     },
        //     Err(_) => self.fallback_sparkline(data), // Fall back on error
        // }
    }

    /// Create Sixel sparkline from data
    pub fn create_sixel_sparkline(
        &self,
        data: &[f64],
        width: u32,
        height: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        if data.is_empty() {
            return Ok(String::new());
        }

        // Create image data
        let mut image_data = vec![0u8; (width * height * 3) as usize];

        // Find min/max for scaling
        let min_val = data.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_val = data.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let range = if max_val > min_val {
            max_val - min_val
        } else {
            1.0
        };

        // Draw sparkline bars
        let x_step = width as f64 / data.len() as f64;

        for (i, &value) in data.iter().enumerate() {
            let x_start = (i as f64 * x_step) as u32;
            let x_end = ((i + 1) as f64 * x_step) as u32;

            let normalized = (value - min_val) / range;
            let bar_height = (normalized * height as f64) as u32;
            let y_start = height - bar_height;

            // Color based on value intensity
            let (r, g, b) = self.value_to_color(normalized);

            // Fill the bar
            for x in x_start..x_end.min(width) {
                for y in y_start..height {
                    let idx = ((y * width + x) * 3) as usize;
                    if idx + 2 < image_data.len() {
                        image_data[idx] = r;
                        image_data[idx + 1] = g;
                        image_data[idx + 2] = b;
                    }
                }
            }
        }

        // Encode as Sixel
        self.encode_to_sixel(&image_data, width, height)
    }

    /// Convert normalized value (0.0-1.0) to RGB color
    fn value_to_color(&self, normalized: f64) -> (u8, u8, u8) {
        if normalized < 0.33 {
            // Green for low values
            let intensity = normalized * 3.0;
            (0, (255.0 * intensity) as u8, 0)
        } else if normalized < 0.66 {
            // Green to yellow transition
            let progress = (normalized - 0.33) * 3.0;
            ((255.0 * progress) as u8, 255, 0)
        } else {
            // Yellow to red for high values
            let progress = (normalized - 0.66) * 3.0;
            (255, (255.0 * (1.0 - progress)) as u8, 0)
        }
    }

    /// Encode image data as Sixel graphics
    fn encode_to_sixel(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut output = String::new();

        // Start Sixel mode
        output.push_str("\x1bPq");

        // Collect unique colors and build palette
        let mut colors = Vec::new();
        let mut color_map: HashMap<[u8; 3], usize> = HashMap::new();

        // Scan image for colors
        for pixel_idx in (0..image_data.len()).step_by(3) {
            if pixel_idx + 2 < image_data.len() {
                let r = image_data[pixel_idx];
                let g = image_data[pixel_idx + 1];
                let b = image_data[pixel_idx + 2];

                if r > 0 || g > 0 || b > 0 {
                    let color = [r, g, b];
                    color_map.entry(color).or_insert_with(|| {
                        let color_idx = colors.len();
                        colors.push(color);
                        color_idx
                    });
                }
            }
        }

        // Define palette colors upfront
        for (color_idx, &[r, g, b]) in colors.iter().enumerate() {
            let r_pct = (r as f32 / 255.0 * 100.0) as u8;
            let g_pct = (g as f32 / 255.0 * 100.0) as u8;
            let b_pct = (b as f32 / 255.0 * 100.0) as u8;
            output.push_str(&format!("#{};2;{};{};{}", color_idx, r_pct, g_pct, b_pct));
        }

        // Process image in 6-pixel high bands
        for band_y in (0..height).step_by(6) {
            // For each color in this band
            for (color_idx, &color) in colors.iter().enumerate() {
                output.push_str(&format!("#{}", color_idx));

                let mut has_pixels = false;

                // Process each column in this band for this color
                for x in 0..width {
                    let mut sixel_char = 0u8;

                    // Check each of the 6 pixels in this column for this color
                    for pixel_y in 0..6 {
                        let y = band_y + pixel_y;
                        if y < height {
                            let pixel_idx = ((y * width + x) * 3) as usize;

                            if pixel_idx + 2 < image_data.len() {
                                let r = image_data[pixel_idx];
                                let g = image_data[pixel_idx + 1];
                                let b = image_data[pixel_idx + 2];

                                if [r, g, b] == color {
                                    sixel_char |= 1 << pixel_y;
                                    has_pixels = true;
                                }
                            }
                        }
                    }

                    // Output sixel character (63 is added to make it printable)
                    output.push(char::from(sixel_char + 63));
                }

                // End of line for this color in this band
                if has_pixels {
                    output.push('$'); // Carriage return
                }
            }

            // Line feed to next band
            if band_y + 6 < height {
                output.push('-');
            }
        }

        // End Sixel mode
        output.push_str("\x1b\\");

        Ok(output)
    }

    /// Fallback sparkline using Unicode block characters
    fn fallback_sparkline(&self, data: &[f64]) -> String {
        if data.is_empty() {
            return String::new();
        }

        // Use existing Unicode block character implementation
        let bars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
        let min_val = data.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max_val = data.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let range = if max_val > min_val {
            max_val - min_val
        } else {
            1.0
        };

        data.iter()
            .map(|&value| {
                let normalized = (value - min_val) / range;
                let index = (normalized * (bars.len() - 1) as f64).round() as usize;
                bars[index.min(bars.len() - 1)]
            })
            .collect()
    }
}
