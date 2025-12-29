use image::{Rgba, RgbaImage};

const MAX_TONEMAP_DIMENSION: u32 = 16384;
const MAX_TONEMAP_PIXELS: usize = 256 * 1024 * 1024;

/// Simple Reinhard tonemapping for HDR to SDR conversion.
/// This is the same approach used by ShareX and Xbox Game Bar.
fn reinhard(v: f32) -> f32 {
    v / (1.0 + v)
}

/// Convert PQ (Perceptual Quantizer) encoded value to linear light.
/// Used for HDR10 content.
pub fn pq_to_linear(pq: f32) -> f32 {
    let m1: f32 = 0.159_301_76;
    let m2: f32 = 78.84375;
    let c1: f32 = 0.8359375;
    let c2: f32 = 18.851_563;
    let c3: f32 = 18.6875;

    let pq_clamped = pq.clamp(0.0, 1.0);
    let pq_pow = pq_clamped.powf(1.0 / m2);
    let numerator = (pq_pow - c1).max(0.0);
    let denominator = c2 - c3 * pq_pow;

    if denominator <= 0.0 {
        0.0
    } else {
        10000.0 * (numerator / denominator).powf(1.0 / m1)
    }
}

/// Convert HLG (Hybrid Log-Gamma) to linear light.
pub fn hlg_to_linear(hlg: f32) -> f32 {
    let b: f32 = 0.28466892;
    let c: f32 = 0.5599107;

    if hlg <= 0.5 {
        (hlg * hlg) / 3.0
    } else {
        ((hlg - c).exp() + b) / 12.0
    }
}

/// Convert linear light to sRGB gamma.
fn linear_to_srgb(linear: f32) -> f32 {
    if linear <= 0.0031308 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

/// Convert scRGB (linear HDR) to SDR using Reinhard tonemapping.
/// sdr_white_level is the display's SDR white level in nits (typically 80-400).
pub fn scrgb_to_sdr(hdr_data: &[f32], width: u32, height: u32, sdr_white_level: f32) -> RgbaImage {
    if width == 0 || height == 0 || width > MAX_TONEMAP_DIMENSION || height > MAX_TONEMAP_DIMENSION {
        return RgbaImage::new(1, 1);
    }

    let pixels_count = match (width as usize).checked_mul(height as usize) {
        Some(c) if c <= MAX_TONEMAP_PIXELS => c,
        _ => return RgbaImage::new(1, 1),
    };

    if hdr_data.len() < pixels_count * 4 {
        return RgbaImage::new(width, height);
    }

    let mut result = RgbaImage::new(width, height);

    // Normalize to SDR white level (scRGB 1.0 = 80 nits, but display may show SDR white at different level)
    let white_scale = 80.0 / sdr_white_level.max(80.0);

    for y in 0..height {
        for x in 0..width {
            let idx = ((y as usize) * (width as usize) + (x as usize)) * 4;

            let r = hdr_data[idx];
            let g = hdr_data[idx + 1];
            let b = hdr_data[idx + 2];
            let a = hdr_data[idx + 3];

            // Scale by white level and apply Reinhard
            let r_scaled = if r.is_finite() { r * white_scale } else { 0.0 };
            let g_scaled = if g.is_finite() { g * white_scale } else { 0.0 };
            let b_scaled = if b.is_finite() { b * white_scale } else { 0.0 };

            let r_tm = reinhard(r_scaled.max(0.0));
            let g_tm = reinhard(g_scaled.max(0.0));
            let b_tm = reinhard(b_scaled.max(0.0));

            // Convert to sRGB
            let r_out = (linear_to_srgb(r_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let g_out = (linear_to_srgb(g_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let b_out = (linear_to_srgb(b_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let a_out = if a.is_finite() { (a * 255.0).clamp(0.0, 255.0) as u8 } else { 255 };

            result.put_pixel(x, y, Rgba([r_out, g_out, b_out, a_out]));
        }
    }

    result
}

/// Convert HDR10 (PQ encoded) to SDR using Reinhard tonemapping.
pub fn hdr10_to_sdr(pq_data: &[u16], width: u32, height: u32, sdr_white_level: f32) -> RgbaImage {
    if width == 0 || height == 0 || width > MAX_TONEMAP_DIMENSION || height > MAX_TONEMAP_DIMENSION {
        return RgbaImage::new(1, 1);
    }

    let pixels_count = match (width as usize).checked_mul(height as usize) {
        Some(c) if c <= MAX_TONEMAP_PIXELS => c,
        _ => return RgbaImage::new(1, 1),
    };

    if pq_data.len() < pixels_count * 4 {
        return RgbaImage::new(width, height);
    }

    let mut result = RgbaImage::new(width, height);

    // HDR10 reference white is 203 nits, scale relative to display SDR white
    let white_scale = 203.0 / sdr_white_level.max(80.0) / 10000.0;

    for y in 0..height {
        for x in 0..width {
            let idx = ((y as usize) * (width as usize) + (x as usize)) * 4;

            // Decode PQ to linear nits
            let pq_r = pq_data[idx] as f32 / 65535.0;
            let pq_g = pq_data[idx + 1] as f32 / 65535.0;
            let pq_b = pq_data[idx + 2] as f32 / 65535.0;
            let a = pq_data[idx + 3] as f32 / 65535.0;

            let linear_r = pq_to_linear(pq_r) * white_scale;
            let linear_g = pq_to_linear(pq_g) * white_scale;
            let linear_b = pq_to_linear(pq_b) * white_scale;

            // Apply Reinhard
            let r_tm = reinhard(linear_r);
            let g_tm = reinhard(linear_g);
            let b_tm = reinhard(linear_b);

            // Convert to sRGB
            let r_out = (linear_to_srgb(r_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let g_out = (linear_to_srgb(g_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let b_out = (linear_to_srgb(b_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let a_out = (a * 255.0).clamp(0.0, 255.0) as u8;

            result.put_pixel(x, y, Rgba([r_out, g_out, b_out, a_out]));
        }
    }

    result
}

/// Convert HLG to SDR using Reinhard tonemapping.
pub fn hlg_to_sdr(hlg_data: &[u8], width: u32, height: u32, sdr_white_level: f32) -> RgbaImage {
    if width == 0 || height == 0 || width > MAX_TONEMAP_DIMENSION || height > MAX_TONEMAP_DIMENSION {
        return RgbaImage::new(1, 1);
    }

    let pixels_count = match (width as usize).checked_mul(height as usize) {
        Some(c) if c <= MAX_TONEMAP_PIXELS => c,
        _ => return RgbaImage::new(1, 1),
    };

    if hlg_data.len() < pixels_count * 4 {
        return RgbaImage::new(width, height);
    }

    let mut result = RgbaImage::new(width, height);
    let white_scale = 80.0 / sdr_white_level.max(80.0);

    for y in 0..height {
        for x in 0..width {
            let idx = ((y as usize) * (width as usize) + (x as usize)) * 4;

            let hlg_r = hlg_data[idx] as f32 / 255.0;
            let hlg_g = hlg_data[idx + 1] as f32 / 255.0;
            let hlg_b = hlg_data[idx + 2] as f32 / 255.0;
            let a = hlg_data[idx + 3] as f32 / 255.0;

            let linear_r = hlg_to_linear(hlg_r) * white_scale;
            let linear_g = hlg_to_linear(hlg_g) * white_scale;
            let linear_b = hlg_to_linear(hlg_b) * white_scale;

            let r_tm = reinhard(linear_r);
            let g_tm = reinhard(linear_g);
            let b_tm = reinhard(linear_b);

            let r_out = (linear_to_srgb(r_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let g_out = (linear_to_srgb(g_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let b_out = (linear_to_srgb(b_tm) * 255.0).clamp(0.0, 255.0) as u8;
            let a_out = (a * 255.0).clamp(0.0, 255.0) as u8;

            result.put_pixel(x, y, Rgba([r_out, g_out, b_out, a_out]));
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reinhard_basic() {
        assert!((reinhard(1.0) - 0.5).abs() < 0.001);
        assert!((reinhard(0.0) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_scrgb_to_sdr() {
        let hdr_data = vec![1.0f32, 1.0, 1.0, 1.0];
        let result = scrgb_to_sdr(&hdr_data, 1, 1, 80.0);
        assert_eq!(result.width(), 1);
        let pixel = result.get_pixel(0, 0);
        assert!(pixel[0] > 100 && pixel[0] < 200);
    }

    #[test]
    fn test_hdr10_to_sdr() {
        let pq_data: Vec<u16> = vec![32768, 32768, 32768, 65535];
        let result = hdr10_to_sdr(&pq_data, 1, 1, 80.0);
        assert_eq!(result.width(), 1);
    }
}
